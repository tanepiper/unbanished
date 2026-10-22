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

mod actions;
mod movement;
mod stack;
pub use actions::Action;
pub use movement::MoveTo;
pub use stack::ActionStack;

// vtable 0x140168790; +0x40; +0x48; +0x30; +0x28 update; +0x58; +0x20; +0x38

use unbanished_formats::game::CharacterDescription;
use unbanished_formats::types::AnimationGroup;

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
        if crate::chance::once_in_years(self.accident_timer, years, odds, self.rng) {
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
    /// Returns the event resources the newest clip fired (its sounds, for
    /// [`AnimationSounds::fire`](crate::behaviour::sounds::AnimationSounds::fire)).
    pub fn animate(&mut self, group: &AnimationGroup, dt: f32) -> Vec<String> {
        let fired = self.animation.update(group, ticks(dt));
        let Some(clip) = self.animation.clip().and_then(|c| group.clips.get(c)) else {
            return Vec::new();
        };
        fired
            .into_iter()
            .filter_map(|i| clip.events.get(i)?.resource.clone())
            .collect()
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

/// Seconds as manager ticks, for action durations.
pub fn duration(s: f32) -> i64 {
    ticks(s)
}

/// Manager ticks as seconds.
pub fn as_seconds(t: i64) -> f32 {
    seconds(t)
}

#[cfg(test)]
mod inventory_action_tests;

#[cfg(test)]
mod drop_off_tests;

#[cfg(test)]
mod restored_move_tests;
