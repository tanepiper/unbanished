//! The character actions (`Action`): what each one does when it starts, each update, and when it
//! stops or is aborted, and dropping off what a citizen carries.

mod start;

use super::*;

/// One task.
#[derive(Debug, Clone)]
pub enum Action {
    /// `MoveTo`: walk to a target.
    MoveTo(Box<MoveTo>),
    // vtable 0x14015ea70
    /// `PlayAnimation`: play `anim` for `duration`.
    PlayAnimation {
        /// The clip.
        anim: usize,
        /// How long it plays, in microseconds.
        duration: i64,
        /// The time played so far.
        elapsed: i64,
    },
    // vtable 0x1401662c0
    /// `PerformWork`: face `facing` (if within ±180), play `anim` for `duration`.
    PerformWork {
        /// The heading to face, in degrees.
        facing: f32,
        /// The work clip.
        anim: usize,
        /// How long the work takes, in microseconds.
        duration: i64,
        /// The time worked so far.
        elapsed: i64,
        // +0x40; +0xac
        /// Decrement the citizen's counter on start.
        consume: bool,
    },
    // vtable 0x1401664e0
    /// `SendComponentMessage`: deliver a message on start.
    SendMessage(Message),
    // vtable 0x140165f80
    /// `CompleteRoad`: the cell becomes its road (or ground) on start.
    ///
    /// Original: `FUN_1400caea0`.
    CompleteRoad {
        /// The cell.
        cell: i32,
        /// The cell type it gets.
        ty: u8,
        /// The road kind built there.
        built: u8,
    },
    /// Reserve at construction; consume on start.
    ///
    /// Original: `FUN_1400cb880`, `FUN_1400cb980`.
    ConsumeStoredRawMaterial {
        /// The storage.
        storage: usize,
        /// What it uses: `(raw material, count)`.
        items: Vec<(usize, i32)>,
        /// Whether the goods were reserved.
        reserved: bool,
    },
    /// Reserve at construction; transfer on start.
    ///
    /// Original: `FUN_1400cf9f0`, `FUN_1400cf700`.
    PickupStoredRawMaterial {
        /// The storage.
        storage: usize,
        /// What it takes: `(raw material, count)`.
        items: Vec<(usize, i32)>,
        /// Whether it may take less than asked.
        partial: bool,
    },
    // vtable 0x14015ead0
    /// `ShowTool`: attach tool mesh `n` (0..10), the carried goods (−2), or nothing (−1). The town
    /// draws it on the citizen's `tool` node (`StartingTown::model_instances`).
    ///
    /// Original: `FUN_140043070`, `FUN_14011c880`.
    ShowTool(i32),
    // vtable 0x140165fe0
    /// `CompleteTask`: tell the job manager the task is done or abandoned.
    ///
    /// Original: `FUN_1400cb270`, `FUN_1400cb010`; abandoned `FUN_1400caf90`.
    CompleteTask {
        /// The work's id.
        work: i32,
        /// The task's parameter.
        param: i32,
    },
    // vtable 0x140165f20
    /// `AnimateNaturalResource`: play the resource's animation `anim`; with `wait`, run until it
    /// ends.
    ///
    /// Original: `FUN_1400caca0`, `FUN_1400caad0`, `FUN_1400caa80`.
    AnimateResource {
        /// The resource.
        entity: usize,
        /// The clip.
        anim: usize,
        /// Whether the action waits until the clip ends.
        wait: bool,
    },
    // vtable 0x140166470
    /// `RemoveNaturalResource`: remove the resource and leave its pile; abort releases the claim
    /// (flag `0x100000`).
    ///
    /// Original: `FUN_1400d02c0`, `FUN_1400cffe0`, `FUN_1400cff70`.
    RemoveResource {
        /// The resource.
        entity: usize,
        /// The removal flag passed to the resource.
        flag: bool,
    },
    // vtable 0x140166330
    /// `PickupRawMaterial`: take the whole pile into the carried storage and destroy it.
    ///
    /// Original: `FUN_1400cf040`, `FUN_1400ceda0`.
    PickupPile {
        /// The pile.
        entity: usize,
    },
    // vtable 0x140166130
    /// `CreateRawMaterial`: leave the output as piles near `cell`, `batch` weight each (the
    /// worker's capacity).
    ///
    /// Original: `FUN_1400cc670`, `FUN_1400cc010`.
    CreateRawMaterial {
        /// The template of the output piles.
        template: usize,
        /// The cell they are left near.
        cell: i32,
        /// The weight of each pile.
        batch: i32,
        /// The workplace that made them.
        workplace: Option<usize>,
        // +0x4c
        /// The yield is scaled by this (1 for production; a dock's share of the fish).
        factor: f32,
    },
    // vtable 0x140166200
    /// `HuntAnimal`, attached to a hunter's walk or wait: running until its notify (run by the
    /// citizens' update) has started an attack.
    ///
    /// Original: `FUN_1400cd8d0`, `FUN_1400cd2b0`.
    HuntAnimal {
        /// The hunter's lodge.
        lodge: usize,
        /// Whether the attack has started.
        done: bool,
    },
    // vtable 0x140166400
    /// `RemoveAnimal`.
    ///
    /// Original: `FUN_1400cfcc0`.
    RemoveAnimal {
        /// The animal.
        entity: usize,
    },
    // vtable 0x1401660c0
    /// `CreateNaturalResource`: plant `template` on `cell`, which must hold no object and
    /// `occupant` as its occupant.
    ///
    /// Original: `FUN_1400cbef0`.
    CreateNaturalResource {
        /// The resource's template.
        template: usize,
        /// The cell's required occupant.
        occupant: Option<usize>,
        /// The cell.
        cell: i32,
    },
    // vtable 0x1401661a0; +0x20
    /// `DropOffRawMaterial`: move up to `count` carried units into `storage`; with `redirect` a
    /// storage that cannot take them sends the citizen on to another, or the goods are left on the
    /// ground.
    ///
    /// Original: `FUN_1400cd1a0`, `FUN_1400cc7d0`.
    DropOff {
        /// The storage.
        storage: usize,
        /// The raw material.
        material: usize,
        /// The most units to move.
        count: i32,
        /// Whether a storage that cannot take them sends the citizen on.
        redirect: bool,
    },
}

/// `FUN_1400ccd90`: `n` units of `m` leave the citizen for a loose pile on the ground.
pub(super) fn drop_pile(world: &mut World, m: usize, n: i32) {
    if n == 0 {
        return;
    }
    world.messages.push(Message {
        component: Component::DropPile,
        message: 0,
        entity: None,
        a: m as i32,
        b: n,
    });
    world.carried.remove(m, n);
}

/// `FUN_1400ccaf0`: as many as carried, up to `count`, into `target` when they fit; else with
/// `redirect`, the nearest storage that takes them (the action now points there and a walk
/// is pushed) or, with none, a loose pile. True when the goods were left on the ground.
pub(super) fn drop_off_one(
    ch: &Character,
    world: &mut World,
    target: usize,
    storage: &mut usize,
    m: usize,
    count: i32,
    redirect: bool,
) -> bool {
    let carried = world.carried.available(m);
    let n = if 0 <= count {
        carried.min(count)
    } else {
        carried
    };
    if 0 < n && world.storages[target].fits(world.materials, m, n) {
        world.storages[target].add(m, n);
        world.carried.remove(m, n);
        return false;
    }
    if !redirect {
        return false;
    }
    let cell = world.cell(ch.position);
    let found = match world.storage_search.as_mut() {
        Some(search) => search(world.storages, world.rng, m, n, cell),
        None => None,
    };
    match found {
        Some((s, point)) => {
            *storage = s;
            let walk = MoveTo::new(point, 1, 0, crate::pathfind::WALKABLE, 0, -1.0, world.rng);
            world.pushes.push(Action::move_to(walk));
            false
        }
        None => {
            drop_pile(world, m, n);
            true
        }
    }
}

impl Action {
    /// A `MoveTo` action.
    pub fn move_to(m: MoveTo) -> Action {
        Action::MoveTo(Box::new(m))
    }

    pub(super) fn update(
        &mut self,
        ch: &mut Character,
        world: &mut World,
        group: &AnimationGroup,
        dt: i64,
    ) -> Status {
        match self {
            Action::MoveTo(m) => m.update(ch, world, group),
            Action::PlayAnimation {
                duration, elapsed, ..
            } => {
                *elapsed += dt;
                if *elapsed < *duration {
                    Status::Running
                } else {
                    Status::Done
                }
            }
            Action::PerformWork {
                duration, elapsed, ..
            } => {
                if *duration <= *elapsed {
                    // `FUN_14011bb20`: an injury fails the work.
                    return if world.injury_check() {
                        Status::Failed
                    } else {
                        Status::Done
                    };
                }
                *elapsed += dt;
                Status::Running
            }
            Action::AnimateResource { entity, .. } => {
                if world.animating.get(*entity).copied().unwrap_or(false) {
                    Status::Running
                } else {
                    Status::Done
                }
            }
            // The base update returns 0.
            Action::RemoveResource { .. }
            | Action::PickupPile { .. }
            | Action::CreateRawMaterial { .. }
            | Action::CompleteRoad { .. }
            | Action::CreateNaturalResource { .. }
            | Action::RemoveAnimal { .. } => Status::Done,
            // `FUN_1400cd710`.
            Action::HuntAnimal { done, .. } => {
                if *done {
                    Status::Done
                } else {
                    Status::Running
                }
            }
            Action::SendMessage(_)
            | Action::ConsumeStoredRawMaterial { .. }
            | Action::PickupStoredRawMaterial { .. }
            | Action::ShowTool(_)
            | Action::CompleteTask { .. }
            | Action::DropOff { .. } => Status::Done,
        }
    }

    /// Slot `+0x38` (stop) and `+0x40` (abort): MoveTo and PerformWork return to clip 0.
    pub(super) fn stop(&mut self, ch: &mut Character, group: &AnimationGroup) {
        match self {
            Action::MoveTo(m) => ch.play(m.stand, group, 1.0),
            Action::PerformWork { .. } => ch.play(0, group, 1.0),
            _ => {}
        }
    }

    /// FUN_1400cb300 / FUN_1400cf140: abort releases pending reservations.
    pub(super) fn abort(
        &mut self,
        ch: &mut Character,
        group: &AnimationGroup,
        storages: &mut [Storage],
    ) {
        match self {
            Action::ConsumeStoredRawMaterial {
                storage,
                items,
                reserved: true,
            }
            | Action::PickupStoredRawMaterial { storage, items, .. } => {
                for &(material, count) in items.iter() {
                    storages[*storage].unreserve(material, count);
                }
            }
            _ => {}
        }
        self.stop(ch, group);
    }
}
