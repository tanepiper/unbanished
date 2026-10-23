//! Herds: their area within a block, spawning and regrowing their animals, and moving home.

use super::*;

/// `FUN_14010bfe0`: `cell` within `area`.
pub(super) fn inside(area: &[i16; 4], cell: i32, n: i32) -> bool {
    let (x, y) = (cell % n, cell / n);
    let (ax, ay, aw, ah) = (
        area[0] as i32,
        area[1] as i32,
        area[2] as i32,
        area[3] as i32,
    );
    !(x < ax || y < ay || ax + aw <= x || ay + ah <= y)
}

/// `FUN_14010b2f0`: a `size` square around `cell`, clamped to the map.
pub(super) fn area_around(n: i32, cell: i32, size: i32) -> [i16; 4] {
    let half = size / 2;
    let x = (cell % n - half).max(0);
    let y = (cell / n - half).max(0);
    let mut w = size;
    let mut h = size;
    if n <= w + x {
        w = n - x - 1;
    }
    if n <= h + y {
        h = n - y - 1;
    }
    [x as i16, y as i16, w as i16, h as i16]
}

/// `FUN_14010b4f0`: the first ground cell without an occupant on the border of squares of
/// side 5, 7, … 31 around `home` (top and bottom rows by column, then the sides by row), else
/// `home`.
pub(super) fn nearest_ground(cells: &Cells, n: i32, home: i32) -> i32 {
    let free = |c: i32| {
        c >= 0
            && (c as usize) < cells.types.len()
            && cells.types[c as usize] == CellType(2)
            && cells.occupants[c as usize].is_none()
    };
    let mut s = 3;
    loop {
        s += 2;
        let half = s / 2;
        let x0 = (home % n - half).max(0);
        let y0 = (home / n - half).max(0);
        let mut w = s;
        if n <= s + x0 {
            w = n - x0 - 1;
        }
        let mut h = s;
        if n <= s + y0 {
            h = n - y0 - 1;
        }
        let bottom = y0 - 1 + h;
        let right = w - 1 + x0;
        for x in x0..=right {
            if free(y0 * n + x) {
                return y0 * n + x;
            }
            if free(n * bottom + x) {
                return n * bottom + x;
            }
        }
        let mut y = y0;
        loop {
            y += 1;
            if y > bottom - 1 {
                break;
            }
            if free(n * y + x0) {
                return x0 + n * y;
            }
            if free(n * y + right) {
                return y * n + right;
            }
        }
        if 30 < s {
            return home;
        }
    }
}

impl Wild<'_, '_> {
    /// The herd's area around its target (`FUN_140126fa0` / `FUN_140127810`): `area` wide,
    /// kept a cell off the map's edge (the height test adds the possibly shortened width, as
    /// the original does).
    pub(super) fn herd_area(&self, h: usize) -> [i16; 4] {
        let herd = &self.wildlife.herds[h];
        let size = self.wildlife.herd_kinds[&herd.template].desc.area;
        let n = self.n();
        let x0 = (herd.target % n - (size >> 1)).max(1);
        let y0 = (herd.target / n - (size >> 1)).max(1);
        let mut w = size;
        if n - 1 <= x0 + size {
            w = n - x0 - 2;
        }
        let mut h = size;
        if n - 1 <= y0 + w {
            h = n - y0 - 2;
        }
        [x0 as i16, y0 as i16, w as i16, h as i16]
    }

    /// A new target in the herd's block; every animal keeps to the area around it. Without habitat
    /// the herd's animals are removed.
    ///
    /// Original: `FUN_140127810`.
    pub fn retarget(&mut self, h: usize) {
        let (p, index, template) = {
            let herd = &self.wildlife.herds[h];
            (herd.population, herd.index, herd.template)
        };
        let _ = template;
        let pt = self.wildlife.populations[p].template;
        let d = self.wildlife.population_kinds[&pt].desc.clone();
        let n = self.n();
        let target = self.wildlife.populations[p].pick(&d, index, n, self.rng);
        self.wildlife.herds[h].target = target;
        if target < 0 {
            for a in std::mem::take(&mut self.wildlife.herds[h].animals) {
                self.kill(a);
            }
            return;
        }
        let area = self.herd_area(h);
        for a in self.wildlife.herds[h].animals.clone() {
            if let Some(animal) = self.wildlife.animals.get_mut(&a) {
                animal.area = area;
            }
        }
    }

    /// `FUN_140126420`: a new target, then `count` animals at random cells of its area.
    pub(super) fn spawn_herd(&mut self, h: usize, count: i32) {
        self.retarget(h);
        let template = self.wildlife.herds[h].template;
        let hk = self.wildlife.herd_kinds[&template].clone();
        let area = self.herd_area(h);
        let (x0, y0, w, hh) = (
            area[0] as i32,
            area[1] as i32,
            area[2] as i32,
            area[3] as i32,
        );
        // The first animal's sex; the rest alternate (`sete` after each, `0x1401267d9`).
        let mut male = self.rng.rand_below(2) == 0;
        let n = self.n();
        for _ in 0..count {
            let x = x0 + self.rng.rand_below(w.max(0) as u32) as i32;
            let y = y0 + self.rng.rand_below(hh.max(0) as u32) as i32;
            // The entity is made here (`FUN_1400c8bf0`): its model's setup draws the variant
            // before the offsets and the heading.
            let Some(variant) = self.pick_variant(hk.animal) else {
                return;
            };
            let base = self.ground(n * y + x);
            let dy = self.rng.rand_float_tol();
            let dx = self.rng.rand_float_tol();
            let position = [base[0] + dx * 0.4, base[1] + dy * 0.4, base[2]];
            let yaw = self.rng.rand_float_tol() * 180.0;
            let Some(a) = self.insert_animal(hk.animal, variant, position, yaw, male) else {
                return;
            };
            male = !male;
            let life = self.wildlife.animal_kinds[&hk.animal].desc.life;
            let age = self.rng.rand_float() * life;
            if let Some(animal) = self.wildlife.animals.get_mut(&a) {
                animal.age = age;
            }
            self.rescale(a);
            self.join(a, h);
            self.register_animal(a);
        }
        let rest = self.rng.rand_float() * (hk.desc.rest_range + hk.desc.rest) * PERIOD;
        let herd = &mut self.wildlife.herds[h];
        herd.rest = ticks(rest);
        herd.moving = false;
    }

    /// `FUN_140126920`: animals join at the target until the herd has `target`.
    pub(super) fn regrow_herd(&mut self, h: usize, target: i32) {
        let template = self.wildlife.herds[h].template;
        let hk = self.wildlife.herd_kinds[&template].clone();
        // The first animal's sex; the rest alternate (`sete` after each, `0x140126cb0`).
        let mut male = self.rng.rand_below(2) == 0;
        while (self.wildlife.herds[h].animals.len() as i32) < target {
            // The entity is made first (`FUN_1400c8bf0`), so its variant's draw comes before the
            // offsets and the heading.
            let Some(variant) = self.pick_variant(hk.animal) else {
                return;
            };
            let area = hk.desc.area as f32;
            let base = self.ground(self.wildlife.herds[h].target);
            let dy = self.rng.rand_float_tol();
            let dx = self.rng.rand_float_tol();
            let position = [base[0] + dx * area, base[1] + dy * area, base[2]];
            let yaw = self.rng.rand_float_tol() * 180.0;
            let Some(a) = self.insert_animal(hk.animal, variant, position, yaw, male) else {
                return;
            };
            male = !male;
            self.join(a, h);
            self.register_animal(a);
        }
    }

    /// `FUN_1401270c0`.
    pub(super) fn update_herd(&mut self, h: usize, dt: i64) {
        let template = self.wildlife.herds[h].template;
        let hd = self.wildlife.herd_kinds[&template].desc.clone();
        let p = self.wildlife.herds[h].population;
        let pt = self.wildlife.populations[p].template;
        let pd = self.wildlife.population_kinds[&pt].desc.clone();
        let n = self.n();
        let herd = &mut self.wildlife.herds[h];
        herd.regrow += dt;
        let period = ticks(hd.regrowth * PERIOD);
        if period < herd.regrow {
            herd.regrow -= period;
            let share = self.wildlife.populations[p].habitable_share(herd.index);
            let f = if share < 0.5 { share * 2.0 } else { 1.0 };
            let count = self.wildlife.herds[h].animals.len() as i32;
            let mut target = if count * 2 == 0 { 2 } else { count * 2 };
            let by_share = (hd.max as f32 * f + 0.5) as i32;
            if by_share < target {
                target = by_share;
            }
            self.regrow_herd(h, target);
        }
        let herd = &self.wildlife.herds[h];
        let habitable = self.wildlife.populations[p].capacity_at(&pd, n, herd.target) != 0;
        if !herd.moving {
            let rest = herd.rest - dt;
            self.wildlife.herds[h].rest = rest;
            if 0 < rest && habitable {
                return;
            }
            self.retarget(h);
            let herd = &mut self.wildlife.herds[h];
            herd.rest = 0;
            herd.moving = true;
        } else {
            if self.all_home(h) {
                let rest = (self.rng.rand_float() * hd.rest_range + hd.rest) * PERIOD;
                let herd = &mut self.wildlife.herds[h];
                herd.moving = false;
                herd.rest = ticks(rest);
                return;
            }
            if !habitable {
                self.retarget(h);
            }
        }
    }

    /// `FUN_140126e50`: every animal stands in its area (`FUN_14010bfe0`).
    pub(super) fn all_home(&self, h: usize) -> bool {
        self.wildlife.herds[h].animals.iter().all(|a| {
            self.wildlife.animals.get(a).is_some_and(|animal| {
                inside(
                    &animal.area,
                    self.cell_of(animal.character.position),
                    self.n(),
                )
            })
        })
    }

    /// `FUN_1401339e0`: the animal joins the herd and keeps to its area.
    pub(super) fn join(&mut self, a: usize, h: usize) {
        let area = self.herd_area(h);
        self.wildlife.herds[h].animals.push(a);
        if let Some(animal) = self.wildlife.animals.get_mut(&a) {
            animal.herd = Some(h);
            animal.area = area;
        }
    }
}
