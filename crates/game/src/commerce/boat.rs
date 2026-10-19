//! The trader boat. See docs/TRADE.md, "The boat".
//!
//! The boat comes in along the map's first river from off the map, takes a water path past its
//! trading post's "boat" point (where it docks), and leaves along the river's tail.
//!
//! One state step of the original measures the route point against itself: a length of 0 that is
//! never below 0, so its jump ahead never happens and it always returns true. There is nothing to
//! port for it.
//!
//! Original: `BoatComponent`: `FUN_140047160`, `FUN_140047200`, `FUN_1400ac590`; the step that
//! does nothing is `FUN_140045af0`.
// vtable 0x14015f060

use crate::behaviour::character::Character;
use crate::pathfind::Pathfinder;
use crate::town::Sim;

/// `DAT_140178880`, `DAT_14017889c`, `DAT_1401787ec`, `DAT_14017880c`, `DAT_1401787fc`.
const NEAR: f32 = 3.0;
const AHEAD: f32 = 6.0;
const SLOW: f32 = 0.125;
const SLOWEST: f32 = 0.33;
const DOCKED: f32 = 0.2;
/// `DAT_1401788f4`: the squared distance from the dock it starts its approach at.
const APPROACH: f32 = 64.0;
/// The trading post's state messages: 9 the boat docked, 0xb it went before docking.
pub const DOCKED_MESSAGE: i32 = 9;
/// The message a boat sends when it has left the map.
pub const GONE_MESSAGE: i32 = 0xb;

/// The boat's character kind (TraderBoat's CharacterDescription).
#[derive(Debug, Clone, Copy)]
pub struct BoatKind {
    /// The character description.
    pub character: banished_formats::game::CharacterDescription,
}

// +0x28; +0x40; +0x4c; +0x50; +0x60; +0x70
/// A trader boat: its state (0 sailing in, 1 approaching, 2 docked, 3 casting off, 4 sailing out),
/// its trading post, the dock and its heading, the point it heads for, its route, and its place on
/// the route.
pub struct Boat {
    /// The template.
    pub template: usize,
    /// The character.
    pub character: Character,
    /// The state: 0 sailing in, 1 approaching, 2 docked, 3 casting off, 4 sailing out.
    pub state: i8,
    /// The trading post.
    pub post: Option<usize>,
    /// The dock.
    pub dock: [f32; 3],
    /// The dock's heading, in degrees.
    pub dock_yaw: f32,
    /// The point it heads for.
    pub target: [f32; 3],
    /// The route.
    pub route: Vec<[f32; 3]>,
    /// Its place on the route.
    pub index: i32,
}

/// `FUN_1400ac590`: from the route index on, the first point at least `d` from `p` (only the
/// plane distance and `p`'s height count), or none at the end.
fn ahead(route: &[[f32; 3]], index: i32, p: [f32; 3], d: f32) -> Option<i32> {
    let mut i = index;
    loop {
        i += 1;
        if route.len() as i32 <= i {
            return None;
        }
        let q = route[i as usize];
        let (dx, dy, dz) = (p[0] - q[0], p[1] - q[1], p[2]);
        if d * d <= dx * dx + dy * dy + dz * dz {
            return Some(i);
        }
    }
}

/// `Math::Vector3::Length`.
fn length(v: [f32; 3]) -> f32 {
    crate::math::length3(v)
}

/// `Math::Vector3::Normalize`.
fn normalize(v: [f32; 3]) -> [f32; 3] {
    crate::math::normalize3(v)
}

/// `Math::Vector3::Heading`.
fn heading(v: [f32; 3]) -> f32 {
    crate::behaviour::character::heading([v[0], v[1]])
}

/// The river's points before it enters the map, a water path (mode 3, types 7..9) from where it
/// enters to the dock and on to where it leaves, then the river's points after the first one off
/// the map again. Also the dock heading, turned with the river's flow at the route point nearest
/// the dock.
///
/// Original: `FUN_140046040` (route).
pub fn route(
    size: usize,
    terrain: &crate::terrain::HeightGrid,
    floors: &crate::world::floor::Floors,
    pathfinder: &mut Pathfinder,
    river: &[[f32; 2]],
    dock: [f32; 3],
    dock_yaw: f32,
) -> (Vec<[f32; 3]>, f32) {
    let n = size as i32;
    let half = (n >> 1) as f32;
    let inside = |p: [f32; 2]| {
        let (x, y) = ((p[0] + half) as i32, (p[1] + half) as i32);
        0 <= x && x < n && 0 <= y && y < n
    };
    let cell = |p: [f32; 2]| {
        let x = ((p[0] + half) as i32).clamp(0, n - 1);
        let y = ((p[1] + half) as i32).clamp(0, n - 1);
        y * n + x
    };
    let mut route = Vec::new();
    let mut i = 0;
    let mut entry = [0.0; 2];
    while i < river.len() {
        let p = river[i];
        i += 1;
        if inside(p) {
            entry = p;
            break;
        }
        route.push([p[0], p[1], 0.0]);
    }
    // The run inside the map: its last point, and the index after the first point off it.
    let mut exit = entry;
    let mut tail = i;
    while tail < river.len() {
        let p = river[tail];
        tail += 1;
        if !inside(p) {
            break;
        }
        exit = p;
    }
    pathfinder.set_mode(3);
    pathfinder.mask = 0x380;
    let dock_cell = cell([dock[0], dock[1]]);
    let into = pathfinder.find(cell(entry), dock_cell).unwrap_or_default();
    let out = pathfinder.find(dock_cell, cell(exit)).unwrap_or_default();
    if !into.is_empty() && !out.is_empty() {
        for c in into.into_iter().chain(out) {
            // `FUN_1400a48b0(.., 0)`: a bridge's deck over the cell counts.
            route.push(crate::town::floor_ground(floors, size, terrain, c));
        }
    }
    for p in &river[tail.min(river.len())..] {
        route.push([p[0], p[1], 0.0]);
    }
    // `FUN_1400ac7e0`: the route point nearest the dock, and the flow two points back.
    let mut yaw = dock_yaw;
    if 2 <= route.len() {
        let d2 = |q: [f32; 3]| {
            let v = [dock[0] - q[0], dock[1] - q[1], dock[2] - q[2]];
            v[0] * v[0] + v[1] * v[1] + v[2] * v[2]
        };
        let mut k = 0;
        let mut best = d2(route[0]);
        for (j, &q) in route.iter().enumerate() {
            let d = d2(q);
            if d < best {
                best = d;
                k = j;
            }
        }
        let a = route[k];
        let b = route[k.saturating_sub(2)];
        let flow = normalize([a[0] - b[0], a[1] - b[1], 0.0]);
        let mut dir = crate::math::from_yaw_pitch(dock_yaw, 0.0);
        if flow[0] * dir[0] + flow[1] * dir[1] < 0.0 {
            dir = [-dir[0], -dir[1], 0.0];
        }
        yaw = heading(dir);
    }
    (route, yaw)
}

impl Sim<'_, '_> {
    fn boat_group(
        &self,
        id: usize,
    ) -> Option<std::sync::Arc<banished_formats::types::AnimationGroup>> {
        let b = self.wildlife.boats.get(&id)?;
        self.wildlife.groups.get(&(b.template, 0)).cloned()
    }

    /// `FUN_14010f6c0` on the boat: clip `clip` at full speed.
    fn boat_clip(&mut self, id: usize, clip: usize) {
        let Some(group) = self.boat_group(id) else {
            return;
        };
        if let Some(b) = self.wildlife.boats.get_mut(&id) {
            b.character.play(clip, &group, 1.0);
        }
    }

    /// The trader boat entity (`FUN_1400c8bf0` of TradeDescription `+0x30`), set up for its
    /// trading post (`FUN_140046040`) and activated: state 0's enter (`FUN_140045d50`) puts it
    /// at the route's start facing its first point six away.
    pub(crate) fn add_boat(&mut self, template: usize, post: usize) -> Option<usize> {
        let kind = *self.wildlife.boat_kinds.get(&template)?;
        // The trading post's "boat" point (`FUN_140128d50`).
        let (dock, dock_yaw) = self.named_point(post, "boat")?;
        let (route, yaw) = route(
            self.size,
            self.terrain,
            &self.cells.floors,
            self.pathfinder,
            self.river,
            dock,
            dock_yaw,
        );
        let start = *route.first()?;
        let id = self.placed.len();
        let t = &self.templates[template];
        self.placed.push(crate::nature::seeding::Placed {
            template,
            bucket: t.bucket,
            cell: (0, 0),
            footprint: (1, 1),
            position: start,
            yaw: 0.0,
            variant: 0,
            mesh: usize::MAX,
            growth: 0.0,
            growth_state: None,
            spawn_state: None,
            alive: true,
            removal_requested: false,
            animation: None,
            scale: [1.0; 4],
        });
        let mut boat = Boat {
            template,
            character: Character::new(kind.character, start, 1.0),
            state: 0,
            post: Some(post),
            dock,
            dock_yaw: yaw,
            target: start,
            route,
            index: 0,
        };
        if length([boat.target[0] - start[0], boat.target[1] - start[1], 0.0]) < NEAR {
            if let Some(i) = ahead(&boat.route, boat.index, start, AHEAD) {
                boat.index = i;
                let q = boat.route[i as usize];
                boat.target = [q[0], q[1], 0.0];
            }
        }
        let h = heading([boat.target[0] - start[0], boat.target[1] - start[1], 0.0]);
        boat.character.yaw = h + kind.character.yaw_offset;
        boat.character.target_heading = h;
        boat.character.tree_slot = self.wildlife.tree.insert(
            crate::behaviour::character_tree::Who::Boat(id),
            boat.character.position,
            boat.character.desc.radius,
        );
        self.wildlife.boats.insert(id, boat);
        self.boat_clip(id, 1);
        *self.models_dirty = true;
        Some(id)
    }

    /// A named interaction point in the world and its heading (`FUN_140128d50`).
    fn named_point(&self, e: usize, name: &str) -> Option<([f32; 3], f32)> {
        let p = &self.placed[e];
        let t = &self.templates[p.template];
        let (local, yaw) = t
            .named_points
            .iter()
            .find(|(n, _, _)| n == name)
            .map(|(_, p, y)| (*p, *y))?;
        // The point's `FromYawPitch(yaw, 0)` turned by the entity transform, then `Heading`.
        let model = crate::math::Transform::of_model(p.yaw, p.position);
        Some((model.apply(local), model.heading_of(yaw)))
    }

    /// `FUN_140046d30`: within three of the point it heads for, the next one six on (none at
    /// the route's end); it heads there.
    fn boat_sail(&mut self, id: usize) -> bool {
        let Some(b) = self.wildlife.boats.get_mut(&id) else {
            return false;
        };
        let p = b.character.position;
        let mut ok = true;
        if length([b.target[0] - p[0], b.target[1] - p[1], b.target[2] - p[2]]) < NEAR {
            match ahead(&b.route, b.index, p, AHEAD) {
                Some(i) => {
                    b.index = i;
                    let q = b.route[i as usize];
                    b.target = [q[0], q[1], 0.0];
                }
                None => ok = false,
            }
        }
        if ok {
            let d = normalize([b.target[0] - p[0], b.target[1] - p[1], b.target[2] - p[2]]);
            let dir = normalize([d[0], d[1], 0.0]);
            b.character.flag_28 = false;
            b.character.direction = dir;
            b.character.target_heading = heading(dir);
        }
        ok
    }

    fn boat_state(&mut self, id: usize, state: i8) {
        let Some(b) = self.wildlife.boats.get_mut(&id) else {
            return;
        };
        b.state = state;
        let clip = match state {
            1 => 2,
            2 => 0,
            3 => 3,
            _ => 1,
        };
        self.boat_clip(id, clip);
        if state == 2 {
            // `FUN_1400458d0`: docked; the trading post hears of it (message 9).
            if let Some(post) = self.wildlife.boats.get(&id).and_then(|b| b.post) {
                self.trade_message(post, DOCKED_MESSAGE);
            }
        }
    }

    fn remove_boat(&mut self, id: usize) {
        if let Some(boat) = self.wildlife.boats.remove(&id) {
            self.wildlife.tree.remove(boat.character.tree_slot);
            self.placed[id].alive = false;
            *self.models_dirty = true;
        }
    }

    /// The boats' StateMachine updates: 0 sailing in, gone at the route's end (the post hears it,
    /// message 0xb), approaching within eight of the dock; 1 slowing to the dock, docked within
    /// 0.2; 3 heading back to the route, sailing out within three; 4 gone at the route's end.
    ///
    /// Original: `FUN_140045bb0`, `FUN_140045950`, `FUN_140045670`, `FUN_1400455a0`.
    pub fn update_boats(&mut self) {
        let ids: Vec<usize> = self.wildlife.boats.keys().copied().collect();
        for id in ids {
            let Some(state) = self.wildlife.boats.get(&id).map(|b| b.state) else {
                continue;
            };
            match state {
                0 => {
                    if !self.boat_sail(id) {
                        let post = self.wildlife.boats[&id].post;
                        self.remove_boat(id);
                        if let Some(post) = post {
                            self.trade_message(post, GONE_MESSAGE);
                        }
                        continue;
                    }
                    let b = &self.wildlife.boats[&id];
                    let p = b.character.position;
                    let v = [p[0] - b.dock[0], p[1] - b.dock[1], p[2] - b.dock[2]];
                    if v[0] * v[0] + v[1] * v[1] + v[2] * v[2] < APPROACH {
                        self.boat_state(id, 1);
                    }
                }
                1 => {
                    let b = self.wildlife.boats.get_mut(&id).unwrap();
                    let p = b.character.position;
                    let v = [b.dock[0] - p[0], b.dock[1] - p[1], b.dock[2] - p[2]];
                    let len = length(v);
                    let factor = (len * SLOW).clamp(SLOWEST, 1.0);
                    // `FUN_14010f5b0`.
                    b.character.flag_28 = true;
                    b.character.direction = normalize(v);
                    b.character.target_heading = b.dock_yaw;
                    b.character.speed = b.character.scale * factor;
                    if len < DOCKED {
                        self.boat_state(id, 2);
                    }
                }
                3 => {
                    let b = self.wildlife.boats.get_mut(&id).unwrap();
                    let p = b.character.position;
                    let v = [b.target[0] - p[0], b.target[1] - p[1], b.target[2] - p[2]];
                    let len = length(v);
                    let dir = normalize([v[0], v[1], 0.0]);
                    b.character.flag_28 = false;
                    b.character.direction = dir;
                    b.character.target_heading = heading(dir);
                    if len < NEAR {
                        self.boat_state(id, 4);
                    }
                }
                4 if !self.boat_sail(id) => self.remove_boat(id),
                _ => {}
            }
        }
    }

    /// The boat's state messages: 0 in 2 (`FUN_140045860`) it casts off; 1 in 0..2
    /// (`FUN_140045fd0`) it sails out.
    pub(crate) fn boat_message(&mut self, id: usize, msg: i32) {
        let Some(state) = self.wildlife.boats.get(&id).map(|b| b.state) else {
            return;
        };
        match (state, msg) {
            (2, 0) => self.boat_state(id, 3),
            (0..=2, 1) => self.boat_state(id, 4),
            _ => {}
        }
    }

    /// `FUN_140046040(boat, none)`: the trading post is gone; the boat sails out.
    pub(crate) fn boat_orphan(&mut self, id: usize) {
        if let Some(b) = self.wildlife.boats.get_mut(&id) {
            b.post = None;
        }
        self.boat_message(id, 1);
    }
}
