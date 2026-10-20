//! Walking to a place (`MoveToAction`): asking for a path, following its cells and facing the next
//! one.

use super::*;

// vtable 0x140166260
/// `MoveTo`.
///
/// Original: `FUN_1400ce640`.
#[derive(Debug, Clone)]
pub struct MoveTo {
    // +0x38
    /// Destination.
    pub target: [f32; 3],
    // +0x5c
    /// Allowed cell types.
    pub flags: u32,
    // +0x468 / +0x46c
    /// Walk and stand clips.
    pub walk: usize,
    /// The clip played while standing.
    pub stand: usize,
    // +0x470
    /// Pathfinder cost table.
    pub mode: u32,
    // +0x478
    /// Speed factor (random 0.9–1.1 when constructed with a negative value).
    pub speed: f32,
    // +0x474
    /// Walking behind another character.
    ///
    /// Original: `FUN_1400cde40`.
    pub following: bool,
    /// Pending request result (`+0x20/+0x24`); available on the next update.
    pending: Option<Option<Vec<i32>>>,
    /// `+0x60` index, `+0x64` count, `+0x68` cells.
    index: i32,
    count: i32,
    path: [i32; PATH_CELLS],
    /// Set by [`MoveTo::restored`]: the first update plays the walk clip again.
    resume: bool,
}

impl MoveTo {
    /// A `MoveTo` action.
    ///
    /// Original: `FUN_1400ce640`.
    pub fn new(
        target: [f32; 3],
        walk: usize,
        stand: usize,
        flags: u32,
        mode: u32,
        speed: f32,
        rng: &mut Random,
    ) -> MoveTo {
        MoveTo {
            target,
            flags,
            walk,
            stand,
            mode,
            speed: if speed >= 0.0 {
                speed
            } else {
                rng.rand_float_tol() * 0.1 + 1.0
            },
            following: false,
            pending: None,
            index: -1,
            count: 0,
            path: [0; PATH_CELLS],
            resume: false,
        }
    }

    // +0x64; +0x68; +0x60
    /// Plan from `from` to the target. A `MoveTo` from a save keeps its way (its cells and the next
    /// one) as it was.
    /// A path request in flight is not saved; one with no way restarts.
    ///
    /// Original: `FUN_1400ce390`, `FUN_1400ce0e0`.
    #[allow(clippy::too_many_arguments)]
    pub fn restored(
        target: [f32; 3],
        walk: usize,
        stand: usize,
        flags: u32,
        mode: u32,
        speed: f32,
        following: bool,
        index: i32,
        path: &[i32],
    ) -> MoveTo {
        let n = path.len().min(PATH_CELLS);
        let mut cells = [0; PATH_CELLS];
        cells[..n].copy_from_slice(&path[..n]);
        MoveTo {
            target,
            flags,
            walk,
            stand,
            mode,
            speed,
            following,
            pending: None,
            index,
            count: n as i32,
            path: cells,
            resume: true,
        }
    }

    // +0x60; +0x64; +0x68
    /// What a save holds of the way: the next cell's index and the cells.
    pub fn saved_way(&self) -> (i32, &[i32]) {
        (
            self.index,
            &self.path[..(self.count.max(0) as usize).min(PATH_CELLS)],
        )
    }

    pub(super) fn request(&mut self, from: [f32; 3], world: &mut World) {
        let a = world.cell(from);
        let b = world.cell(self.target);
        self.index = -1;
        if a == b {
            self.count = 1;
            self.path[0] = a;
            self.index = 0;
            return;
        }
        world.pathfinder.set_mode(self.mode);
        self.pending = Some(pathfind::request(
            world.pathfinder,
            world.types,
            world.regions,
            self.flags,
            a,
            b,
        ));
    }

    pub(super) fn face_waypoint(&self, ch: &mut Character, world: &World) {
        let cell = self.path[(self.index as usize).min(PATH_CELLS - 1)];
        ch.face(world.cell_centre(cell));
    }

    /// `FUN_1400cdf80` (start).
    pub(super) fn start(
        &mut self,
        ch: &mut Character,
        world: &mut World,
        group: &AnimationGroup,
    ) -> Status {
        let d = [
            ch.position[0] - self.target[0],
            ch.position[1] - self.target[1],
        ];
        if (d[0] * d[0] + d[1] * d[1]).sqrt() < ARRIVE {
            return Status::Done;
        }
        self.request(ch.position, world);
        if self.index < 0 {
            return match self.pending {
                Some(Some(_)) => Status::Running,
                _ => Status::Failed,
            };
        }
        self.face_waypoint(ch, world);
        ch.play(self.walk, group, 1.0);
        Status::Running
    }

    /// `FUN_1400cda40` (update).
    pub(super) fn update(
        &mut self,
        ch: &mut Character,
        world: &mut World,
        group: &AnimationGroup,
    ) -> Status {
        // Deviation: a walk restored from a save plays its walk clip again and faces its next
        // cell on its first update. The original restores the clip with the animation block
        // (`AnimationManager::Serialize`, `FUN_18005d130`), which the port does not decode
        // yet; without a walking clip the body has no root motion, never reaches the next
        // cell, and the citizen stands still for good.
        if std::mem::take(&mut self.resume) && self.index >= 0 && self.index < self.count {
            self.face_waypoint(ch, world);
            ch.play(self.walk, group, 1.0);
        }
        if self.index != -1 {
            let here = world.cell(ch.position);
            if world.cell(self.target) == here {
                let d = [
                    ch.position[0] - self.target[0],
                    ch.position[1] - self.target[1],
                ];
                if (d[0] * d[0] + d[1] * d[1]).sqrt() >= ARRIVE {
                    ch.face(self.target);
                    return Status::Running;
                }
                return Status::Done;
            }
            if self.count < 2 {
                return Status::Running;
            }
            if self.index < self.count {
                let t = world.types[here as usize].0;
                let scale = if self.following {
                    ch.desc.follow_speed
                } else if t == 1 || t == 9 {
                    ch.desc.road_speed
                } else if t == 0 {
                    ch.desc.type0_speed
                } else {
                    1.0
                };
                ch.speed = scale * self.speed;
                if here != self.path[self.index as usize] {
                    return Status::Running;
                }
                self.index += 1;
                self.face_waypoint(ch, world);
                return Status::Running;
            }
            ch.play(self.stand, group, 1.0);
            self.index = -1;
            return Status::Running;
        }
        if let Some(result) = self.pending.take() {
            let Some(path) = result else {
                return Status::Failed;
            };
            let n = path.len().min(PATH_CELLS);
            self.path[..n].copy_from_slice(&path[..n]);
            self.count = n as i32;
            if n == 0 {
                return Status::Failed;
            }
            self.index = 1;
            self.face_waypoint(ch, world);
            ch.play(self.walk, group, 1.0);
            return Status::Running;
        }
        match self.start(ch, world, group) {
            Status::Failed => Status::Failed,
            _ => Status::Running,
        }
    }
}
