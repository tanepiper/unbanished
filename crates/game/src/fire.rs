//! Fire: FireComponent (vtable `0x14015f480`, factory `FUN_1400055e0`), its work FireWork
//! (vtable `0x1401676c0`), WellComponent (vtable `0x140160fb0`, states `FUN_14005da70`) and the
//! town's water sources (`Game +0xf0`: `+0x130` shore points, `+0xe0` wells, `+0x140` per 8 × 8
//! block). See docs/FIRE.md.
//!
//! A building catches fire now and then (or from a burning neighbour), is damaged for half the
//! burn (DestroyComponent), and marks the cells around it as an emergency: citizens there drop
//! what they do and carry buckets from the nearest water to it. Enough buckets, or the burn
//! running out, put it out; it is left as rubble.

use banished_formats::game::FireDescription;

use crate::character::{work_facing, Action, Component, Message, MoveTo};
use crate::jobs::{Jobs, Work, WorkKind};
use crate::scheduler::{seconds, ticks};
use crate::seeding::Cells;
use crate::town::{cell_ground, Sim};

/// `DAT_140178934`.
const PERIOD: f32 = 300.0;
/// The map layer counting fires over each cell (`+0x22`; total `Map +0x9c`).
pub const LAYER: usize = 6;
/// FireWork's registration mask (category 14, emergencies).
const MASK: u32 = 0x4000;
/// Event type 31: a building caught fire.
const CAUGHT: i32 = 0x1f;
/// `Game +0xd0` bit 6: fires are off (no disasters).
pub const NO_FIRES: u32 = 0x40;
/// `Map::Reachable` mask of the water search.
const WATER_MASK: u32 = 0xe07;
/// `DAT_140178824`, `DAT_140178888`.
const THROW: f32 = 0.75;
const RUN: f32 = 3.5;

/// FireComponent: `+0x28` buckets thrown, `+0x2c` buckets at the last spread, `+0x30` burn
/// left (burning while above zero), `+0x38` towards the next spread, `+0x40` towards the next
/// ignition check.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Fire {
    pub buckets: i32,
    pub buckets_seen: i32,
    pub burn: i64,
    pub spread: i64,
    pub timer: i64,
}

/// The town's water sources.
#[derive(Debug, Clone, Default)]
pub struct Water {
    /// `+0x130`: (shore cell, water cell) pairs.
    pub points: Vec<(i32, i32)>,
    /// `+0xe0`: WellComponents (a removed well leaves its slot empty).
    pub wells: Vec<Option<usize>>,
    /// `+0x140`: per 8 × 8 block, the point indices (wells as `-1 - i`).
    pub blocks: Vec<Vec<i32>>,
    /// Cells per map side.
    pub size: i32,
}

impl Water {
    /// `FUN_1400a43f0`'s scan: every ground cell (type 2) next to river or lake water (types 7,
    /// 8) is a point, with the first such neighbour by rows (`FUN_14008e050`).
    pub fn new(cells: &Cells) -> Water {
        let n = cells.size as i32;
        let b = (n >> 3).max(0) as usize;
        let mut water = Water {
            blocks: vec![Vec::new(); b * b],
            size: n,
            ..Default::default()
        };
        for y in 0..n {
            for x in 0..n {
                let cell = y * n + x;
                if cells.types[cell as usize].0 != 2 {
                    continue;
                }
                'found: for dy in -1..=1 {
                    for dx in -1..=1 {
                        let (wx, wy) = (x + dx, y + dy);
                        if wx < 0 || wy < 0 || n <= wx || n <= wy {
                            continue;
                        }
                        let w = wy * n + wx;
                        if (7..=8).contains(&cells.types[w as usize].0) {
                            let i = water.points.len() as i32;
                            water.points.push((cell, w));
                            water.add_entry(x, y, i);
                            break 'found;
                        }
                    }
                }
            }
        }
        water
    }

    fn block(&self, x: i32, y: i32) -> Option<usize> {
        let b = self.size >> 3;
        let (bx, by) = (x >> 3, y >> 3);
        (0 <= bx && bx < b && 0 <= by && by < b).then_some((b * by + bx) as usize)
    }

    /// `FUN_14008df50`.
    pub fn add_entry(&mut self, x: i32, y: i32, v: i32) {
        if let Some(k) = self.block(x, y) {
            self.blocks[k].push(v);
        }
    }

    /// `FUN_14008deb0`.
    fn remove_entry(&mut self, x: i32, y: i32, v: i32) {
        if let Some(k) = self.block(x, y) {
            self.blocks[k].retain(|&e| e != v);
        }
    }

    /// `FUN_140097180`: the first empty slot, else a new one, filed under the well's map cell.
    pub fn add_well(&mut self, e: usize, cell: (i32, i32)) {
        let i = match self.wells.iter().position(Option::is_none) {
            Some(i) => {
                self.wells[i] = Some(e);
                i
            }
            None => {
                self.wells.push(Some(e));
                self.wells.len() - 1
            }
        };
        self.add_entry(cell.0, cell.1, -1 - i as i32);
    }

    /// `FUN_140097070`.
    pub fn remove_well(&mut self, e: usize, cell: (i32, i32)) {
        if let Some(i) = self.wells.iter().position(|&w| w == Some(e)) {
            self.remove_entry(cell.0, cell.1, -1 - i as i32);
            self.wells[i] = None;
        }
    }
}

/// `Citizen::HasEmergency` (`FUN_14011b0b0`): a fire's layer over the home's or the
/// workplace's map-rectangle centre, or the citizen's own cell.
pub fn has_emergency(
    cells: &Cells,
    placed: &[crate::seeding::Placed],
    home: Option<usize>,
    workplace: Option<usize>,
    position: [f32; 3],
) -> bool {
    if cells.layer_totals[LAYER] < 1 {
        return false;
    }
    let n = cells.size as i32;
    for e in [home, workplace].into_iter().flatten() {
        let p = &placed[e];
        let cell = (p.cell.1 + (p.footprint.1 >> 1)) * n + p.cell.0 + (p.footprint.0 >> 1);
        if cells.layers[cell as usize][LAYER] != 0 {
            return true;
        }
    }
    let half = (n >> 1) as f32;
    let x = ((position[0] + half) as i32).clamp(0, n - 1);
    let y = ((position[1] + half) as i32).clamp(0, n - 1);
    cells.layers[(y * n + x) as usize][LAYER] != 0
}

impl Sim<'_, '_> {
    fn fire_desc(&self, e: usize) -> Option<FireDescription> {
        self.site_templates[self.sites.get(&e)?.template].fire
    }

    fn fire(&mut self, e: usize) -> Option<&mut Fire> {
        self.sites.get_mut(&e)?.fire.as_mut()
    }

    /// FireComponent's scheduled update (`FUN_140049910`) of every building that has one:
    /// not burning, the ignition odds (unless fires are off); burning, the spread and the
    /// burn running out.
    pub fn update_fires(&mut self, jobs: &mut Jobs, visits: &[(usize, i64)]) {
        for &(e, dt) in visits {
            if !self
                .sites
                .get(&e)
                .is_some_and(|s| s.fire.is_some() && !s.destroyed)
            {
                continue;
            }
            let Some(d) = self.fire_desc(e) else { continue };
            let off = self.game_flags & NO_FIRES != 0;
            let Some(f) = self.fire(e) else { continue };
            if f.burn == 0 {
                if off {
                    continue;
                }
                f.timer += dt;
                if self.odds(e, d.years, d.odds) {
                    self.ignite(jobs, e);
                }
                continue;
            }
            f.burn -= dt;
            f.spread -= dt;
            if 0 < f.burn && f.spread < 1 {
                self.spread_fire(jobs, e, &d);
            }
            if self.fire(e).is_some_and(|f| f.burn < 1) {
                self.put_out(jobs, e);
            }
        }
    }

    /// `FUN_1400ae760(timer, [years, odds], rng)`: one in `years × 12 × 300 s × odds` per
    /// second since the last check; the time restarts.
    fn odds(&mut self, e: usize, years: f32, odds: f32) -> bool {
        let f = self.fire(e).unwrap();
        let elapsed = seconds(f.timer);
        f.timer = 0;
        if odds <= 0.0 {
            return false;
        }
        let span = years * 12.0 * PERIOD;
        self.rng.rand_float() < elapsed / (span * odds)
    }

    /// `FireComponent::Ignite` (`FUN_140049f70`): a building neither damaged nor being torn
    /// down is damaged for half the burn; it burns, gets its FireWork (prioritised), marks
    /// the emergency around it and, as the first fire, posts event 31.
    /// The particles and the fire sound follow `burn` in the app (docs/PARTICLES.md,
    /// `Ambience::drive_entities`). (AutoSlow, settings `+0x17`, comes from the event:
    /// `EventLog::add`.)
    pub fn ignite(&mut self, jobs: &mut Jobs, e: usize) {
        if self.is_damaged(e) || self.is_demolishing(e) {
            return;
        }
        let Some(d) = self.fire_desc(e) else { return };
        let total = ticks(d.burn_months * PERIOD + d.burn);
        if !self.damage(jobs, e, ticks(seconds(total) * 0.5)) {
            return;
        }
        if let Some(f) = self.fire(e) {
            f.burn = total;
            f.spread = ticks(d.interval);
            f.buckets = 0;
            f.buckets_seen = 0;
        }
        let work = Work::new(
            e,
            self.rect_cell(e),
            self.professions.laborer,
            WorkKind::Fire(crate::herbalist::HerbalistWork::default()),
        );
        self.set_work(jobs, e, work, MASK, false);
        let id = self.sites[&e].work_id;
        jobs.prioritize(id);
        if self.cells.layer_totals[LAYER] < 1 {
            self.site_events.push((CAUGHT, 3, e));
        }
        self.mark_emergency(e, &d, 1);
    }

    /// `FUN_14004a320`: the fire layer over the map rectangle grown by the radius
    /// (`FUN_1400a4f50`).
    fn mark_emergency(&mut self, e: usize, d: &FireDescription, delta: i32) {
        let (x, y) = self.placed[e].cell;
        let (w, h) = self.placed[e].footprint;
        let r = d.radius;
        let n = self.size as i32;
        for cy in y - r..=y + h - 1 + r {
            for cx in x - r..=x + w - 1 + r {
                if 0 <= cx && 0 <= cy && cx < n && cy < n {
                    let v = &mut self.cells.layers[(cy * n + cx) as usize][LAYER];
                    *v = (*v as i32 + delta) as u8;
                }
            }
        }
        self.cells.layer_totals[LAYER] += delta as i16;
    }

    /// `FireComponent::PutOut` (`FUN_140049e20`): it is rubble now; its work and emergency
    /// go; the particles fade and the fire sound stops with `burn` at 0 (in the app).
    pub fn put_out(&mut self, jobs: &mut Jobs, e: usize) {
        let Some(d) = self.fire_desc(e) else { return };
        if let Some(f) = self.fire(e) {
            f.burn = 0;
            f.spread = 0;
        }
        self.to_rubble(jobs, e);
        self.remove_work(jobs, e);
        self.mark_emergency(e, &d, -1);
    }

    /// FireComponent's message slot (`FUN_1400499f0`): message 0, a bucket of water.
    pub fn fire_message(&mut self, jobs: &mut Jobs, e: usize, msg: i32) {
        if msg != 0 {
            return;
        }
        let Some(d) = self.fire_desc(e) else { return };
        let Some(f) = self.fire(e) else { return };
        f.buckets += 1;
        f.burn -= ticks(d.interval * PERIOD / d.buckets as f32);
        if f.burn < 1 || d.buckets <= f.buckets {
            self.put_out(jobs, e);
        }
    }

    /// `FUN_140049a90`: each spread interval due, a random cell within `min(w, h)` of the
    /// rectangle; a building there with a FireComponent catches with its odds (halved in
    /// rain, and halved again when more than a fifth of its buckets were thrown here since the
    /// last spread).
    fn spread_fire(&mut self, jobs: &mut Jobs, e: usize, d: &FireDescription) {
        let n = self.size as i32;
        let (x, y) = self.placed[e].cell;
        let (w, h) = self.placed[e].footprint;
        let m = w.min(h);
        let x1 = (w - 1 + m + x).min(n - 1);
        let y1 = (h - 1 + m + y).min(n - 1);
        let x0 = (x - m).max(0);
        let y0 = y - m;
        loop {
            let Some(f) = self.fire(e) else { return };
            if 0 < f.spread {
                break;
            }
            f.spread += ticks(d.interval * PERIOD);
            let cx = self.rng.rand_range(x0, x1);
            let cy = self.rng.rand_range(y0, y1);
            let cell = cy * n + cx;
            // Below the map the original reads outside it; nothing burns there.
            let other = (0 <= cell && cell < n * n)
                .then(|| self.cells.occupants[cell as usize])
                .flatten();
            if let Some(o) = other {
                if let Some(od) = self.fire_desc(o) {
                    let mut chance = od.spread;
                    if self.precipitation {
                        chance *= 0.5;
                    }
                    let f = self.fire(e).unwrap();
                    if od.buckets / 5 < f.buckets - f.buckets_seen {
                        chance *= 0.5;
                    }
                    if self.rng.rand_float() < chance {
                        self.ignite(jobs, o);
                    }
                }
            }
            if self.fire(e).is_none_or(|f| f.burn <= 0) {
                break;
            }
        }
        if let Some(f) = self.fire(e) {
            f.buckets_seen = f.buckets;
        }
    }

    /// `FUN_14008d6a0`: the nearest water within `max` (path rank, else `n` + distance) by
    /// rings of 8 × 8 blocks around the citizen's: a shore point (its cell walkable) or a
    /// well's first use point. A shore point gives (shore, water); a well a random use point
    /// and its entrance.
    fn water_search(&mut self, cell: i32, max: i32) -> Option<([f32; 3], [f32; 3])> {
        let n = self.size as i32;
        let b = n >> 3;
        let (bx, by) = ((cell % n) >> 3, (cell / n) >> 3);
        let mut best = max;
        let mut found: Option<i32> = None;
        let mut r = -1;
        loop {
            if best <= r.max(0) * 8 {
                break;
            }
            let r1 = r + 1;
            for dy in -r1..=r1 {
                let y = by + dy;
                if 0 <= y && y < b {
                    if 0 <= bx - r1 && bx - r1 < b {
                        self.water_block(y * b + bx - r1, -r1, dy, cell, &mut best, &mut found);
                    }
                    if 0 <= bx + r1 && bx + r1 < b {
                        self.water_block(y * b + bx + r1, r1, dy, cell, &mut best, &mut found);
                    }
                }
            }
            for dx in 1 - r1..=r1 - 1 {
                let x = bx + dx;
                if 0 <= x && x < b {
                    if 0 <= by - r1 && by - r1 < b {
                        self.water_block((by - r1) * b + x, dx, -r1, cell, &mut best, &mut found);
                    }
                    if 0 <= by + r1 && by + r1 < b {
                        self.water_block((by + r1) * b + x, dx, r1, cell, &mut best, &mut found);
                    }
                }
            }
            r = r1;
        }
        let v = found?;
        if 0 <= v {
            let (a, w) = self.water.points[v as usize];
            return Some((
                cell_ground(self.size, self.terrain, a),
                cell_ground(self.size, self.terrain, w),
            ));
        }
        let well = self.water.wells[(-1 - v) as usize]?;
        let count = self.templates[self.placed[well].template].use_points.len();
        let k = self.rng.rand_below(count as u32) as usize;
        let point = self.use_point(well, k).0;
        let entrance = self.entrance_cell(well);
        Some((point, cell_ground(self.size, self.terrain, entrance)))
    }

    /// `FUN_14008dc00`: the block's sources nearer than the best so far.
    #[allow(clippy::too_many_arguments)]
    fn water_block(
        &mut self,
        block: i32,
        dx: i32,
        dy: i32,
        cell: i32,
        best: &mut i32,
        found: &mut Option<i32>,
    ) {
        let entries = self.water.blocks[block as usize].clone();
        let d = (dy * 8) * (dy * 8) + (dx * 8) * (dx * 8);
        for v in entries {
            if *best * *best < d {
                continue;
            }
            let target = if v < 0 {
                let Some(well) = self.water.wells.get((-1 - v) as usize).copied().flatten() else {
                    continue;
                };
                self.cell_of(self.use_point(well, 0).0)
            } else {
                let a = self.water.points[v as usize].0;
                if (1u32 << (self.cells.types[a as usize].0 & 0x1f)) & WATER_MASK == 0 {
                    continue;
                }
                a
            };
            if !self.reachable_by(WATER_MASK, cell, target) {
                continue;
            }
            let rank = self.rank_or_distance(cell, target);
            if rank < *best {
                *best = rank;
                *found = Some(v);
            }
        }
    }

    /// FireWork's slot `+0x20` (`FUN_1400eeaf0`): below its most firefighters, the nearest
    /// water; then the burning building's use point nearest that water; within patience,
    /// the bucket run.
    pub(crate) fn fire_try(&mut self, work: &mut Work, c: usize, time: i64) -> u8 {
        let e = work.entity;
        let Some(d) = self.fire_desc(e) else { return 0 };
        let WorkKind::Fire(fw) = &work.kind else {
            return 0;
        };
        if d.workers <= fw.workers.len() as i32 {
            return 1;
        }
        let cell = self.citizen_cell(c);
        let Some((water, second)) = self.water_search(cell, self.rules.water_distance) else {
            return 0;
        };
        // `FUN_140127fd0(interact, 0, ..)`: the use point nearest the water.
        let from = self.cell_of(water);
        let mut best = 0x7fffffff;
        let mut point = None;
        for &x in &self.interact_entities(e) {
            for k in 0..self.templates[self.placed[x].template].use_points.len() {
                let p = self.use_point(x, k).0;
                let pc = self.cell_of(p);
                if self.reachable(from, pc) {
                    let r = self.rank_or_distance(from, pc);
                    if r < best {
                        best = r;
                        point = Some(p);
                    }
                }
            }
        }
        let Some(point) = point.filter(|_| best < 0xffff) else {
            return 0;
        };
        let pc = self.cell_of(point);
        if !self.patience(work, c, pc, cell, time) {
            return 0;
        }
        self.fire_sequence(c, work, water, second, point, e);
        self.took_job(c, work);
        if let WorkKind::Fire(fw) = &mut work.kind {
            fw.workers.insert(c);
        }
        2
    }

    /// `FUN_1400bfec0`: run to the water, fill the bucket, run to the fire, throw it (message
    /// 0 to the FireComponent), done.
    fn fire_sequence(
        &mut self,
        c: usize,
        work: &mut Work,
        water: [f32; 3],
        second: [f32; 3],
        point: [f32; 3],
        e: usize,
    ) {
        self.clear_citizen(c, Some(work));
        self.on_empty_think(c);
        let facing = work_facing(self.facing_centre(e), point);
        let heading = work_facing(second, water);
        self.push(c, self.perform(c, facing, 7, THROW, false));
        self.push(c, Action::ShowTool(-1));
        self.push(
            c,
            Action::CompleteTask {
                work: work.id,
                param: 0,
            },
        );
        self.push(
            c,
            Action::SendMessage(Message {
                component: Component::Fire,
                message: 0,
                entity: Some(e),
                a: 0,
                b: 0,
            }),
        );
        self.push(c, self.perform(c, facing, 6, THROW, false));
        let run = MoveTo::new(point, 4, 0, 0xe1f, 0, RUN, self.rng);
        self.push(c, Action::move_to(run));
        self.push(c, self.perform(c, heading, 7, THROW, false));
        self.push(c, Action::ShowTool(8));
        self.push(c, self.perform(c, heading, 6, THROW, false));
        let run = MoveTo::new(water, 1, 0, 0xe1f, 0, RUN, self.rng);
        self.push(c, Action::move_to(run));
    }

    /// Well state 2 enter (`FUN_14005d880` → `FUN_140097180`) and state 3 enter
    /// (`FUN_14005d840` → `FUN_140097070`).
    pub(crate) fn well_join(&mut self, e: usize, join: bool) {
        let cell = self.placed[e].cell;
        if join {
            self.water.add_well(e, cell);
        } else {
            self.water.remove_well(e, cell);
        }
    }
}
