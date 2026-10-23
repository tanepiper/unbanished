//! Livestock: roaming until a pasture takes them, penning, breeding and births, trade, release and burial.

use super::*;

impl Wild<'_, '_> {
    /// LivestockComponent's scheduled update (`FUN_140129020`): in a pasture, an animal with a
    /// product adds one to the pasture's count every `interval` × 300 s.
    pub(super) fn update_livestock(&mut self, a: usize, dt: i64) {
        let Some(animal) = self.wildlife.animals.get(&a) else {
            return;
        };
        let Some(Livestock {
            pasture: Some(p),
            timer,
        }) = animal.livestock
        else {
            return;
        };
        let Some(kind) = self.wildlife.livestock_kinds.get(&animal.template) else {
            return;
        };
        if kind.product.is_none() {
            return;
        }
        let period = ticks(kind.interval * PERIOD);
        let mut timer = timer + dt;
        let mut products = 0;
        // Deviation: a zero interval would never leave the loop in the original.
        while 0 < period && period < timer {
            products += 1;
            timer -= period;
        }
        if let Some(l) = self
            .wildlife
            .animals
            .get_mut(&a)
            .unwrap()
            .livestock
            .as_mut()
        {
            l.timer = timer;
        }
        if let Some(pasture) = self.wildlife.pastures.get_mut(&p) {
            pasture.produced += products;
        }
    }

    /// The animal joins pasture `to` (or the town's list) and leaves its old one; leaving a pasture
    /// it turns wild, joining one it keeps to its area.
    ///
    /// Original: `FUN_140129190`.
    pub fn assign(&mut self, a: usize, to: Option<usize>) {
        let Some(l) = self.wildlife.animals.get(&a).and_then(|x| x.livestock) else {
            return;
        };
        if l.pasture == to {
            return;
        }
        match l.pasture {
            None => self.wildlife.unassigned.retain(|&x| x != a),
            Some(p) => {
                if let Some(p) = self.wildlife.pastures.get_mut(&p) {
                    p.livestock.retain(|&x| x != a);
                }
                self.turn_wild(a);
            }
        }
        if let Some(l) = self
            .wildlife
            .animals
            .get_mut(&a)
            .and_then(|x| x.livestock.as_mut())
        {
            l.pasture = to;
        }
        match to {
            Some(p) => {
                if let Some(pasture) = self.wildlife.pastures.get_mut(&p) {
                    pasture.livestock.push(a);
                }
                self.pen(a);
            }
            None => self.wildlife.unassigned.push(a),
        }
    }

    /// `FUN_14010b3a0`: wild again, at home where it stands with an area around it, then the
    /// scheduled update with no time.
    pub(super) fn turn_wild(&mut self, a: usize) {
        let n = self.n();
        let Some(animal) = self.wildlife.animals.get(&a) else {
            return;
        };
        let cell = self.cell_of(animal.character.position);
        let size = self.wildlife.animal_kinds[&animal.template].desc.area;
        let animal = self.wildlife.animals.get_mut(&a).unwrap();
        animal.flags |= WILD;
        animal.home = cell;
        animal.area = area_around(n, cell, size);
        self.update_animal(a, 0);
    }

    /// `FUN_140129570`: no longer wild, the animal keeps to its pasture's area.
    pub(super) fn pen(&mut self, a: usize) {
        let Some(p) = self
            .wildlife
            .animals
            .get(&a)
            .and_then(|x| x.livestock)
            .and_then(|l| l.pasture)
        else {
            return;
        };
        let Some(area) = self.wildlife.pastures.get(&p).map(|x| x.area) else {
            return;
        };
        let animal = self.wildlife.animals.get_mut(&a).unwrap();
        animal.flags &= !WILD;
        animal.area = area;
    }

    /// A pasture with room takes the waiting livestock of its kind, in the town list's order, until
    /// full.
    ///
    /// Original: `FUN_14012c610`.
    pub fn fill(&mut self, p: usize) {
        if self.wildlife.unassigned.is_empty() {
            return;
        }
        let cap = self.wildlife.capacity(p);
        let Some(pasture) = self.wildlife.pastures.get(&p) else {
            return;
        };
        if cap <= pasture.livestock.len() as i32 {
            return;
        }
        let kind = pasture.kind;
        let waiting: Vec<usize> = self
            .wildlife
            .unassigned
            .iter()
            .copied()
            .filter(|a| Some(self.wildlife.animals[a].template) == kind)
            .collect();
        for a in waiting {
            self.assign(a, Some(p));
            if cap <= self.wildlife.pastures[&p].livestock.len() as i32 {
                break;
            }
        }
    }

    /// Each animal with young due has them, while the pasture holds fewer than three over its room
    /// (the animals born join the end of the list and are seen too). Born animals are registered by
    /// the caller.
    ///
    /// Original: `FUN_14012c840`.
    pub fn births(&mut self, p: usize) -> Vec<usize> {
        let cap = self.wildlife.capacity(p);
        let mut born = Vec::new();
        let mut i = 0;
        while i < self
            .wildlife
            .pastures
            .get(&p)
            .map_or(0, |x| x.livestock.len())
        {
            let a = self.wildlife.pastures[&p].livestock[i];
            let animal = self.wildlife.animals.get_mut(&a).unwrap();
            if animal.flags & BIRTH != 0 {
                animal.flags &= !BIRTH;
                if (self.wildlife.pastures[&p].livestock.len() as i32) < cap + 3 {
                    born.extend(self.give_birth(p, Some(a), false));
                }
            }
            i += 1;
        }
        born
    }

    /// A young animal of the pasture's kind next to its parent (or anywhere in the pasture, any age
    /// under a year, without one); male more likely the more females there are. With `buckets` it
    /// joins the entity manager's buckets here, else the caller's.
    ///
    /// Original: `FUN_14012db40`.
    pub fn give_birth(&mut self, p: usize, parent: Option<usize>, buckets: bool) -> Option<usize> {
        let pasture = self.wildlife.pastures.get(&p)?;
        let kind = pasture.kind?;
        let area = pasture.area;
        let variant = self.pick_variant(kind)?;
        let position = match parent.and_then(|a| self.wildlife.animals.get(&a)) {
            Some(parent) => {
                let q = parent.character.position;
                let dy = self.rng.rand_float_tol();
                let dx = self.rng.rand_float_tol();
                [q[0] + dx, q[1] + dy, q[2]]
            }
            None => {
                let n = self.n();
                let rx = self.rng.rand_below(area[2] as u32) as i32;
                let ry = self.rng.rand_below(area[3] as u32) as i32;
                let dy = self.rng.rand_float_tol();
                let dx = self.rng.rand_float_tol();
                let cell = n * (area[1] as i32 + ry) + area[0] as i32 + rx;
                let g = self.ground(cell);
                [g[0] + dx, g[1] + dy, g[2]]
            }
        };
        let herd = &self.wildlife.pastures[&p].livestock;
        let females = herd
            .iter()
            .filter(|a| self.wildlife.animals.get(a).is_some_and(|x| !x.male))
            .count();
        let male = if herd.is_empty() {
            true
        } else {
            let share = females as f32 / herd.len() as f32;
            share > self.rng.rand_float()
        };
        let yaw = self.rng.rand_float_tol() * 180.0;
        let a = self.insert_animal(kind, variant, position, yaw, male)?;
        self.assign(a, Some(p));
        let age = if parent.is_none() {
            self.rng.rand_float()
        } else {
            0.0
        };
        self.wildlife.animals.get_mut(&a).unwrap().age = age;
        self.rescale(a);
        self.register_animal_in(a, buckets);
        Some(a)
    }

    /// The starting livestock of a kind, `count` animals around `at` (`spread` either way), males
    /// and females in turn from a random first; any age under a year. They wait, wild, for a
    /// pasture.
    ///
    /// Original: `FUN_1400a5fe0`.
    pub fn start_livestock(&mut self, template: usize, count: i32, at: [f32; 3], spread: f32) {
        let mut male = self.rng.rand_below(2) == 0;
        for _ in 0..count {
            let Some(variant) = self.pick_variant(template) else {
                return;
            };
            let dy = self.rng.rand_float_tol();
            let dx = self.rng.rand_float_tol();
            let (x, y) = (at[0] + dx * spread, at[1] + dy * spread);
            // `at`'s height, as the original passes it: the CharacterComponent's first update
            // stands it on the ground.
            let position = [x, y, at[2]];
            let yaw = self.rng.rand_float_tol() * 180.0;
            let Some(a) = self.insert_animal(template, variant, position, yaw, male) else {
                return;
            };
            male = !male;
            self.wildlife.animals.get_mut(&a).unwrap().age = self.rng.rand_float();
            self.rescale(a);
            self.register_animal(a);
        }
    }

    // +0x48
    /// Livestock bought at a trading post, `count` animals at random between its first two create
    /// points (`a`, `b`), males and females in turn from a random first, turned at random; not
    /// wild, they keep to the cells between the points (`area`) until a pasture takes them. Each is
    /// `RandFloat` × half its lifespan old (the lifespan is drawn when the entity is made). The
    /// owner registers the entities.
    ///
    /// Original: `FUN_14005b640`.
    pub fn trade_livestock(
        &mut self,
        template: usize,
        count: i32,
        a: [f32; 3],
        b: [f32; 3],
        area: [i16; 4],
    ) {
        let mut male = self.rng.rand_below(2) == 0;
        for _ in 0..count {
            let Some(variant) = self.pick_variant(template) else {
                return;
            };
            let fy = self.rng.rand_float();
            let fx = self.rng.rand_float();
            let (x, y) = (a[0] + (b[0] - a[0]) * fx, a[1] + (b[1] - a[1]) * fy);
            // `a`'s height, as the original passes it: the CharacterComponent's first update
            // stands it on the ground.
            let position = [x, y, a[2]];
            let yaw = self.rng.rand_float_tol() * crate::nature::seeding::YAW_RANGE;
            let Some(id) = self.insert_animal(template, variant, position, yaw, male) else {
                return;
            };
            male = !male;
            let animal = self.wildlife.animals.get_mut(&id).unwrap();
            animal.flags &= !WILD;
            animal.area = area;
            let life = animal.life;
            let f = self.rng.rand_float();
            self.wildlife.animals.get_mut(&id).unwrap().age = f * life * 0.5;
            self.rescale(id);
            self.register_animal_in(id, false);
        }
    }

    /// The pasture's message 0 (a rancher's visit): a random animal of it may breed.
    ///
    /// Original: `FUN_14012cbf0`.
    pub fn mark_breeder(&mut self, p: usize) {
        let Some(herd) = self.wildlife.pastures.get(&p).map(|x| x.livestock.clone()) else {
            return;
        };
        if herd.is_empty() {
            return;
        }
        let r = self.rng.rand_below(herd.len() as u32) as usize;
        if let Some(animal) = self.wildlife.animals.get_mut(&herd[r]) {
            animal.flags |= BREEDER;
        }
    }

    /// A pasture is removed: every animal goes back to the town's list, last first, and the
    /// pasture leaves the town's list.
    ///
    /// Original: `FUN_14012c310` (state 3) and `FUN_14012ccb0` (removal).
    pub fn release(&mut self, p: usize) {
        while let Some(a) = self
            .wildlife
            .pastures
            .get(&p)
            .and_then(|x| x.livestock.last().copied())
        {
            self.assign(a, None);
        }
        self.wildlife.pasture_list.retain(|&x| x != p);
    }

    /// The entity manager's removal of the queued animals (their components' removal slots).
    pub fn bury(&mut self) -> bool {
        let dying = std::mem::take(&mut self.wildlife.dying);
        let changed = !dying.is_empty();
        for a in dying {
            if let Some(animal) = self.wildlife.animals.remove(&a) {
                // CharacterComponent's end (`FUN_14010ee50`): out of the tree.
                self.wildlife.tree.remove(animal.character.tree_slot);
                if let Some(h) = animal.herd {
                    self.wildlife.herds[h].animals.retain(|&x| x != a);
                }
            }
            self.wildlife.kinds.remove(&a);
            if let Some(p) = self.placed.get_mut(a) {
                p.alive = false;
            }
            self.scheduler.remove(a);
        }
        changed
    }
}
