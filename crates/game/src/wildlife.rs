//! Wild animals: PopulationComponent (vtable `0x1401604b0`), HerdComponent (`0x140169ab0`),
//! AnimalComponent (`0x140168530`) and WildAnimalComponent (`0x14016af20`). See
//! docs/WILDLIFE-POPULATION.md.
//!
//! A population divides the map into cells of `cell` map cells. Each cell holds a
//! capacity, set by its habitat, and a count that regrows and spreads every 300 s. The map is
//! also divided into herd blocks. Each block starts one herd of `min..max` animals on a random
//! habitable cell of the block. A herd rests for a while, then moves to another habitable
//! cell of its block, and regrows from time to time. Its animals wander within an area around
//! their home, idle, grow up and die of old age.

use std::collections::BTreeMap;

use banished_formats::game::{
    AnimalDescription, CharacterDescription, HerdDescription, PopulationDescription,
};
use banished_formats::types::AnimationGroup;

use crate::animation::Playback;
use crate::character::{Action, Character, MoveTo};
use crate::random::Random;
use crate::scheduler::{seconds, ticks, EntityScheduler};
use crate::seeding::{Cells, Placed, Template};
use crate::terrain::{CellType, HeightGrid};

/// AnimalComponent `+0x20` flag: a hunter has it (`FUN_14010c730`).
pub const HUNTED: u32 = 0x100000;
/// AnimalComponent `+0x20` flag: wild (`FUN_14010c8d0`).
pub const WILD: u32 = 0x400000;
/// The animal walk mask (`3999`).
const ANIMAL_WALK: u32 = 0xf9f;
/// `DAT_140178934`.
const PERIOD: f32 = 300.0;

/// A population template's description and herd template (index into the placed-object
/// templates).
#[derive(Debug, Clone)]
pub struct PopulationKind {
    pub desc: PopulationDescription,
    pub herd: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct HerdKind {
    pub desc: HerdDescription,
    pub animal: usize,
}

#[derive(Debug, Clone)]
pub struct AnimalKind {
    pub desc: AnimalDescription,
    /// The raw-material (pile) templates a hunted animal leaves.
    pub materials: Vec<usize>,
    pub character: CharacterDescription,
}

/// PopulationComponent.
#[derive(Debug, Clone, PartialEq)]
pub struct Population {
    pub entity: usize,
    pub template: usize,
    /// `+0x30`: cells per row.
    pub width: i32,
    /// `+0x38`: capacity, count and next count per cell.
    pub cells: Vec<[i32; 3]>,
    /// `+0x48`: herd blocks per row.
    pub herd_width: i32,
    /// `+0x60`: towards the next 300 s step.
    pub clock: i64,
    /// `+0x68`: the next cell to recount.
    pub cursor: i32,
}

/// HerdComponent.
#[derive(Debug, Clone, PartialEq)]
pub struct Herd {
    pub entity: usize,
    pub template: usize,
    /// `+0x28`, `+0x30`: the population and the herd's block.
    pub population: usize,
    pub index: i32,
    /// `+0x38`: the animals (entities), in joining order.
    pub animals: Vec<usize>,
    /// `+0x48`: moving to the target (else resting).
    pub moving: bool,
    /// `+0x4c`: the target map cell (−1 none).
    pub target: i32,
    /// `+0x50`: rest left; `+0x58`: towards the next regrowth.
    pub rest: i64,
    pub regrow: i64,
}

/// AnimalComponent and WildAnimalComponent, with the animal's CharacterComponent.
pub struct Animal {
    pub template: usize,
    pub character: Character,
    /// WildAnimalComponent `+0x28`: the herd (index into [`Wildlife::herds`]).
    pub herd: Option<usize>,
    /// Component `+0x20` flags ([`HUNTED`], [`WILD`]).
    pub flags: u32,
    /// `+0x28`: the home cell.
    pub home: i32,
    /// `+0x30`: until the next decision.
    pub timer: i64,
    /// `+0x38..+0x3e`: the area it keeps to (x, y, width, height).
    pub area: [i16; 4],
    /// `+0x40`: male (the second mesh, antlers).
    pub male: bool,
    /// `+0x44`, `+0x48`: age and lifespan (years).
    pub age: f32,
    pub life: f32,
    /// `+0x50`: livestock breeding clock (not used by wild animals).
    pub breed: i64,
    /// The model variant and mesh.
    pub variant: usize,
    pub mesh: usize,
    /// Model scale (`FUN_14010c7f0`).
    pub scale: f32,
    /// Queued for removal (`FUN_1400c8370`, entity flag `0x400`).
    pub dying: bool,
}

/// What a scheduled entity is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Population(usize),
    Herd(usize),
    Animal,
}

/// The wildlife of a town.
#[derive(Default)]
pub struct Wildlife {
    pub populations: Vec<Population>,
    pub herds: Vec<Herd>,
    /// Animals by entity.
    pub animals: BTreeMap<usize, Animal>,
    /// Templates by placed-object template index.
    pub population_kinds: BTreeMap<usize, PopulationKind>,
    pub herd_kinds: BTreeMap<usize, HerdKind>,
    pub animal_kinds: BTreeMap<usize, AnimalKind>,
    /// Animation groups by animal template and model variant.
    pub groups: BTreeMap<(usize, usize), std::sync::Arc<AnimationGroup>>,
    /// Entities queued for removal, in order.
    pub dying: Vec<usize>,
    kinds: BTreeMap<usize, Kind>,
}

/// The town state wildlife reads and writes.
pub struct Wild<'a, 't> {
    pub wildlife: &'a mut Wildlife,
    pub placed: &'a mut Vec<Placed>,
    pub templates: &'a [Template],
    pub cells: &'a Cells,
    pub terrain: &'a HeightGrid<'t>,
    pub rng: &'a mut Random,
    pub scheduler: &'a mut EntityScheduler<usize>,
}

impl Population {
    /// `FUN_1400544b0`: a cell's capacity: the description's when its block has habitat and
    /// nothing forbidden (cell types, or an occupant with `no_occupants`).
    fn capacity(&self, d: &PopulationDescription, cells: &Cells, i: i32) -> i32 {
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
    fn block(&self) -> i32 {
        self.width / self.herd_width.max(1)
    }

    /// `FUN_140054880`: the share of a herd block's cells that are habitable.
    fn habitable_share(&self, index: i32) -> f32 {
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
    fn pick(&self, d: &PopulationDescription, index: i32, n: i32, rng: &mut Random) -> i32 {
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
    fn capacity_at(&self, d: &PopulationDescription, n: i32, cell: i32) -> i32 {
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
    fn regrow(&mut self, d: &PopulationDescription) {
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

impl Wildlife {
    /// `FUN_1400c8370` on an animal (RemoveAnimal's start, old age, a herd without habitat):
    /// queued for removal; it leaves its herd (WildAnimalComponent's removal slot).
    pub fn kill(&mut self, a: usize) {
        let Some(animal) = self.animals.get_mut(&a) else {
            return;
        };
        if animal.dying {
            return;
        }
        animal.dying = true;
        if let Some(h) = animal.herd.take() {
            self.herds[h].animals.retain(|&x| x != a);
        }
        self.dying.push(a);
    }

    /// The animation group of an animal.
    pub fn group(&self, a: &Animal) -> Option<&AnimationGroup> {
        self.groups.get(&(a.template, a.variant)).map(|g| &**g)
    }
}

impl Wild<'_, '_> {
    fn n(&self) -> i32 {
        self.cells.size as i32
    }

    fn cell_of(&self, p: [f32; 3]) -> i32 {
        let n = self.n();
        let half = (n >> 1) as f32;
        let x = ((p[0] + half) as i32).clamp(0, n - 1);
        let y = ((p[1] + half) as i32).clamp(0, n - 1);
        n * y + x
    }

    fn ground(&self, cell: i32) -> [f32; 3] {
        crate::town::cell_ground(self.cells.size, self.terrain, cell)
    }

    /// `FUN_1400c8bf0` + `FUN_1400c9d70` for an entity without a map footprint: a placed
    /// entry registered in its template's bucket.
    fn create(&mut self, template: usize, position: [f32; 3]) -> usize {
        let id = self.placed.len();
        let cell = self.cell_of(position);
        let n = self.n();
        self.placed.push(Placed {
            template,
            bucket: self.templates[template].bucket,
            cell: (cell % n, cell / n),
            footprint: (1, 1),
            position,
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
        let handle = self.scheduler.register(id, self.templates[template].bucket);
        debug_assert_eq!(handle, id);
        id
    }

    /// `FUN_1400a5eb0` for one MapGenerator population: the entity, then its setup
    /// (`FUN_140054b80`) with its herds.
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

    /// The herd's area around its target (`FUN_140126fa0` / `FUN_140127810`): `area` wide,
    /// kept a cell off the map's edge (the height test adds the possibly shortened width, as
    /// the original does).
    fn herd_area(&self, h: usize) -> [i16; 4] {
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

    /// `FUN_140127810`: a new target in the herd's block; every animal keeps to the area
    /// around it. Without habitat the herd's animals are removed.
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

    fn kill(&mut self, a: usize) {
        self.wildlife.kill(a);
    }

    /// `FUN_140126420`: a new target, then `count` animals at random cells of its area.
    fn spawn_herd(&mut self, h: usize, count: i32) {
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
        let male = self.rng.rand_below(2) == 0;
        let n = self.n();
        for _ in 0..count {
            let x = x0 + self.rng.rand_below(w.max(0) as u32) as i32;
            let y = y0 + self.rng.rand_below(hh.max(0) as u32) as i32;
            let base = self.ground(n * y + x);
            let dy = self.rng.rand_float_tol();
            let dx = self.rng.rand_float_tol();
            let position = [base[0] + dx * 0.4, base[1] + dy * 0.4, base[2]];
            let yaw = self.rng.rand_float_tol() * 180.0;
            let Some(a) = self.new_animal(hk.animal, position, yaw, male) else {
                return;
            };
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
    fn regrow_herd(&mut self, h: usize, target: i32) {
        let template = self.wildlife.herds[h].template;
        let hk = self.wildlife.herd_kinds[&template].clone();
        let male = self.rng.rand_below(2) == 0;
        while (self.wildlife.herds[h].animals.len() as i32) < target {
            let area = hk.desc.area as f32;
            let base = self.ground(self.wildlife.herds[h].target);
            let dy = self.rng.rand_float_tol();
            let dx = self.rng.rand_float_tol();
            let position = [base[0] + dx * area, base[1] + dy * area, base[2]];
            let yaw = self.rng.rand_float_tol() * 180.0;
            let Some(a) = self.new_animal(hk.animal, position, yaw, male) else {
                return;
            };
            self.join(a, h);
            self.register_animal(a);
        }
    }

    /// `FUN_1400c8bf0` of an animal template and `FUN_14010c8d0`: placed with its model and
    /// character, wild; a male takes the variant's second mesh.
    fn new_animal(
        &mut self,
        template: usize,
        position: [f32; 3],
        yaw: f32,
        male: bool,
    ) -> Option<usize> {
        let kind = self.wildlife.animal_kinds.get(&template)?.clone();
        let t = &self.templates[template];
        let (mut variant, mut mesh) = (0usize, 0usize);
        if let Some(m) = &t.model {
            variant = m.variant.max(0) as usize;
            if m.random_variant {
                variant = self.rng.rand_below(m.variants.len() as u32) as usize;
            }
            mesh = m.mesh.max(0) as usize;
            if male && m.variants.get(variant).is_some_and(|v| v.meshes.len() > 1) {
                mesh = 1;
            }
        }
        let id = self.placed.len();
        let mut character = Character::new(kind.character, position, 1.0);
        character.yaw = yaw;
        self.wildlife.animals.insert(
            id,
            Animal {
                template,
                character,
                herd: None,
                flags: WILD,
                home: -1,
                timer: 0,
                area: [0; 4],
                male,
                age: 0.0,
                life: 0.0,
                breed: 0,
                variant,
                mesh,
                scale: 1.0,
                dying: false,
            },
        );
        self.rescale(id);
        Some(id)
    }

    /// `FUN_1401339e0`: the animal joins the herd and keeps to its area.
    fn join(&mut self, a: usize, h: usize) {
        let area = self.herd_area(h);
        self.wildlife.herds[h].animals.push(a);
        if let Some(animal) = self.wildlife.animals.get_mut(&a) {
            animal.herd = Some(h);
            animal.area = area;
        }
    }

    /// `FUN_1400c9d70` for an animal: its entity, the setup (`FUN_14010c220`, the lifespan),
    /// the activation (`FUN_14010c0f0`: home where it stands, its area around it, then the
    /// scheduled update with no time).
    fn register_animal(&mut self, a: usize) {
        let template = self.wildlife.animals[&a].template;
        let position = self.wildlife.animals[&a].character.position;
        let e = self.create(template, position);
        debug_assert_eq!(e, a);
        self.wildlife.kinds.insert(a, Kind::Animal);
        let d = self.wildlife.animal_kinds[&template].desc.clone();
        let life = self.rng.rand_float_tol() * d.life_range + d.life;
        let n = self.n();
        let cell = self.cell_of(position);
        let animal = self.wildlife.animals.get_mut(&a).unwrap();
        animal.life = life;
        if animal.flags & WILD != 0 {
            animal.home = cell;
            animal.area = area_around(n, cell, d.area);
        }
        self.update_animal(a, 0);
    }

    /// `FUN_14010c7f0`: the model grows from a third to full size by the adult age.
    fn rescale(&mut self, a: usize) {
        let Some(animal) = self.wildlife.animals.get_mut(&a) else {
            return;
        };
        let d = &self.wildlife.animal_kinds[&animal.template].desc;
        let s = (animal.age / d.adult * 0.67 + 0.33).min(1.0);
        animal.scale = s;
        animal.character.scale = s;
    }

    /// A scheduled visit of entity `e` with `elapsed` ticks.
    pub fn scheduled(&mut self, e: usize, elapsed: i64) {
        match self.wildlife.kinds.get(&e).copied() {
            Some(Kind::Population(p)) => self.update_population(p, elapsed),
            Some(Kind::Herd(h)) => self.update_herd(h, elapsed),
            Some(Kind::Animal) => self.update_animal(e, elapsed),
            None => {}
        }
    }

    /// `FUN_1400549d0`.
    fn update_population(&mut self, p: usize, dt: i64) {
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

    /// `FUN_1401270c0`.
    fn update_herd(&mut self, h: usize, dt: i64) {
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
    fn all_home(&self, h: usize) -> bool {
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

    /// `FUN_14010b8f0`: AnimalComponent's scheduled update.
    pub fn update_animal(&mut self, a: usize, dt: i64) {
        let Some(animal) = self.wildlife.animals.get_mut(&a) else {
            return;
        };
        if animal.dying {
            return;
        }
        let d = self.wildlife.animal_kinds[&animal.template].desc.clone();
        animal.age += seconds(dt) * (1.0 / 3600.0);
        if animal.life < animal.age {
            self.kill(a);
            return;
        }
        // Not ported: livestock breeding (`+0x40` with flag `0x800000` and without
        // `0x200000`); wild animals never have it.
        self.rescale(a);
        let n = self.n();
        let animal = self.wildlife.animals.get_mut(&a).unwrap();
        if animal.character.stack.is_active() {
            return;
        }
        animal.timer -= dt;
        if 0 < animal.timer {
            return;
        }
        let mut anim = 1;
        if animal.flags & HUNTED == 0 {
            if animal.flags & WILD != 0 {
                let home = animal.home;
                // A home taken by an object (other than a trading post's, not ported) or no
                // longer ground: a new home and area.
                let taken = home >= 0 && self.cells.objects[home as usize].is_some();
                if home < 0 || self.cells.types[home as usize] != CellType(2) || taken {
                    self.rehome(a, &d);
                }
            }
            let animal = &self.wildlife.animals[&a];
            let here = self.cell_of(animal.character.position);
            let area = animal.area;
            let mut wander = !inside(&area, here, n);
            if !wander {
                anim = self.rng.rand_below(4) as usize;
                wander = anim == 0;
            }
            if wander {
                let mut in_area = 0;
                let mut total = 0;
                let mut dest;
                loop {
                    let area = self.wildlife.animals[&a].area;
                    let ry = self.rng.rand_below(area[3] as u32) as i32;
                    let rx = self.rng.rand_below(area[2] as u32) as i32;
                    let cell = rx + area[0] as i32 + (ry + area[1] as i32) * n;
                    let base = self.ground(cell);
                    let first = self.rng.rand_float_tol();
                    let second = self.rng.rand_float_tol();
                    dest = [base[0] + second * 0.4, base[1] + first * 0.4, base[2]];
                    if 10 < in_area {
                        self.rehome(a, &d);
                        in_area = 0;
                    }
                    in_area += 1;
                    total += 1;
                    let at = self.cell_of(dest);
                    if self.cells.types[at as usize] == CellType(2) || 100 <= total {
                        break;
                    }
                }
                if self.cells.types[self.cell_of(dest) as usize] == CellType(2) {
                    let walk = MoveTo::new(dest, 0, 1, ANIMAL_WALK, 1, -1.0, self.rng);
                    self.restack(a, Some(Action::move_to(walk)));
                    self.wildlife.animals.get_mut(&a).unwrap().timer = 0;
                    return;
                }
                anim = self.rng.rand_below(3) as usize + 1;
            }
        }
        let idle = self.rng.rand_float() * d.idle + d.idle;
        self.idle(a, anim, idle, d.turn);
    }

    /// `FUN_1400ca8d0` on the animal's stack, then (with `action`) `FUN_1400ca9b0`.
    fn restack(&mut self, a: usize, action: Option<Action>) {
        let Some(animal) = self.wildlife.animals.get(&a) else {
            return;
        };
        let Some(group) = self
            .wildlife
            .groups
            .get(&(animal.template, animal.variant))
            .cloned()
        else {
            return;
        };
        let ch = &mut self.wildlife.animals.get_mut(&a).unwrap().character;
        let mut stack = std::mem::take(&mut ch.stack);
        stack.clear(ch, &group, &mut []);
        if let Some(action) = action {
            stack.push(ch, &group, action);
        }
        ch.stack = stack;
    }

    /// `FUN_14010c730`: a hunter has the animal: it stops what it does and stands.
    pub fn hunted(&mut self, a: usize) {
        let Some(animal) = self.wildlife.animals.get_mut(&a) else {
            return;
        };
        animal.flags |= HUNTED;
        if animal.character.stack.is_active() {
            self.restack(a, None);
            self.update_animal(a, 0);
        }
    }

    /// `FUN_14010b720`: play clip `anim` for `seconds`; with `turn`, face a random way.
    fn idle(&mut self, a: usize, anim: usize, secs: f32, turn: bool) {
        let looping = {
            let animal = &self.wildlife.animals[&a];
            self.wildlife
                .group(animal)
                .and_then(|g| g.clips.get(anim))
                .is_some_and(|c| c.looping)
        };
        let animal = self.wildlife.animals.get_mut(&a).unwrap();
        if !(animal.character.animation.clip == anim && looping) {
            animal.character.animation = Playback::new(anim, 0);
        }
        animal.timer = ticks(secs);
        if turn {
            let y = self.rng.rand_float_tol();
            let x = self.rng.rand_float_tol();
            let len = (x * x + y * y).sqrt().max(f32::EPSILON);
            let ch = &mut self.wildlife.animals.get_mut(&a).unwrap().character;
            ch.flag_28 = false;
            ch.direction = [x / len, y / len, 0.0];
            ch.target_heading = (y / len).atan2(x / len).to_degrees();
        }
    }

    /// `FUN_14010b4f0` then `FUN_14010b2f0`: the nearest free ground in growing squares around
    /// the home becomes the home, and the area follows.
    fn rehome(&mut self, a: usize, d: &AnimalDescription) {
        let n = self.n();
        let home = self.wildlife.animals[&a].home.max(0);
        let new = nearest_ground(self.cells, n, home);
        let animal = self.wildlife.animals.get_mut(&a).unwrap();
        animal.home = new;
        animal.area = area_around(n, new, d.area);
    }

    /// The entity manager's removal of the queued animals (their components' removal slots).
    pub fn bury(&mut self) -> bool {
        let dying = std::mem::take(&mut self.wildlife.dying);
        let changed = !dying.is_empty();
        for a in dying {
            if let Some(animal) = self.wildlife.animals.remove(&a) {
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

/// `FUN_14010bfe0`: `cell` within `area`.
fn inside(area: &[i16; 4], cell: i32, n: i32) -> bool {
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
fn area_around(n: i32, cell: i32, size: i32) -> [i16; 4] {
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
fn nearest_ground(cells: &Cells, n: i32, home: i32) -> i32 {
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
