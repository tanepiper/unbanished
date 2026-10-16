//! TornadoComponent (vtable `0x140160ba0`, TornadoDescription): the environment forms one while
//! it rains (`Environment::update`); it crosses the map along a jagged line, flattening crops
//! and trees, wrecking buildings and killing animals and citizens near it. See
//! docs/TORNADO.md.
//!
//! - Setup (slot `+0x20`, `FUN_140058f00`): two random cells at least `+0x24` apart (at most a
//!   hundred tries), a 129-point line between them bent by midpoint displacement
//!   (`FUN_1400592e0`), the event "a tornado has touched down" (type 30, severity 3).
//! - Update (slot `+0x40`, `FUN_140058630`): when its particles are gone the entity goes;
//!   else it follows the line (`FUN_1400ac6b0`: within 3 of its point, the next one 10 on),
//!   wrecks what is near (`FUN_1400589a0`) or, at the end, stops forming particles, and moves
//!   `+0x20` a second toward its point on the ground.

use crate::jobs::Jobs;
use crate::random::Random;
use crate::scheduler::{seconds, ticks};
use crate::terrain::HeightGrid;
use crate::town::Sim;

/// TornadoDescription (serializer `FUN_1400596f0`).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct TornadoTemplate {
    /// `+0x20`: cells a second.
    pub speed: f32,
    /// `+0x24`: the ends' least distance in cells.
    pub min_distance: i32,
    /// `+0x28`: the time between wrecking passes.
    pub interval: i64,
    /// `+0x30`: the reach of a pass (cells, and the radius about the thrown point).
    pub reach: i32,
    /// `+0x34`: how far from it a pass throws its point.
    pub throw: i32,
}

/// The component's state (serializer `FUN_1400594e0`).
#[derive(Debug, Clone, PartialEq)]
pub struct Tornado {
    /// `+0x30`: the point it heads for; `+0x40` the line; `+0x50` the point's index.
    pub target: [f32; 3],
    pub path: Vec<[f32; 3]>,
    pub index: i32,
    /// `+0x28`: until the next wrecking pass.
    pub timer: i64,
    /// `+0x58`: where it is.
    pub position: [f32; 3],
    /// Its particle system still forms particles (`FUN_140054080(0)` stops it); the time
    /// since it stopped.
    pub emitting: bool,
    pub stopped: i64,
}

/// `PATH`: the line's points (`0x81`).
const POINTS: usize = 129;
/// The Tornado particles' longest life (`+0x60 + +0x64`, 30 ± 2 s): once it stops forming
/// them, the last is gone after this.
pub const FADE_SECONDS: f32 = 32.0;

fn normalize(v: [f32; 3]) -> ([f32; 3], f32) {
    let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if l == 0.0 {
        (v, 0.0)
    } else {
        ([v[0] / l, v[1] / l, v[2] / l], l)
    }
}

/// `FUN_1400592e0(path, a, b)`: the point midway between `a` and `b` moves off the line, to its
/// left, by up to a quarter of their distance (`RandFloat`); then each half.
fn displace(path: &mut [[f32; 3]], a: usize, b: usize, rng: &mut Random) {
    let (p, q) = (path[a], path[b]);
    let (d, length) = normalize([q[0] - p[0], q[1] - p[1], q[2] - p[2]]);
    let mid = [
        (p[0] + q[0]) * 0.5,
        (p[1] + q[1]) * 0.5,
        (p[2] + q[2]) * 0.5,
    ];
    let (side, _) = normalize([-d[1], d[0], 0.0]);
    let m = (a + b) >> 1;
    let r = rng.rand_float();
    let s = length * 0.25;
    path[m] = [
        side[0] * s * r + mid[0],
        side[1] * s * r + mid[1],
        side[2] * s * r + mid[2],
    ];
    if 1 < m - a {
        displace(path, a, m, rng);
    }
    if 1 < b - m {
        displace(path, m, b, rng);
    }
}

impl Tornado {
    /// `FUN_140058f00` on a map `size` cells a side.
    pub fn new(
        t: &TornadoTemplate,
        size: usize,
        terrain: &HeightGrid,
        rng: &mut Random,
    ) -> Tornado {
        let n = size as u32;
        let mut tries = 0;
        let (mut x0, mut y0, mut x1, mut y1);
        loop {
            x0 = rng.rand_below(n) as i32;
            y0 = rng.rand_below(n) as i32;
            x1 = rng.rand_below(n) as i32;
            y1 = rng.rand_below(n) as i32;
            if 99 < tries {
                break;
            }
            tries += 1;
            let (dx, dy) = (x1 - x0, y1 - y0);
            if t.min_distance * t.min_distance <= dx * dx + dy * dy {
                break;
            }
        }
        let ground = |x: i32, y: i32| {
            let half = (size as i32 >> 1) as f32 - 0.5;
            let (px, py) = (x as f32 - half, y as f32 - half);
            let mut normal = [0.0; 3];
            [px, py, terrain.ground(px, py, &mut normal)]
        };
        let start = ground(x0, y0);
        let end = ground(x1, y1);
        let mut path = vec![[0.0; 3]; POINTS];
        path[0] = [start[0], start[1], 0.0];
        path[POINTS - 1] = [end[0], end[1], 0.0];
        displace(&mut path, 0, POINTS - 1, rng);
        Tornado {
            target: path[0],
            path,
            index: 0,
            timer: 0,
            position: start,
            emitting: true,
            stopped: 0,
        }
    }

    /// `FUN_1400ac6b0(state, position, 10, 3)`: within 3 of its point, the next point at least
    /// 10 on (`FUN_1400ac590`); false past the end.
    pub fn follow(&mut self) -> bool {
        let p = [self.position[0], self.position[1], 0.0];
        let (dx, dy) = (self.target[0] - p[0], self.target[1] - p[1]);
        if (dx * dx + dy * dy).sqrt() < 3.0 {
            let mut i = self.index;
            loop {
                i += 1;
                if self.path.len() as i32 <= i {
                    return false;
                }
                let q = self.path[i as usize];
                let (dx, dy, dz) = (p[0] - q[0], p[1] - q[1], p[2]);
                if 100.0 <= dx * dx + dy * dy + dz * dz {
                    break;
                }
            }
            self.index = i;
            let q = self.path[i as usize];
            self.target = [q[0], q[1], 0.0];
        }
        true
    }

    /// The point a wrecking pass throws (`FUN_1400589a0`): `RandFloatTol × 180°` about it, up
    /// to `+0x34` away (`RandFloat`).
    pub fn throw(&self, t: &TornadoTemplate, rng: &mut Random) -> [f32; 3] {
        let angle = rng.rand_float_tol() * 180.0 * 0.017_453_292;
        let r = rng.rand_float();
        let reach = t.throw as f32;
        let (sin, cos) = ((angle as f64).sin() as f32, (angle as f64).cos() as f32);
        let v = [sin * reach, cos * reach, reach * 0.0];
        [
            v[0] * r + self.position[0],
            v[1] * r + self.position[1],
            v[2] * r + self.position[2],
        ]
    }
}

impl Sim<'_, '_> {
    /// Every tornado's update (`FUN_140058630`) for `dt` ticks.
    pub fn update_tornadoes(&mut self, jobs: &mut Jobs, dt: i64) {
        let ids: Vec<usize> = self.tornadoes.keys().copied().collect();
        for e in ids {
            let Some(t) = self.templates[self.placed[e].template].tornado else {
                continue;
            };
            let tornado = self.tornadoes.get_mut(&e).unwrap();
            // Its particles are gone: the entity goes (`FUN_1400c8370`).
            if !tornado.emitting {
                tornado.stopped += dt;
                if ticks(FADE_SECONDS) <= tornado.stopped {
                    self.tornadoes.remove(&e);
                    self.placed[e].alive = false;
                    *self.models_dirty = true;
                    continue;
                }
            }
            let going = tornado.follow();
            let (d, _) = normalize([
                tornado.target[0] - tornado.position[0],
                tornado.target[1] - tornado.position[1],
                tornado.target[2] - tornado.position[2],
            ]);
            if !going {
                tornado.emitting = false;
            } else {
                self.wreck(jobs, e, &t, dt);
            }
            let tornado = self.tornadoes.get_mut(&e).unwrap();
            let step = seconds(dt) * t.speed;
            let p = [
                d[0] * step + tornado.position[0],
                d[1] * step + tornado.position[1],
                d[2] * step + tornado.position[2],
            ];
            // `FUN_1400a48b0(…, skip floors)`.
            let mut normal = [0.0; 3];
            tornado.position = [p[0], p[1], self.terrain.ground(p[0], p[1], &mut normal)];
            self.placed[e].position = tornado.position;
        }
    }

    /// `FUN_1400589a0`: every `+0x28` a pass: a point thrown about the tornado, the natural
    /// resource on its cell, every entity within `+0x30` of it (`FUN_140097e60`), then the
    /// objects of every cell within `+0x30` of the tornado (`FUN_140058d20` each).
    fn wreck(&mut self, jobs: &mut Jobs, e: usize, t: &TornadoTemplate, dt: i64) {
        let tornado = self.tornadoes.get_mut(&e).unwrap();
        tornado.timer -= dt;
        if 0 < tornado.timer {
            return;
        }
        tornado.timer += t.interval;
        let point = tornado.throw(t, self.rng);
        let position = tornado.position;
        let n = self.size as i32;
        let half = (n >> 1) as f32;
        let cell = |p: [f32; 3]| {
            let x = ((p[0] + half) as i32).clamp(0, n - 1);
            let y = ((p[1] + half) as i32).clamp(0, n - 1);
            (x, y)
        };
        let (x, y) = cell(point);
        if let Some(o) = self.cells.objects[(y * n + x) as usize] {
            if self.templates[self.placed[o].template].natural.is_some() {
                self.hit(jobs, Hit::Entity(o));
            }
        }
        // The entities whose models stand within the reach of the point.
        let reach = t.reach as f32;
        let near = |p: [f32; 3]| {
            let (dx, dy, dz) = (p[0] - point[0], p[1] - point[1], p[2] - point[2]);
            dx * dx + dy * dy + dz * dz < reach * reach
        };
        let mut hits: Vec<Hit> = Vec::new();
        for (i, p) in self.placed.iter().enumerate() {
            if p.alive && i != e && !self.wildlife.animals.contains_key(&i) && near(p.position) {
                hits.push(Hit::Entity(i));
            }
        }
        for (&a, animal) in &self.wildlife.animals {
            if !animal.dying && near(animal.character.position) {
                hits.push(Hit::Entity(a));
            }
        }
        for (c, ch) in self.characters.iter().enumerate() {
            if self.groups.get(c).copied().flatten().is_some() && near(ch.position) {
                hits.push(Hit::Citizen(c));
            }
        }
        for h in hits {
            self.hit(jobs, h);
        }
        let (cx, cy) = cell(position);
        for y in cy - t.reach..=cy + t.reach {
            for x in cx - t.reach..=cx + t.reach {
                if x < 0 || y < 0 || n <= x || n <= y {
                    continue;
                }
                if let Some(o) = self.cells.objects[(y * n + x) as usize] {
                    self.hit(jobs, Hit::Entity(o));
                }
            }
        }
    }

    /// `FUN_140058d20`: a growing natural resource (a crop, a tree) not already falling loses
    /// its growth and is removed; a building that burns (Build and Fire) and is not damaged
    /// yet is wrecked to rubble; an animal goes (`FUN_1400c9ea0`); a citizen dies (code 5).
    fn hit(&mut self, jobs: &mut Jobs, h: Hit) {
        let e = match h {
            Hit::Citizen(c) => {
                if !self.tornado_kills.contains(&c) {
                    self.tornado_kills.push(c);
                }
                return;
            }
            Hit::Entity(e) => e,
        };
        if !self.placed[e].alive {
            return;
        }
        let template = &self.templates[self.placed[e].template];
        if template.natural.is_some() && template.growth.is_some() {
            if self.placed[e].animation.is_none() && !self.placed[e].removal_requested {
                self.placed[e].growth = 0.0;
                self.placed[e].removal_requested = true;
                self.animate_resource(e, 2);
                *self.models_dirty = true;
            }
            return;
        }
        let building = self
            .sites
            .get(&e)
            .is_some_and(|s| s.fire.is_some() && !s.destroyed)
            && self.has_build(e);
        if building {
            if !self.is_damaged(e) && self.damage(jobs, e, 0) {
                self.to_rubble(jobs, e);
            }
            return;
        }
        if self.wildlife.animals.contains_key(&e) {
            self.wildlife.kill(e);
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum Hit {
    Entity(usize),
    Citizen(usize),
}
