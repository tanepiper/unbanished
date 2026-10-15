//! CharacterComponent (vtable `0x140168790`, constructor `FUN_14010f1f0`) and the `Action` task
//! stack it runs. See docs/CITIZEN-AI.md.
//!
//! - Slot `+0x40` (`FUN_14010e990`, bit `0x04` phase, every iteration): run the action stack
//!   (`FUN_1400ca120`), then turn toward the target heading, move by the animation's root
//!   motion times the speed, and drop onto the ground.
//! - Slot `+0x48` (`FUN_14010e950`, bit `0x08` bucket phase): notify the running chain
//!   (`FUN_1400ca450`, action slot `+0x30`). No shipped citizen action overrides it.
//!
//! Actions (registered `Action` types, vtable slots `+0x20` start, `+0x28` update,
//! `+0x38` stop, `+0x40` abort) return [`Status`]. The stack is an array of chains; the top
//! chain is started once, then updated each run. `Done` pops it and continues with the next
//! chain in the same run, `Failed` clears everything, and when the stack empties the
//! "on empty" chain (`+0x58`) runs once (`FUN_1400ca080`).

use banished_formats::game::CharacterDescription;
use banished_formats::types::AnimationGroup;

use crate::animation::Playback;
use crate::pathfind::{self, Pathfinder};
use crate::random::Random;
use crate::scheduler::{seconds, ticks};
use crate::stock::{consume_stored, pickup_stored, RawMaterial, Storage};
use crate::terrain::{CellType, HeightGrid};

/// `DAT_1401787d4`: arrival distance.
pub const ARRIVE: f32 = 0.05;
/// `DAT_1401787e4`: turns smaller than this snap to the target heading.
const SNAP_DEGREES: f32 = 0.1;
/// MoveTo's path buffer (`+0x68`, 256 cells, `FUN_1400ac220`).
const PATH_CELLS: usize = 0x100;
/// Cell type whose surface is at height 0 (`FUN_14010e990`, type `0x0b`).
const FLAT_TYPE: u8 = 11;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Done = 0,
    Running = 1,
    Failed = 2,
}

/// Which component a [`SendMessage`] targets (`DAT_1401cf0e8` CitizenAI, `DAT_1401cf158`
/// Citizen).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Component {
    CitizenAi,
    Citizen,
    /// `DAT_1401cc2c0`.
    Happiness,
    /// `DAT_1401cf008`: BuildComponent message 0 adds one unit of construction work.
    Build,
    /// The job manager: message 0 is CompleteTask's start (`FUN_1400fdc30`), 1 its abort
    /// (`FUN_1400fdcd0`); `a` is the work id, `b` the task parameter.
    Jobs,
}

/// A message an action delivered (slot `+0x78` of the receiving component). The owner
/// dispatches these after the character update.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Message {
    pub component: Component,
    pub message: i32,
    pub entity: Option<usize>,
    pub a: i32,
    pub b: i32,
}

/// Map state the character and its actions read.
pub struct World<'a> {
    pub size: usize,
    pub types: &'a [CellType],
    pub regions: &'a [u16],
    pub terrain: &'a HeightGrid<'a>,
    pub pathfinder: &'a mut Pathfinder,
    pub rng: &'a mut Random,
    pub messages: &'a mut Vec<Message>,
    pub storages: &'a mut [Storage],
    pub materials: &'a [RawMaterial],
    pub carried: &'a mut Storage,
    /// Citizen `+0xac`: tool uses left (PerformWork with a tool spends one).
    pub tool_uses: &'a mut i32,
    /// The tool ShowTool attached (`-1` none, `-2` carried goods).
    pub shown_tool: &'a mut i32,
}

impl World<'_> {
    /// Cell under a map position (clamped, as every caller in the original does).
    pub fn cell(&self, p: [f32; 3]) -> i32 {
        let n = self.size as i32;
        let half = (n >> 1) as f32;
        let x = ((p[0] + half) as i32).clamp(0, n - 1);
        let y = ((p[1] + half) as i32).clamp(0, n - 1);
        n * y + x
    }

    /// Centre of a cell on the ground (`FUN_1400a48b0` without bridge picking).
    pub fn cell_centre(&self, cell: i32) -> [f32; 3] {
        let n = self.size as i32;
        let c = (n >> 1) as f32 - 0.5;
        let (x, y) = ((cell % n) as f32 - c, (cell / n) as f32 - c);
        let mut normal = [0.0; 3];
        [x, y, self.terrain.ground(x, y, &mut normal)]
    }

    fn ground(&self, x: f32, y: f32) -> f32 {
        let mut normal = [0.0; 3];
        self.terrain.ground(x, y, &mut normal)
    }
}

/// `Math::Vector3::Heading`: `atan2(-x, y)` in degrees (0 faces +y).
pub fn heading(dir: [f32; 2]) -> f32 {
    (-dir[0]).atan2(dir[1]).to_degrees()
}

/// FUN_1400c7e70: normalize the full 3D vector before taking its heading.
pub fn work_facing(source: [f32; 3], target: [f32; 3]) -> f32 {
    let mut d = [
        source[0] - target[0],
        source[1] - target[1],
        source[2] - target[2],
    ];
    let length = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
    if length != 0.0 {
        let inverse = 1.0 / length;
        for v in &mut d {
            *v *= inverse;
        }
    }
    // Runtime Heading (18007d3d0) rounds the double-precision atan2 result to f32 first.
    ((-(d[0] as f64)).atan2(d[1] as f64) as f32) * 57.29578
}

fn normalize2(v: [f32; 2]) -> [f32; 2] {
    let l = (v[0] * v[0] + v[1] * v[1]).sqrt();
    if l > 0.0 {
        [v[0] / l, v[1] / l]
    } else {
        v
    }
}

/// The character state the actions drive.
pub struct Character {
    pub desc: CharacterDescription,
    /// Model transform `+0x158` and yaw `+0x168` (degrees, including the yaw offset).
    pub position: [f32; 3],
    pub yaw: f32,
    /// `+0x2c..+0x34`: walking direction; `+0x38` speed; `+0x3c` target heading.
    pub direction: [f32; 3],
    pub speed: f32,
    pub target_heading: f32,
    /// `+0x28`, cleared when a waypoint is faced (consumer not traced).
    pub flag_28: bool,
    /// Model scale z (model `+0x13c`): `FUN_14010f6c0` multiplies it into the speed.
    pub scale: f32,
    /// The model's animation state (one layer: the original blends between clips).
    pub animation: Playback,
    pub stack: ActionStack,
}

impl Character {
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
            animation: Playback::new(0, 0),
            stack: ActionStack::default(),
        }
    }

    /// `FUN_14010f6c0`: play clip `anim` (a looping clip already playing keeps its time,
    /// `AnimationState::PlayAnimation`) and set the speed to `scale * speed`.
    pub fn play(&mut self, anim: usize, group: &AnimationGroup, speed: f32) {
        let same = self.animation.clip == anim && group.clips.get(anim).is_some_and(|c| c.looping);
        if !same {
            self.animation = Playback::new(anim, 0);
        }
        self.speed = self.scale * speed;
    }

    /// `FUN_14010e990`: run the actions, turn and move. `dt` is the manager's iteration delta.
    /// Call [`Character::animate`] first: `AnimationManager::Update` runs before the entity
    /// manager in each iteration, and the move uses that step's root motion.
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
        if let Some(clip) = group.clips.get(self.animation.clip) {
            self.animation.update(clip, ticks(dt));
        }
    }

    /// Root motion of the last animation step (`MeshInstance::RootAnimationOffset`).
    fn root_offset(&self, group: &AnimationGroup) -> f32 {
        let Some(clip) = group.clips.get(self.animation.clip) else {
            return 0.0;
        };
        let pose = self.animation.pose(clip);
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

    /// The movement half of `FUN_14010e990` (after the stack ran and the clip advanced).
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

    /// `FUN_1400ce1f0` / MoveTo's final approach: face and walk toward a point.
    pub fn face(&mut self, target: [f32; 3]) {
        let d = normalize2([target[0] - self.position[0], target[1] - self.position[1]]);
        self.flag_28 = false;
        self.direction = [d[0], d[1], 0.0];
        self.target_heading = heading(d);
    }
}

/// `MoveTo` (vtable `0x140166260`, constructor `FUN_1400ce640`).
#[derive(Debug, Clone)]
pub struct MoveTo {
    /// `+0x38`: destination.
    pub target: [f32; 3],
    /// `+0x5c`: allowed cell types.
    pub flags: u32,
    /// `+0x468` / `+0x46c`: walk and stand clips.
    pub walk: usize,
    pub stand: usize,
    /// `+0x470`: pathfinder cost table.
    pub mode: u32,
    /// `+0x478`: speed factor (random 0.9–1.1 when constructed with a negative value).
    pub speed: f32,
    /// `+0x474`: walking behind another character (`FUN_1400cde40`).
    pub following: bool,
    /// Pending request result (`+0x20/+0x24`); available on the next update.
    pending: Option<Option<Vec<i32>>>,
    /// `+0x60` index, `+0x64` count, `+0x68` cells.
    index: i32,
    count: i32,
    path: [i32; PATH_CELLS],
}

impl MoveTo {
    /// `FUN_1400ce640`.
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
        }
    }

    /// `FUN_1400ce0e0`: plan from `from` to the target.
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
    MoveTo(Box<MoveTo>),
    /// `PlayAnimation` (vtable `0x14015ea70`): play `anim` for `duration`.
    PlayAnimation {
        anim: usize,
        duration: i64,
        elapsed: i64,
    },
    /// `PerformWork` (vtable `0x1401662c0`): face `facing` (if within ±180), play `anim` for
    /// `duration`.
    PerformWork {
        facing: f32,
        anim: usize,
        duration: i64,
        elapsed: i64,
        /// `+0x40`: decrement the citizen's `+0xac` counter on start.
        consume: bool,
    },
    /// `SendComponentMessage` (vtable `0x1401664e0`): deliver a message on start.
    SendMessage(Message),
    /// FUN_1400cb880 / FUN_1400cb980: reserve at construction; consume on start.
    ConsumeStoredRawMaterial {
        storage: usize,
        items: Vec<(usize, i32)>,
        reserved: bool,
    },
    /// FUN_1400cf9f0 / FUN_1400cf700: reserve at construction; transfer on start.
    PickupStoredRawMaterial {
        storage: usize,
        items: Vec<(usize, i32)>,
        partial: bool,
    },
    /// `ShowTool` (vtable `0x14015ead0`, start `FUN_140043070`): attach tool mesh `n`.
    /// Not ported: drawing the tool on the citizen's `tool` node.
    ShowTool(i32),
    /// `CompleteTask` (vtable `0x140165fe0`, constructor `FUN_1400cb270`): tell the job manager
    /// the task is done (start `FUN_1400cb010`) or abandoned (abort `FUN_1400caf90`).
    CompleteTask {
        work: i32,
        param: i32,
    },
    /// `DropOffRawMaterial` (vtable `0x1401661a0`, start `FUN_1400cc7d0`): move up to `count`
    /// carried units into `storage`.
    DropOff {
        storage: usize,
        material: usize,
        count: i32,
    },
}

impl Action {
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
                ch.play(*anim, group, 0.0);
                // `FUN_14010f530(FromYawPitch(facing))`.
                if (-180.0..=180.0).contains(facing) {
                    let (s, c) = facing.to_radians().sin_cos();
                    ch.direction = [-s, c, 0.0];
                    ch.target_heading = *facing;
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
            } => {
                // `FUN_1400ccaf0`: as many as carried, up to the request, when they fit.
                let carried = world.carried.available(*material);
                let n = if *count >= 0 {
                    carried.min(*count)
                } else {
                    carried
                };
                if n > 0 && world.storages[*storage].fits(world.materials, *material, n) {
                    world.storages[*storage].add(*material, n);
                    world.carried.remove(*material, n);
                    return Status::Running;
                }
                // Not ported: redirecting to another storage (`FUN_140092a60` put, MoveTo) and
                // dropping a loose pile (`FUN_1400ccd90`); the drop-off fails instead.
                Status::Failed
            }
            Action::ConsumeStoredRawMaterial {
                storage,
                items,
                reserved,
            } => {
                if consume_stored(&mut world.storages[*storage], items, *reserved) {
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
                    // `FUN_14011bb20` (injury check) can fail the work; not ported yet.
                    return Status::Done;
                }
                *elapsed += dt;
                Status::Running
            }
            // The base update returns 0.
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

/// The CharacterComponent's task stack (`+0x48`, top index `+0x60`) and its "on empty" chain
/// (`+0x58`).
#[derive(Debug, Clone, Default)]
pub struct ActionStack {
    chains: Vec<Vec<Task>>,
    on_empty: Vec<Task>,
    /// Job-manager messages from aborted CompleteTasks (`FUN_1400caf90`), for the owner to
    /// deliver right after the clear.
    pub outbox: Vec<Message>,
}

impl ActionStack {
    /// `+0x60 >= 0`: something is queued (the AI's "is busy" test).
    pub fn is_active(&self) -> bool {
        !self.chains.is_empty()
    }

    /// `FUN_1400ca8d0`: abort every chain (slot `+0x40` on all actions) and drop the
    /// "on empty" chain.
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
                t.action.abort(ch, group, storages);
            }
        }
        self.chains.clear();
        self.on_empty.clear();
    }

    /// `FUN_1400fc5d0`: does a chain start with this work's CompleteTask?
    pub fn holds_task(&self, work: i32) -> Option<i32> {
        self.chains
            .iter()
            .find_map(|c| match c.first().map(|t| &t.action) {
                Some(Action::CompleteTask { work: w, param }) if *w == work => Some(*param),
                _ => None,
            })
    }

    pub fn set_on_empty(&mut self, actions: Vec<Action>) {
        self.on_empty = actions
            .into_iter()
            .map(|action| Task {
                action,
                started: false,
            })
            .collect();
    }

    /// `FUN_1400ca9b0`: stop the current top's started actions, then push a chain.
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
                    if status != Status::Running {
                        break;
                    }
                }
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
