//! Populations: their cells' capacity and count, regrowth every period, and the herds a population starts.

use super::*;

impl Wild<'_, '_> {
    /// Start one population of a new town: its entity, then its herds.
    ///
    /// Original: `FUN_140054b80`, called by `FUN_1400a5eb0`.
    pub fn start_population(&mut self, template: usize) {
        let Some(kind) = self.wildlife.population_kinds.get(&template).cloned() else {
            return;
        };
        let d = &kind.desc;
        let entity = self.create(template, [0.0; 3]);
        let n = self.n();
        let width = n / d.cell.max(1);
        let mut p = Population {
            entity,
            template,
            width,
            cells: vec![[0; 3]; (width * width) as usize],
            herd_width: n / d.herd_block.max(1),
            clock: 0,
            cursor: 0,
        };
        for i in 0..width * width {
            let c = p.capacity(d, self.cells, i);
            p.cells[i as usize] = [c, c, c];
        }
        let index = self.wildlife.populations.len();
        let herds = p.herd_width * p.herd_width;
        self.wildlife.populations.push(p);
        self.wildlife.kinds.insert(entity, Kind::Population(index));
        let Some(herd_t) = kind.herd else { return };
        let Some(hk) = self.wildlife.herd_kinds.get(&herd_t).cloned() else {
            return;
        };
        for i in 0..herds {
            let h = self.wildlife.herds.len();
            self.wildlife.herds.push(Herd {
                entity: usize::MAX,
                template: herd_t,
                population: index,
                index: i,
                animals: Vec::new(),
                moving: false,
                target: -1,
                rest: 0,
                regrow: 0,
            });
            let count =
                self.rng.rand_below((hk.desc.max - hk.desc.min) as u32) as i32 + hk.desc.min;
            self.spawn_herd(h, count);
            // Registered after its animals (`FUN_1400c9d70` follows `FUN_140126420`).
            let e = self.create(herd_t, [0.0; 3]);
            self.wildlife.herds[h].entity = e;
            self.wildlife.kinds.insert(e, Kind::Herd(h));
        }
    }

    /// `FUN_1400549d0`.
    pub(super) fn update_population(&mut self, p: usize, dt: i64) {
        let template = self.wildlife.populations[p].template;
        let d = self.wildlife.population_kinds[&template].desc.clone();
        let period = ticks(PERIOD);
        let pop = &mut self.wildlife.populations[p];
        pop.clock += dt;
        while period < pop.clock {
            pop.regrow(&d);
            pop.clock -= period;
        }
        if !d.refresh {
            return;
        }
        let total = pop.width * pop.width;
        for _ in 0..total / 16 {
            let i = pop.cursor;
            let cap = pop.capacity(&d, self.cells, i);
            let c = &mut pop.cells[i as usize];
            c[0] = cap;
            if cap < c[1] {
                c[1] = cap;
            }
            if cap < c[2] {
                c[2] = cap;
            }
            pop.cursor = if i + 1 == total { 0 } else { i + 1 };
        }
    }
}

impl Population {
    /// `FUN_1400544b0`: a cell's capacity: the description's when its block has habitat and
    /// nothing forbidden (cell types, or an occupant with `no_occupants`).
    pub(super) fn capacity(&self, d: &PopulationDescription, cells: &Cells, i: i32) -> i32 {
        let n = cells.size as i32;
        let cs = d.cell;
        let (y0, x0) = ((i / self.width) * cs, (i % self.width) * cs);
        let (mut habitat, mut forbid) = (0, 0u32);
        for y in y0..y0 + cs {
            for x in x0..x0 + cs {
                let c = (n * y + x) as usize;
                let bit = 1u32 << (cells.types[c].0 & 0x1f);
                if bit & d.habitat != 0 {
                    habitat += 1;
                }
                forbid |= d.forbidden & bit;
                if d.no_occupants && cells.occupants[c].is_some() {
                    forbid |= 1;
                }
            }
        }
        if forbid == 0 && habitat > 0 {
            d.capacity
        } else {
            0
        }
    }

    /// The side of a herd block in population cells.
    pub(super) fn block(&self) -> i32 {
        self.width / self.herd_width.max(1)
    }

    /// `FUN_140054880`: the share of a herd block's cells that are habitable.
    pub(super) fn habitable_share(&self, index: i32) -> f32 {
        let bs = self.block();
        let (row0, col0) = (
            (index / self.herd_width) * bs,
            (index % self.herd_width) * bs,
        );
        let mut n = 0;
        for r in 0..bs {
            for c in 0..bs {
                if self.cells[(self.width * (row0 + r) + col0 + c) as usize][0] > 0 {
                    n += 1;
                }
            }
        }
        n as f32 / (bs * bs) as f32
    }

    /// `FUN_140054600`: the map cell at the centre of a random habitable population cell of
    /// a herd block (`Rand(count)`), or −1 without one (no draw).
    pub(super) fn pick(
        &self,
        d: &PopulationDescription,
        index: i32,
        n: i32,
        rng: &mut Random,
    ) -> i32 {
        let bs = self.block();
        let (row0, col0) = (
            (index / self.herd_width) * bs,
            (index % self.herd_width) * bs,
        );
        let mut found = Vec::new();
        for r in 0..bs {
            for c in 0..bs {
                let i = self.width * (row0 + r) + col0 + c;
                if self.cells[i as usize][0] > 0 {
                    found.push(i);
                }
            }
        }
        if found.is_empty() {
            return -1;
        }
        let i = found[rng.rand_below(found.len() as u32) as usize];
        let cs = d.cell;
        n * (cs * (i / self.width) + (cs >> 1)) + cs * (i % self.width) + (cs >> 1)
    }

    /// The capacity of the population cell holding map cell `cell`.
    pub(super) fn capacity_at(&self, d: &PopulationDescription, n: i32, cell: i32) -> i32 {
        if cell < 0 {
            return 0;
        }
        let (y, x) = (cell / n / d.cell, cell % n / d.cell);
        self.cells
            .get((y * self.width + x) as usize)
            .map_or(0, |c| c[0])
    }

    /// `FUN_140054220`: every cell regrows up to its capacity, then each count becomes the
    /// weighted mean of the habitable cells within two (weight `16 − dx² − dy²`).
    pub(super) fn regrow(&mut self, d: &PopulationDescription) {
        for c in &mut self.cells {
            c[1] += d.regrowth;
            if c[0] < c[1] {
                c[1] = c[0];
            }
        }
        let w = self.width;
        let mut mean = vec![0; self.cells.len()];
        for gy in 0..w {
            for gx in 0..w {
                let (mut sum, mut weights) = (0, 0);
                for dy in -2..3 {
                    for dx in -2..3 {
                        let (x, y) = (gx + dx, gy + dy);
                        if x < 0 || y < 0 || x >= w || y >= w {
                            continue;
                        }
                        let c = self.cells[(w * y + x) as usize];
                        if c[0] == 0 {
                            continue;
                        }
                        let k = 16 - dx * dx - dy * dy;
                        if k >= 0 {
                            sum += c[1] * k;
                            weights += k;
                        }
                    }
                }
                mean[(w * gy + gx) as usize] = if weights == 0 { 0 } else { sum / weights };
            }
        }
        for (c, m) in self.cells.iter_mut().zip(mean) {
            c[1] = m;
            c[2] = m;
        }
    }
}
