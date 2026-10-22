//! Fighting a fire: finding water, the firefighter's work and the wells' bucket lines.

use super::*;

impl Sim<'_, '_> {
    /// `FUN_14008d6a0`: the nearest water within `max` (path rank, else `n` + distance) by
    /// rings of 8 × 8 blocks around the citizen's: a shore point (its cell walkable) or a
    /// well's first use point. A shore point gives (shore, water); a well a random use point
    /// and its entrance.
    pub(super) fn water_search(&mut self, cell: i32, max: i32) -> Option<([f32; 3], [f32; 3])> {
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
            return Some((self.ground_at(a), self.ground_at(w)));
        }
        let well = self.water.wells[(-1 - v) as usize]?;
        let count = self.use_point_count(well);
        let k = self.rng.rand_below(count as u32) as usize;
        let point = self.use_point(well, k).0;
        let entrance = self.entrance_cell(well);
        Some((point, self.ground_at(entrance)))
    }

    /// `FUN_14008dc00`: the block's sources nearer than the best so far.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn water_block(
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
    pub(super) fn fire_sequence(
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
