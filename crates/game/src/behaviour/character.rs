//! CharacterComponent and the `Action` task stack it runs. See docs/CITIZEN-AI.md.
//!
//! - Slot (bit `0x04` phase, every iteration): run the action stack, then turn toward the target
//!   heading, move by the animation's root motion times the speed, and drop onto the ground.
//! - Slot (bit `0x08` bucket phase): notify the running chain (action slot) on the started actions
//!   of the running chain. MoveTo's sets whether it walks behind another character, at the
//!   description's follow speed ([`ActionStack::notify_moves`]); HuntAnimal's looks for prey.
//!
//! Actions (registered `Action` types, vtable slots start, stop, abort) return [`Status`]. The
//! stack is an array of chains; the top chain is started once, then updated each run. `Done` pops
//! it and continues with the next chain in the same run, `Failed` clears everything, and when the
//! stack empties the "on empty" chain runs once.
//!
//! Original: `FUN_14010f1f0`, `FUN_14010e990`, `FUN_1400ca120`, `FUN_14010e950`, `FUN_1400ca450`,
//! `FUN_1400cde40`, `FUN_1400cd2b0`, `FUN_1400ca080`.
// vtable 0x140168790; +0x40; +0x48; +0x30; +0x28 update; +0x58; +0x20; +0x38

use banished_formats::game::CharacterDescription;
use banished_formats::types::AnimationGroup;

use crate::behaviour::animation::AnimationStack;
use crate::pathfind::{self, Pathfinder};
use crate::random::Random;
use crate::scheduler::{seconds, ticks};
use crate::terrain::{CellType, HeightGrid};
use crate::work::stock::{consume_stored, pickup_stored, RawMaterial, Storage};

// DAT_1401787d4
/// Arrival distance.
pub const ARRIVE: f32 = 0.05;
/// `DAT_1401787e4`: turns smaller than this snap to the target heading.
const SNAP_DEGREES: f32 = 0.1;
/// MoveTo's path buffer (`+0x68`, 256 cells, `FUN_1400ac220`).
const PATH_CELLS: usize = 0x100;
/// Cell type whose surface is at height 0 (`FUN_14010e990`, type `0x0b`).
const FLAT_TYPE: u8 = 11;

/// The result of an action's step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// The action finished.
    Done = 0,
    /// The action goes on.
    Running = 1,
    /// The action failed; the stack is abandoned.
    Failed = 2,
}

// DAT_1401cf0e8; DAT_1401cf158
/// Which component a `SendMessage` targets (CitizenAI, Citizen).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Component {
    /// The citizen's AI.
    CitizenAi,
    /// The citizen component (needs, family).
    Citizen,
    // DAT_1401cc2c0
    /// Not identified yet.
    Happiness,
    // DAT_1401cf008
    /// BuildComponent message 0 adds one unit of construction work.
    Build,
    // DAT_1401cf5f0
    /// DestroyComponent message 0 adds one unit of deconstruction work.
    Destroy,
    // DAT_1401cc1e0
    /// FireComponent message 0, a bucket of water.
    ///
    /// Original: `FUN_1400499f0`.
    Fire,
    // +0x2b
    /// CompleteRoad's start: `a` the cell, `b` the cell type, `message` the byte.
    ///
    /// Original: `FUN_1400cad20`, `FUN_1400a1b10`.
    Road,
    /// The job manager: message 0 is CompleteTask's start, 1 its abort; `a` is the work id, `b` the
    /// task parameter.
    ///
    /// Original: `FUN_1400fdc30`, `FUN_1400fdcd0`.
    Jobs,
    // +0x30
    /// A NaturalResourceComponent: 0 remove (RemoveNaturalResource start, `a` its flag), 1 release
    /// the removal claim (its abort), 2 play animation `a` (AnimateNaturalResource start).
    ///
    /// Original: `FUN_14012b910`.
    NaturalResource,
    /// A RawMaterialComponent pile: 0 picked up (PickupRawMaterial start).
    Pile,
    // +0x4c
    /// CreateRawMaterial's start: `a` the output template, `b` the cell, `entity` the workplace,
    /// `message` the yield factor as `f32` bits.
    ///
    /// Original: `FUN_1400cc010`.
    CreateRawMaterial,
    /// CreateNaturalResource's start once the cell is free: `a` the template, `b` the cell.
    ///
    /// Original: `FUN_1400cbb20`.
    CreateNaturalResource,
    /// RemoveAnimal's start: the animal `entity` is removed.
    ///
    /// Original: `FUN_1400cfcc0`.
    Animal,
    /// CropFieldComponent's message slot: `message` 0 planted / 1 tended, `a` the cell index,
    /// `entity` the field.
    ///
    /// Original: `FUN_140120020`.
    CropField,
    /// OrchardComponent's message slot: 0 planted, 1 picked, 2 tended; `a` the tree slot.
    ///
    /// Original: `FUN_1400522f0`.
    Orchard,
    /// PastureComponent's message slot: 0 a rancher's visit.
    ///
    /// Original: `FUN_14012cbf0`.
    Pasture,
    /// A loose pile of `a` (material), `b` units, left on the ground near the citizen (the units
    /// already left its storage).
    ///
    /// Original: `FUN_1400ccd90`.
    DropPile,
    /// QuarryComponent's message slot: the quarry `entity` digs deeper.
    ///
    /// Original: `FUN_1400554e0`.
    Quarry,
    /// FishingComponent: the fishing sequence's report (`a` the catch's cell). It reaches the
    /// component's default message handler, which does nothing.
    Fishing,
}

// +0x78
/// A message an action delivered (slot of the receiving component). The owner dispatches these
/// after the character update.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Message {
    /// The component that receives the message.
    pub component: Component,
    /// The message's number.
    pub message: i32,
    /// The entity it is for, if any.
    pub entity: Option<usize>,
    /// The first argument.
    pub a: i32,
    /// The second argument.
    pub b: i32,
}

/// Map state the character and its actions read. `Town::FindMaterialStorage` for a redirected
/// drop-off: the storages, the game's Random, the material, the count and the citizen's cell give a
/// storage and the point to walk to.
///
/// Original: `FUN_140092a60`.
pub type StorageSearch<'a> =
    dyn FnMut(&[Storage], &mut Random, usize, i32, i32) -> Option<(usize, [f32; 3])> + 'a;

/// What a character needs from the world to move: the map, the ground and the path finder.
pub struct World<'a> {
    /// The map's size in cells.
    pub size: usize,
    /// The type of each map cell.
    pub types: &'a [CellType],
    // +0x08; +0x00; +0x28
    /// The object stamped into each map cell.
    pub objects: &'a [Option<usize>],
    /// The occupant of each map cell.
    pub occupants: &'a [Option<usize>],
    /// The flags of each map cell.
    pub flags: &'a [u8],
    // +0x2a
    /// The road kind planned or built (`0xff` none).
    pub roads: &'a [u8],
    // +0x2d
    /// The floors (bridge decks) that characters walk on.
    pub floors: &'a crate::world::floor::Floors,
    /// The walkable region of each cell.
    pub regions: &'a [u16],
    /// The ground's heights.
    pub terrain: &'a HeightGrid<'a>,
    /// The pathfinder.
    pub pathfinder: &'a mut Pathfinder,
    /// The game's random numbers.
    pub rng: &'a mut Random,
    /// The messages that actions send, delivered after the update.
    pub messages: &'a mut Vec<Message>,
    /// What ConsumeRawMaterial used up (material, amount), for the stock manager's count.
    pub used: &'a mut Vec<(usize, i32)>,
    /// The town's storages.
    pub storages: &'a mut [Storage],
    /// The raw materials.
    pub materials: &'a [RawMaterial],
    /// What the character carries.
    pub carried: &'a mut Storage,
    /// The drop-off redirect's storage search (citizens only).
    pub storage_search: Option<&'a mut StorageSearch<'a>>,
    /// Actions an action pushed from its start (`ActionStack::Push`), pushed once it returns.
    pub pushes: Vec<Action>,
    // +0xac
    /// Citizen: tool uses left (PerformWork with a tool spends one).
    pub tool_uses: &'a mut i32,
    /// The tool ShowTool attached (`-1` none, `-2` carried goods).
    pub shown_tool: &'a mut i32,
    /// Per placed entity: its model is playing an animation (`MeshInstance::IsAnimating`).
    pub animating: &'a mut [bool],
    // +0x1f8; +0x200
    /// A citizen's accident: the profession whose work it had the accident at.
    pub accident: &'a mut Option<usize>,
    /// The time since the last check for an injury at work.
    pub accident_timer: &'a mut i64,
    // +0x54; +0x58; +0x30; +0x50
    /// The current job's profession (→ the work's slot) and its accident odds.
    pub accident_odds: Option<(usize, f32, f32)>,
}

impl World<'_> {
    // RandFloat() < time since the last check / (+0x54 × 12 × 300 s × +0x58); Game +0xd0
    /// Already hurt, or, at a job whose profession has accidents, `RandFloat < time since the last
    /// check / (× 12 × 300 s ×)` (the time restarts at every check). Not ported: the debug flag
    /// that stops it (`Game` bit 8).
    ///
    /// Original: `Citizen::InjuryCheck`, `FUN_14011bb20`, `FUN_1400ae760`.
    pub fn injury_check(&mut self) -> bool {
        if self.accident.is_some() {
            return true;
        }
        let Some((profession, years, odds)) = self.accident_odds else {
            return false;
        };
        if odds <= 0.0 {
            *self.accident_timer = 0;
            return false;
        }
        let period = years * 12.0 * 300.0;
        let since = crate::scheduler::seconds(*self.accident_timer);
        *self.accident_timer = 0;
        if self.rng.rand_float() < since / (period * odds) {
            *self.accident = Some(profession);
            return true;
        }
        false
    }

    /// Cell under a map position (clamped, as every caller in the original does).
    pub fn cell(&self, p: [f32; 3]) -> i32 {
        let n = self.size as i32;
        let half = (n >> 1) as f32;
        let x = ((p[0] + half) as i32).clamp(0, n - 1);
        let y = ((p[1] + half) as i32).clamp(0, n - 1);
        n * y + x
    }

    /// Centre of a cell on the ground (on a bridge's floor).
    ///
    /// Original: `FUN_1400a48b0`.
    pub fn cell_centre(&self, cell: i32) -> [f32; 3] {
        let n = self.size as i32;
        let c = (n >> 1) as f32 - 0.5;
        let (x, y) = ((cell % n) as f32 - c, (cell / n) as f32 - c);
        [x, y, self.ground(x, y)]
    }

    /// `FUN_1400a48b0`: the terrain, or a bridge's floor above it.
    fn ground(&self, x: f32, y: f32) -> f32 {
        self.floors.ground(self.size, self.terrain, x, y)
    }
}

/// `Math::Vector3::Heading`: `atan2(-x, y)` in degrees (0 faces +y).
pub fn heading(dir: [f32; 2]) -> f32 {
    crate::math::heading(dir[0], dir[1])
}

/// Normalize the full 3D vector before taking its heading.
///
/// Original: `FUN_1400c7e70`.
pub fn work_facing(source: [f32; 3], target: [f32; 3]) -> f32 {
    let d = crate::math::normalize3([
        source[0] - target[0],
        source[1] - target[1],
        source[2] - target[2],
    ]);
    crate::math::heading(d[0], d[1])
}

fn normalize2(v: [f32; 2]) -> [f32; 2] {
    crate::math::normalize2(v)
}

/// The character state the actions drive.
pub struct Character {
    /// The character description: speeds and turning.
    pub desc: CharacterDescription,
    // +0x158; +0x168
    /// The position of the model.
    pub position: [f32; 3],
    /// The heading in degrees, including the model's yaw offset.
    pub yaw: f32,
    // +0x2c..+0x34; +0x38; +0x3c
    /// The walking direction.
    pub direction: [f32; 3],
    /// The walking speed.
    pub speed: f32,
    /// The heading it turns towards.
    pub target_heading: f32,
    // +0x28
    /// Set with a heading of its own, cleared when a waypoint is faced; no byte read of it was
    /// found in the Application outside the saves, which keep it.
    ///
    /// Original: `FUN_14010f5b0`.
    pub flag_28: bool,
    // +0x13c; FUN_14010f6c0 multiplies it into the speed.
    /// The model's z scale, which scales the walking speed.
    pub scale: f32,
    /// The model's animation state, its clips blending into each other.
    pub animation: AnimationStack,
    /// The actions it runs.
    pub stack: ActionStack,
    // +0x40; Game +0x88
    /// Its slot in the characters' tree (`Game`), −1 while in none.
    pub tree_slot: i32,
}

impl Character {
    /// A character at a position, with its description and its model's scale.
    pub fn new(desc: CharacterDescription, position: [f32; 3], scale: f32) -> Character {
        Character {
            desc,
            position,
            yaw: desc.yaw_offset,
            direction: [0.0; 3],
            speed: 0.0,
            target_heading: 0.0,
            flag_28: false,
            scale,
            animation: AnimationStack::new(0, 0),
            stack: ActionStack::default(),
            tree_slot: -1,
        }
    }

    /// Play clip `anim` (a looping clip already playing keeps its time,
    /// `AnimationState::PlayAnimation`) and set the speed to `scale * speed`.
    ///
    /// Original: `FUN_14010f6c0`.
    pub fn play(&mut self, anim: usize, group: &AnimationGroup, speed: f32) {
        self.animation.play(group, anim, 0);
        self.speed = self.scale * speed;
    }

    /// Run the actions, turn and move. `dt` is the manager's iteration delta. Call
    /// [`Character::animate`] first: `AnimationManager::Update` runs before the entity manager in
    /// each iteration, and the move uses that step's root motion.
    ///
    /// Original: `FUN_14010e990`.
    pub fn update(&mut self, world: &mut World, group: &AnimationGroup, dt: f32) {
        let mut stack = std::mem::take(&mut self.stack);
        stack.run(self, world, group, dt);
        world.messages.append(&mut stack.outbox);
        self.stack = stack;
        self.turn();
        self.walk(world, group);
    }

    /// `Graphics::AnimationManager::Update`: advance the clip; the root delta moves the body.
    pub fn animate(&mut self, group: &AnimationGroup, dt: f32) {
        self.animation.update(group, ticks(dt));
    }

    /// Root motion of the last animation step (`MeshInstance::RootAnimationOffset`).
    fn root_offset(&self, group: &AnimationGroup) -> f32 {
        let Some(pose) = self.animation.pose(group) else {
            return 0.0;
        };
        pose.first().map_or(0.0, |root| {
            let t = root.translation;
            (t[0] * t[0] + t[1] * t[1] + t[2] * t[2]).sqrt()
        })
    }

    fn turn(&mut self) {
        // Turn: keep `turn_keep` of the remaining angle (shortest way round).
        let desc = self.desc;
        let current = self.yaw - desc.yaw_offset;
        let mut diff = self.target_heading - current;
        if diff.abs() >= 180.0 {
            diff = if diff <= 0.0 {
                diff + 360.0
            } else {
                -(360.0 - diff)
            };
        }
        let mut new_heading = self.target_heading;
        if diff.abs() > SNAP_DEGREES {
            new_heading = (diff + current) * (1.0 - desc.turn_keep) + desc.turn_keep * current;
            if new_heading > 180.0 {
                new_heading -= 360.0;
            } else if new_heading < -180.0 {
                new_heading += 360.0;
            }
        }
        self.yaw = new_heading + desc.yaw_offset;
    }

    /// The movement part of a character's update, after the actions ran and the clip advanced.
    ///
    /// Original: the second half of `FUN_14010e990`.
    pub fn walk(&mut self, world: &World, group: &AnimationGroup) {
        let distance = self.root_offset(group) * self.speed;
        let mut p = self.position;
        if distance > 0.0 {
            p[0] += distance * self.direction[0];
            p[1] += distance * self.direction[1];
            p[2] += distance * self.direction[2];
        }
        let cell = world.cell(p);
        p[2] = if world.types[cell as usize].0 == FLAT_TYPE {
            0.0
        } else if self.desc.fixed_height {
            self.desc.height
        } else {
            world.ground(p[0], p[1]).max(self.desc.min_height)
        };
        self.position = p;
    }

    /// Face a point and walk toward it (the end of a `MoveTo`). The direction is the 2D unit
    /// vector normalised again in 3D; the heading comes from the 2D vector.
    ///
    /// Original: `FUN_1400ce1f0`.
    pub fn face(&mut self, target: [f32; 3]) {
        let d = normalize2([target[0] - self.position[0], target[1] - self.position[1]]);
        self.flag_28 = false;
        self.direction = crate::math::normalize3([d[0], d[1], 0.0]);
        self.target_heading = heading(d);
    }
}

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

    fn request(&mut self, from: [f32; 3], world: &mut World) {
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

    fn face_waypoint(&self, ch: &mut Character, world: &World) {
        let cell = self.path[(self.index as usize).min(PATH_CELLS - 1)];
        ch.face(world.cell_centre(cell));
    }

    /// `FUN_1400cdf80` (start).
    fn start(&mut self, ch: &mut Character, world: &mut World, group: &AnimationGroup) -> Status {
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
    fn update(&mut self, ch: &mut Character, world: &mut World, group: &AnimationGroup) -> Status {
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
fn drop_pile(world: &mut World, m: usize, n: i32) {
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
fn drop_off_one(
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

    fn start(&mut self, ch: &mut Character, world: &mut World, group: &AnimationGroup) -> Status {
        match self {
            Action::MoveTo(m) => m.start(ch, world, group),
            Action::PlayAnimation { anim, .. } => {
                ch.play(*anim, group, 1.0);
                Status::Running
            }
            Action::PerformWork {
                facing,
                anim,
                consume,
                ..
            } => {
                // Start (slot 4, `FUN_1400ce8f0`): the clip at speed 0 (`FUN_14010f6c0(anim,
                // 1, 0)`), then `FUN_14010f530(FromYawPitch(facing))`.
                ch.play(*anim, group, 0.0);
                if (-180.0..=180.0).contains(facing) {
                    // FUN_14010f530: the direction (x, y, 0) normalized, the heading back
                    // from the vector.
                    let v = crate::math::from_yaw_pitch(*facing, 0.0);
                    ch.flag_28 = false;
                    ch.direction = crate::math::normalize3([v[0], v[1], 0.0]);
                    ch.target_heading = crate::math::heading(v[0], v[1]);
                }
                // `+0x40`: spend one tool use (never below zero).
                if *consume && *world.tool_uses > 0 {
                    *world.tool_uses = (*world.tool_uses - 1).max(0);
                }
                Status::Running
            }
            Action::SendMessage(m) => {
                world.messages.push(*m);
                Status::Done
            }
            Action::AnimateResource { entity, anim, wait } => {
                world.messages.push(Message {
                    component: Component::NaturalResource,
                    message: 2,
                    entity: Some(*entity),
                    a: *anim as i32,
                    b: 0,
                });
                if let Some(a) = world.animating.get_mut(*entity) {
                    *a = true;
                }
                if *wait {
                    Status::Running
                } else {
                    Status::Done
                }
            }
            Action::RemoveResource { entity, flag } => {
                world.messages.push(Message {
                    component: Component::NaturalResource,
                    message: 0,
                    entity: Some(*entity),
                    a: *flag as i32,
                    b: 0,
                });
                Status::Done
            }
            Action::CompleteRoad { cell, ty, built } => {
                world.messages.push(Message {
                    component: Component::Road,
                    message: *built as i32,
                    entity: None,
                    a: *cell,
                    b: *ty as i32,
                });
                Status::Done
            }
            Action::CreateRawMaterial {
                template,
                cell,
                workplace,
                factor,
                ..
            } => {
                world.messages.push(Message {
                    component: Component::CreateRawMaterial,
                    message: factor.to_bits() as i32,
                    entity: *workplace,
                    a: *template as i32,
                    b: *cell,
                });
                Status::Done
            }
            // `FUN_1400cbb20`: the cell must be free (`FUN_1400a1fd0`: not cell types 0 or 1, no
            // road kind `+0x2a`; a road plan always has one) and without flags. Otherwise the
            // action fails.
            Action::CreateNaturalResource {
                template,
                occupant,
                cell,
            } => {
                let i = *cell as usize;
                let free =
                    world.objects.get(i) == Some(&None) && world.occupants.get(i) == Some(occupant);
                let ty = 1u32 << (world.types[i].0 & 0x1f);
                if !free || ty & 3 != 0 || world.roads[i] != 0xff || world.flags[i] != 0 {
                    return Status::Failed;
                }
                world.messages.push(Message {
                    component: Component::CreateNaturalResource,
                    message: 0,
                    entity: None,
                    a: *template as i32,
                    b: *cell,
                });
                Status::Done
            }
            // `FUN_1400cd720`.
            Action::HuntAnimal { done, .. } => {
                if *done {
                    Status::Done
                } else {
                    Status::Running
                }
            }
            Action::RemoveAnimal { entity } => {
                world.messages.push(Message {
                    component: Component::Animal,
                    message: 0,
                    entity: Some(*entity),
                    a: 0,
                    b: 0,
                });
                Status::Done
            }
            Action::PickupPile { entity } => {
                world.messages.push(Message {
                    component: Component::Pile,
                    message: 0,
                    entity: Some(*entity),
                    a: 0,
                    b: 0,
                });
                Status::Done
            }
            Action::ShowTool(n) => {
                // Above 10 (except −2) detaches (`FUN_14011cbb0`); otherwise attach (`FUN_14011c880`).
                *world.shown_tool = if (*n as u32) > 10 && *n != -2 { -1 } else { *n };
                Status::Running
            }
            Action::CompleteTask { work, param } => {
                if *work >= 0 {
                    world.messages.push(Message {
                        component: Component::Jobs,
                        message: 0,
                        entity: None,
                        a: *work,
                        b: *param,
                    });
                }
                Status::Done
            }
            Action::DropOff {
                storage,
                material,
                count,
                redirect,
            } => {
                // `FUN_1400cc7d0`. No storage: everything carried is left on the ground
                // (`FUN_1400cca40`).
                if *storage == usize::MAX {
                    let items: Vec<(usize, i32)> = world
                        .carried
                        .items
                        .iter()
                        .filter(|i| 0 < i.count)
                        .map(|i| (i.material, i.count))
                        .collect();
                    for (m, n) in items {
                        drop_pile(world, m, n);
                    }
                    return Status::Running;
                }
                // Every material tries the storage the action was made for; a redirect points
                // the action at the storage found.
                let target = *storage;
                let one = |world: &mut World, m: usize, storage: &mut usize| -> bool {
                    drop_off_one(ch, world, target, storage, m, *count, *redirect)
                };
                if *material != usize::MAX {
                    let m = *material;
                    return if one(world, m, storage) {
                        Status::Failed
                    } else {
                        Status::Running
                    };
                }
                let items: Vec<usize> = world
                    .carried
                    .items
                    .iter()
                    .filter(|i| 0 < i.count)
                    .map(|i| i.material)
                    .collect();
                let mut dropped = false;
                for m in items {
                    dropped |= one(world, m, storage);
                }
                if dropped {
                    Status::Failed
                } else {
                    Status::Running
                }
            }
            Action::ConsumeStoredRawMaterial {
                storage,
                items,
                reserved,
            } => {
                // Storage `usize::MAX`: what the citizen carries (`FUN_1400cb880` on its own
                // StorageComponent).
                let target = if *storage == usize::MAX {
                    &mut *world.carried
                } else {
                    &mut world.storages[*storage]
                };
                if consume_stored(target, items, *reserved) {
                    // `FUN_1400cb420`: each item's amount counts as used (`+0xc`).
                    world.used.extend(items.iter().copied());
                    Status::Done
                } else {
                    Status::Failed
                }
            }
            Action::PickupStoredRawMaterial {
                storage,
                items,
                partial,
            } => {
                if pickup_stored(
                    &mut world.storages[*storage],
                    world.carried,
                    world.materials,
                    items,
                    *partial,
                ) {
                    Status::Done
                } else {
                    Status::Failed
                }
            }
        }
    }

    fn update(
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
    fn stop(&mut self, ch: &mut Character, group: &AnimationGroup) {
        match self {
            Action::MoveTo(m) => ch.play(m.stand, group, 1.0),
            Action::PerformWork { .. } => ch.play(0, group, 1.0),
            _ => {}
        }
    }

    /// FUN_1400cb300 / FUN_1400cf140: abort releases pending reservations.
    fn abort(&mut self, ch: &mut Character, group: &AnimationGroup, storages: &mut [Storage]) {
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

#[derive(Debug, Clone)]
struct Task {
    action: Action,
    started: bool,
}

// +0x48; +0x58; +0x60
/// The CharacterComponent's task stack (top index) and its "on empty" chain.
#[derive(Debug, Clone, Default)]
pub struct ActionStack {
    chains: Vec<Vec<Task>>,
    on_empty: Vec<Task>,
    /// Job-manager messages from aborted CompleteTasks, for the owner to deliver right after the
    /// clear.
    ///
    /// Original: `FUN_1400caf90`.
    pub outbox: Vec<Message>,
}

impl ActionStack {
    // +0x18; +0x10
    /// A saved stack: its entries bottom to top, each one action and whether it started, and the
    /// "on empty" action. Actions linked to an entry are not saved.
    ///
    /// Original: `FUN_14010ef80`.
    pub fn restored(entries: Vec<(Action, bool)>, on_empty: Option<Action>) -> ActionStack {
        ActionStack {
            chains: entries
                .into_iter()
                .map(|(action, started)| vec![Task { action, started }])
                .collect(),
            on_empty: on_empty
                .into_iter()
                .map(|action| Task {
                    action,
                    started: false,
                })
                .collect(),
            outbox: Vec::new(),
        }
    }

    /// What a save holds: each entry's first action with its started flag, and the "on empty"
    /// chain's first action.
    ///
    /// Original: `FUN_14010ef80`.
    pub fn saved(&self) -> (Vec<(&Action, bool)>, Option<&Action>) {
        (
            self.chains
                .iter()
                .filter_map(|c| c.first().map(|t| (&t.action, t.started)))
                .collect(),
            self.on_empty.first().map(|t| &t.action),
        )
    }

    // +0x60 + 1
    /// How many entries the stack holds.
    pub fn depth(&self) -> usize {
        self.chains.len()
    }

    // +0x60 >= 0
    /// `>= 0`: something is queued (the AI's "is busy" test).
    pub fn is_active(&self) -> bool {
        !self.chains.is_empty()
    }

    // +0x40
    /// Abort every chain (slot on all actions) and drop the "on empty" chain.
    ///
    /// Original: `FUN_1400ca8d0`.
    pub fn clear(&mut self, ch: &mut Character, group: &AnimationGroup, storages: &mut [Storage]) {
        for chain in &mut self.chains {
            for t in chain.iter_mut() {
                if let Action::CompleteTask { work, param } = t.action {
                    if work >= 0 {
                        self.outbox.push(Message {
                            component: Component::Jobs,
                            message: 1,
                            entity: None,
                            a: work,
                            b: param,
                        });
                    }
                }
                // `FUN_1400cff70`: the resource is free again.
                if let Action::RemoveResource { entity, .. } = t.action {
                    self.outbox.push(Message {
                        component: Component::NaturalResource,
                        message: 1,
                        entity: Some(entity),
                        a: 0,
                        b: 0,
                    });
                }
                t.action.abort(ch, group, storages);
            }
        }
        self.chains.clear();
        self.on_empty.clear();
    }

    /// The stored goods queued actions hold reserved: `(storage, material, count)` of each
    /// unstarted PickupStoredRawMaterial and reserving ConsumeStoredRawMaterial.
    pub fn pending_reservations(&self) -> Vec<(usize, usize, i32)> {
        let mut out = Vec::new();
        for t in self.chains.iter().flatten().filter(|t| !t.started) {
            match &t.action {
                Action::PickupStoredRawMaterial { storage, items, .. }
                | Action::ConsumeStoredRawMaterial {
                    storage,
                    items,
                    reserved: true,
                } => out.extend(items.iter().map(|&(m, n)| (*storage, m, n))),
                _ => {}
            }
        }
        out
    }

    /// Does a chain start with this work's CompleteTask?
    ///
    /// Original: `FUN_1400fc5d0`.
    pub fn holds_task(&self, work: i32) -> Option<i32> {
        self.chains
            .iter()
            .find_map(|c| match c.first().map(|t| &t.action) {
                Some(Action::CompleteTask { work: w, param }) if *w == work => Some(*param),
                _ => None,
            })
    }

    // +0x10
    /// Link `action` into the top chain (the original's of the top action).
    pub fn attach(&mut self, action: Action) {
        if let Some(top) = self.chains.last_mut() {
            top.push(Task {
                action,
                started: false,
            });
        }
    }

    /// The top chain's `HuntAnimal`, if any: its lodge and its `done` flag.
    pub fn hunt_watch(&mut self) -> Option<(usize, &mut bool)> {
        self.chains
            .last_mut()?
            .iter_mut()
            .filter(|t| t.started)
            .find_map(|t| match &mut t.action {
                Action::HuntAnimal { lodge, done } => Some((*lodge, done)),
                _ => None,
            })
    }

    // +0x30
    /// The scheduled notify: the running chain's started actions get slot; MoveTo's sets whether it
    /// follows.
    ///
    /// Original: `FUN_14010e950`, `FUN_1400ca450`, `FUN_1400cde40`.
    pub fn notify_moves(&mut self, following: bool) {
        let Some(chain) = self.chains.last_mut() else {
            return;
        };
        for t in chain.iter_mut().filter(|t| t.started) {
            if let Action::MoveTo(m) = &mut t.action {
                m.following = following;
            }
        }
    }

    /// Set the actions to run when the stack is empty.
    pub fn set_on_empty(&mut self, actions: Vec<Action>) {
        self.on_empty = actions
            .into_iter()
            .map(|action| Task {
                action,
                started: false,
            })
            .collect();
    }

    /// Stop the current top's started actions, then push a chain.
    ///
    /// Original: `FUN_1400ca9b0`.
    pub fn push(&mut self, ch: &mut Character, group: &AnimationGroup, action: Action) {
        if let Some(top) = self.chains.last_mut() {
            for t in top.iter_mut().filter(|t| t.started) {
                t.started = false;
                t.action.stop(ch, group);
            }
        }
        self.chains.push(vec![Task {
            action,
            started: false,
        }]);
    }

    /// `FUN_1400ca120`.
    fn run(&mut self, ch: &mut Character, world: &mut World, group: &AnimationGroup, dt: f32) {
        let dt = ticks(dt);
        while let Some(top) = self.chains.last_mut() {
            let mut status = Status::Running;
            if !top.first().is_some_and(|t| t.started) {
                for t in top.iter_mut() {
                    t.started = true;
                    status = t.action.start(ch, world, group);
                    if status != Status::Running || !world.pushes.is_empty() {
                        break;
                    }
                }
            }
            // An action's start pushed a chain (`ActionStack::Push`): the top's started
            // actions stop and the new chain runs first.
            if !world.pushes.is_empty() {
                for action in std::mem::take(&mut world.pushes) {
                    self.push(ch, group, action);
                }
                if status == Status::Failed {
                    self.clear(ch, group, world.storages);
                }
                continue;
            }
            let mut ran = false;
            if status == Status::Running {
                for t in top.iter_mut() {
                    status = t.action.update(ch, world, group, dt);
                    if status != Status::Running {
                        break;
                    }
                }
                ran = status == Status::Running;
            }
            match status {
                Status::Done => {
                    let mut chain = self.chains.pop().unwrap_or_default();
                    for t in chain.iter_mut().filter(|t| t.started) {
                        t.started = false;
                        t.action.stop(ch, group);
                    }
                }
                // `FUN_1400ca8d0` also drops the "on empty" chain.
                Status::Failed => self.clear(ch, group, world.storages),
                Status::Running => {}
            }
            if ran {
                break;
            }
        }
        if self.chains.is_empty() && !self.on_empty.is_empty() {
            for t in self.on_empty.iter_mut() {
                t.started = true;
                if t.action.start(ch, world, group) != Status::Running {
                    break;
                }
            }
            self.on_empty.clear();
        }
    }
}

/// Seconds as manager ticks, for action durations.
pub fn duration(s: f32) -> i64 {
    ticks(s)
}

/// Manager ticks as seconds.
pub fn as_seconds(t: i64) -> f32 {
    seconds(t)
}

#[cfg(test)]
mod inventory_action_tests {
    use super::*;

    #[test]
    fn clearing_pending_consumption_and_pickup_releases_reservations() {
        let group = AnimationGroup {
            clips: vec![],
            default_blend: 0.0,
            blends: vec![],
            blend_matrix: vec![],
        };
        let mut ch = Character::new(CharacterDescription::default(), [0.0; 3], 1.0);
        let mut storages = vec![Storage {
            capacity: 10,
            accepts: u32::MAX,
            items: vec![],
        }];
        storages[0].add(0, 3);
        let mut stack = ActionStack::default();
        for pickup in [false, true] {
            storages[0].reserve(0, 1);
            stack.push(
                &mut ch,
                &group,
                if pickup {
                    Action::PickupStoredRawMaterial {
                        storage: 0,
                        items: vec![(0, 1)],
                        partial: false,
                    }
                } else {
                    Action::ConsumeStoredRawMaterial {
                        storage: 0,
                        items: vec![(0, 1)],
                        reserved: true,
                    }
                },
            );
        }
        assert_eq!(storages[0].available(0), 1);
        stack.clear(&mut ch, &group, &mut storages);
        assert_eq!(storages[0].available(0), 3);
        assert_eq!(storages[0].entry(0).unwrap().reserved, 0);
        assert!(!stack.is_active());
    }
}

#[cfg(test)]
mod drop_off_tests {
    use super::*;

    /// A drop-off with `redirect` whose storage is full: with another storage found, the
    /// action points there and a walk is pushed; with none, the goods are left as a pile
    /// (`DropPile`) and the action fails. Without `redirect` nothing happens.
    #[test]
    fn a_full_storage_redirects_or_drops_a_pile() {
        let n = 8;
        let types = vec![CellType(2); n * n];
        let none = vec![None; n * n];
        let zeros = vec![0u8; n * n];
        let roads = vec![0xffu8; n * n];
        let floors = crate::world::floor::Floors::new(n * n);
        let regions = vec![0u16; n * n];
        let heights = vec![0i16; (2 * n + 1) * (2 * n + 1)];
        let terrain = HeightGrid {
            width: 2 * n + 1,
            heights: &heights,
        };
        let materials = vec![RawMaterial {
            size: 1,
            ..Default::default()
        }];
        let group = AnimationGroup {
            clips: vec![],
            default_blend: 0.0,
            blends: vec![],
            blend_matrix: vec![],
        };
        for (found, redirect) in [(true, true), (false, true), (false, false)] {
            let mut pathfinder = Pathfinder::new(n, &types);
            let mut rng = Random::new(1);
            let (mut messages, mut used) = (Vec::new(), Vec::new());
            let full = Storage {
                capacity: 0,
                accepts: u32::MAX,
                items: vec![],
            };
            let mut storages = vec![
                full.clone(),
                Storage {
                    capacity: 10,
                    ..full
                },
            ];
            let mut carried = Storage {
                capacity: 10,
                accepts: u32::MAX,
                items: vec![],
            };
            carried.add(0, 3);
            let (mut tool_uses, mut shown, mut accident, mut timer) = (0, -1, None, 0);
            let mut animating = vec![];
            let mut search = |_: &[Storage], _: &mut Random, m: usize, count: i32, _: i32| {
                assert_eq!((m, count), (0, 3));
                found.then_some((1, [1.0, 1.0, 0.0]))
            };
            let mut world = World {
                size: n,
                types: &types,
                objects: &none,
                occupants: &none,
                flags: &zeros,
                roads: &roads,
                floors: &floors,
                regions: &regions,
                terrain: &terrain,
                pathfinder: &mut pathfinder,
                rng: &mut rng,
                messages: &mut messages,
                used: &mut used,
                storages: &mut storages,
                materials: &materials,
                carried: &mut carried,
                storage_search: Some(&mut search),
                pushes: Vec::new(),
                tool_uses: &mut tool_uses,
                shown_tool: &mut shown,
                animating: &mut animating,
                accident: &mut accident,
                accident_timer: &mut timer,
                accident_odds: None,
            };
            let mut ch = Character::new(CharacterDescription::default(), [0.0; 3], 1.0);
            let mut action = Action::DropOff {
                storage: 0,
                material: usize::MAX,
                count: -1,
                redirect,
            };
            let status = action.start(&mut ch, &mut world, &group);
            let pushed = world.pushes.len();
            let Action::DropOff { storage, .. } = action else {
                unreachable!()
            };
            match (found, redirect) {
                (true, _) => {
                    assert_eq!((status, storage, pushed), (Status::Running, 1, 1));
                    assert_eq!(world.carried.available(0), 3);
                }
                (false, true) => {
                    assert_eq!((status, storage, pushed), (Status::Failed, 0, 0));
                    assert_eq!(world.carried.available(0), 0);
                    assert_eq!(world.messages[0].component, Component::DropPile);
                    assert_eq!((world.messages[0].a, world.messages[0].b), (0, 3));
                }
                (false, false) => {
                    assert_eq!((status, pushed), (Status::Running, 0));
                    assert_eq!(world.carried.available(0), 3);
                    assert!(world.messages.is_empty());
                }
            }
        }
    }
}

#[cfg(test)]
mod restored_move_tests {
    use super::*;

    /// A walk restored from a save mid-way (index 1 of 3 cells) plays its walk clip and faces
    /// its next cell on the first update, once. Without the clip the body has no root motion
    /// and would never reach the next cell.
    #[test]
    fn a_restored_walk_plays_its_clip_and_faces_the_next_cell_once() {
        let n = 8;
        let types = vec![CellType(2); n * n];
        let none = vec![None; n * n];
        let zeros = vec![0u8; n * n];
        let roads = vec![0xffu8; n * n];
        let floors = crate::world::floor::Floors::new(n * n);
        let regions = vec![0u16; n * n];
        let heights = vec![0i16; (2 * n + 1) * (2 * n + 1)];
        let terrain = HeightGrid {
            width: 2 * n + 1,
            heights: &heights,
        };
        let group = AnimationGroup {
            clips: vec![],
            default_blend: 0.0,
            blends: vec![],
            blend_matrix: vec![],
        };
        let mut pathfinder = Pathfinder::new(n, &types);
        let mut rng = Random::new(1);
        let (mut messages, mut used) = (Vec::new(), Vec::new());
        let mut storages = vec![];
        let mut carried = Storage {
            capacity: 0,
            accepts: u32::MAX,
            items: vec![],
        };
        let (mut tool_uses, mut shown, mut accident, mut timer) = (0, -1, None, 0);
        let mut animating = vec![];
        let mut world = World {
            size: n,
            types: &types,
            objects: &none,
            occupants: &none,
            flags: &zeros,
            roads: &roads,
            floors: &floors,
            regions: &regions,
            terrain: &terrain,
            pathfinder: &mut pathfinder,
            rng: &mut rng,
            messages: &mut messages,
            used: &mut used,
            storages: &mut storages,
            materials: &[],
            carried: &mut carried,
            storage_search: None,
            pushes: Vec::new(),
            tool_uses: &mut tool_uses,
            shown_tool: &mut shown,
            animating: &mut animating,
            accident: &mut accident,
            accident_timer: &mut timer,
            accident_odds: None,
        };
        let (here, next, end) = (2 * n as i32 + 2, 2 * n as i32 + 3, 2 * n as i32 + 4);
        let (walk, stand) = (5, 6);
        let mut ch = Character::new(
            CharacterDescription::default(),
            world.cell_centre(here),
            1.0,
        );
        ch.direction = [0.0; 3];
        ch.play(stand, &group, 1.0);
        let mut m = MoveTo::restored(
            world.cell_centre(end),
            walk,
            stand,
            0,
            0,
            1.0,
            false,
            1,
            &[here, next, end],
        );
        assert_eq!(m.update(&mut ch, &mut world, &group), Status::Running);
        assert_eq!(ch.animation.clip(), Some(walk));
        let to_next = world.cell_centre(next);
        assert!(ch.direction[0] * (to_next[0] - ch.position[0]) > 0.0);
        // Only the first update resumes: a clip played afterwards is left alone.
        ch.play(stand, &group, 1.0);
        m.update(&mut ch, &mut world, &group);
        assert_eq!(ch.animation.clip(), Some(stand));
    }
}
