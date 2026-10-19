//! Wild animals: PopulationComponent, HerdComponent, AnimalComponent and WildAnimalComponent. See
//! docs/WILDLIFE-POPULATION.md.
//!
//! A population divides the map into cells of `cell` map cells. Each cell holds a capacity, set by
//! its habitat, and a count that regrows and spreads every 300 s. The map is also divided into herd
//! blocks. Each block starts one herd of `min..max` animals on a random habitable cell of the
//! block. A herd rests for a while, then moves to another habitable cell of its block, and regrows
//! from time to time. Its animals wander within an area around their home, idle, grow up and die of
//! old age.
//!
//! Livestock (LivestockComponent) are animals too. Until a pasture of their kind takes them they
//! roam like wild ones and wait in the town's list; in a pasture they keep to its area, a breeding
//! male has young, and products gather. See docs/FARMING.md "Pastures".
//!
//! Original: `0x140169ab0`, `0x140168530`, `0x14016af20`.
// vtable 0x1401604b0; vtable 0x140169fb0; +0x80

use std::collections::BTreeMap;

use banished_formats::game::{
    AnimalDescription, CharacterDescription, HerdDescription, PopulationDescription,
};
use banished_formats::types::AnimationGroup;

use crate::behaviour::animation::AnimationStack;
use crate::behaviour::character::{Action, Character, MoveTo};
use crate::nature::seeding::{Cells, Placed, Template};
use crate::random::Random;
use crate::scheduler::{seconds, ticks, EntityScheduler};
use crate::terrain::{CellType, HeightGrid};

// +0x20
/// AnimalComponent flag: a hunter has it.
///
/// Original: `FUN_14010c730`.
pub const HUNTED: u32 = 0x100000;
// +0x20
/// AnimalComponent flag: wild.
///
/// Original: `FUN_14010c8d0`.
pub const WILD: u32 = 0x400000;
// +0x20
/// AnimalComponent flag: young are due; the pasture's update has them born.
///
/// Original: `FUN_14010b8f0`, `FUN_14012c840`.
pub const BIRTH: u32 = 0x200000;
// +0x20
/// AnimalComponent flag: may breed (a rancher's visit marks a random animal of the pasture).
///
/// Original: `FUN_14012cbf0`.
pub const BREEDER: u32 = 0x800000;
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

/// The settings of a kind of herd.
#[derive(Debug, Clone)]
pub struct HerdKind {
    pub desc: HerdDescription,
    pub animal: usize,
}

/// The settings of a kind of animal.
#[derive(Debug, Clone)]
pub struct AnimalKind {
    pub desc: AnimalDescription,
    /// The raw-material (pile) templates a hunted animal leaves.
    pub materials: Vec<usize>,
    pub character: CharacterDescription,
}

/// A livestock template's LivestockDescription, resolved: the product's pile template, every
/// `interval` × 300 s, and the stock slot.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LivestockKind {
    pub product: Option<usize>,
    pub interval: f32,
    pub group: i32,
}

// +0x28; +0x30
/// LivestockComponent: the pasture (entity), towards the next product.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Livestock {
    pub pasture: Option<usize>,
    pub timer: i64,
}

// +0x28; +0x30; +0x40; +0x44
/// PastureComponent's livestock side: the kind (animal template), its animals in joining order, the
/// most it keeps (the window's slider), products gathered; with the MapComponent's area (x, y,
/// width, height).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Pasture {
    pub kind: Option<usize>,
    pub livestock: Vec<usize>,
    pub limit: i32,
    pub produced: i32,
    pub area: [i16; 4],
}

/// PopulationComponent.
#[derive(Debug, Clone, PartialEq)]
pub struct Population {
    pub entity: usize,
    pub template: usize,
    // +0x30
    /// Cells per row.
    pub width: i32,
    // +0x38
    /// Capacity, count and next count per cell.
    pub cells: Vec<[i32; 3]>,
    // +0x48
    /// Herd blocks per row.
    pub herd_width: i32,
    // +0x60
    /// Towards the next 300 s step.
    pub clock: i64,
    // +0x68
    /// The next cell to recount.
    pub cursor: i32,
}

/// HerdComponent.
#[derive(Debug, Clone, PartialEq)]
pub struct Herd {
    pub entity: usize,
    pub template: usize,
    // +0x28, +0x30
    /// The population and the herd's block.
    pub population: usize,
    pub index: i32,
    // +0x38
    /// The animals (entities), in joining order.
    pub animals: Vec<usize>,
    // +0x48
    /// Moving to the target (else resting).
    pub moving: bool,
    // +0x4c
    /// The target map cell (−1 none).
    pub target: i32,
    // +0x50; +0x58
    /// The rest time left.
    pub rest: i64,
    /// The time towards the next regrowth.
    pub regrow: i64,
}

/// AnimalComponent and WildAnimalComponent, with the animal's CharacterComponent.
pub struct Animal {
    pub template: usize,
    pub character: Character,
    // +0x28
    /// WildAnimalComponent: the herd (index into [`Wildlife::herds`]).
    pub herd: Option<usize>,
    // +0x20
    /// Component flags ([`HUNTED`], [`WILD`]).
    pub flags: u32,
    // +0x28
    /// The home cell.
    pub home: i32,
    // +0x30
    /// Until the next decision.
    pub timer: i64,
    // +0x38..+0x3e
    /// The area it keeps to (x, y, width, height).
    pub area: [i16; 4],
    // +0x40
    /// Male (the second mesh, antlers).
    pub male: bool,
    // +0x44, +0x48
    /// Age and lifespan (years).
    pub age: f32,
    pub life: f32,
    // +0x50
    /// Until a breeding male's young (livestock only).
    pub breed: i64,
    /// LivestockComponent.
    pub livestock: Option<Livestock>,
    /// The model variant and mesh.
    pub variant: usize,
    pub mesh: usize,
    /// Model scale.
    ///
    /// Original: `FUN_14010c7f0`.
    pub scale: f32,
    /// Queued for removal (entity flag `0x400`).
    ///
    /// Original: `FUN_1400c8370`.
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
    // Game +0x88
    /// The characters' spatial tree (`Game`): citizens, animals, boats and nomads.
    pub tree: crate::behaviour::character_tree::CharacterTree,
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
    /// LivestockDescriptions by animal template.
    pub livestock_kinds: BTreeMap<usize, LivestockKind>,
    // +0x80
    /// The town's livestock without a pasture, in order.
    pub unassigned: Vec<usize>,
    // +0x90
    /// PastureComponents by entity, and the town's list of them.
    pub pastures: BTreeMap<usize, Pasture>,
    pub pasture_list: Vec<usize>,
    /// Nomads by entity, and their kinds by template.
    pub nomads: BTreeMap<usize, crate::people::immigration::Nomad>,
    pub nomad_kinds: BTreeMap<usize, crate::people::immigration::NomadKind>,
    /// Trader boats by entity, and their kinds by template.
    pub boats: BTreeMap<usize, crate::commerce::boat::Boat>,
    pub boat_kinds: BTreeMap<usize, crate::commerce::boat::BoatKind>,
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
    /// A saved PopulationComponent; returns its index.
    ///
    /// Original: `FUN_140054e00`.
    pub fn restore_population(&mut self, p: Population) -> usize {
        let index = self.populations.len();
        self.kinds.insert(p.entity, Kind::Population(index));
        self.populations.push(p);
        index
    }

    /// A saved HerdComponent; returns its index.
    ///
    /// Original: `FUN_140127460`.
    pub fn restore_herd(&mut self, h: Herd) -> usize {
        let index = self.herds.len();
        self.kinds.insert(h.entity, Kind::Herd(index));
        self.herds.push(h);
        index
    }

    /// Restore an animal from a save: its animal component, its wild animal component, its
    /// character and its livestock component.
    ///
    /// Original: `FUN_14010c310` (the animal component).
    pub fn restore_animal(&mut self, entity: usize, a: Animal) {
        self.kinds.insert(entity, Kind::Animal);
        self.animals.insert(entity, a);
    }

    /// An animal dies (killed by a hunter, of old age, or in a herd without a habitat): it is
    /// queued for removal and leaves its herd.
    ///
    /// Original: `FUN_1400c8370` on the animal.
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
        // LivestockComponent's removal slot (`FUN_1401290d0`).
        if let Some(l) = animal.livestock {
            match l.pasture {
                None => self.unassigned.retain(|&x| x != a),
                Some(p) => {
                    if let Some(p) = self.pastures.get_mut(&p) {
                        p.livestock.retain(|&x| x != a);
                    }
                }
            }
        }
        self.dying.push(a);
    }

    /// The animals a pasture has room for, its area over the kind's space per animal (none without
    /// a kind).
    ///
    /// Original: `FUN_14012da40`.
    pub fn capacity(&self, p: usize) -> i32 {
        let Some(pasture) = self.pastures.get(&p) else {
            return 0;
        };
        let Some(kind) = pasture.kind else { return 0 };
        let space = self.animal_kinds.get(&kind).map_or(0, |k| k.desc.space);
        let area = pasture.area[2] as i32 * pasture.area[3] as i32;
        // Deviation: a zero space would divide by zero in the original.
        if space == 0 {
            return 0;
        }
        area / space
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
        crate::town::floor_ground(&self.cells.floors, self.cells.size, self.terrain, cell)
    }

    /// `FUN_1400c8bf0` + `FUN_1400c9d70` for an entity without a map footprint: a placed
    /// entry registered in its template's bucket.
    fn create(&mut self, template: usize, position: [f32; 3]) -> usize {
        let id = self.place(template, position);
        let handle = self.scheduler.register(id, self.templates[template].bucket);
        debug_assert_eq!(handle, id);
        id
    }

    /// The placed entry of [`Wild::create`] alone (its owner registers it in the buckets).
    fn place(&mut self, template: usize, position: [f32; 3]) -> usize {
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
        id
    }

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
        let variant = self.pick_variant(template)?;
        self.insert_animal(template, variant, position, yaw, male)
    }

    /// `FUN_1400c8bf0`'s ModelComponent: the variant (random when the model says so) and its
    /// mesh.
    fn pick_variant(&mut self, template: usize) -> Option<(usize, usize)> {
        self.wildlife.animal_kinds.get(&template)?;
        let t = &self.templates[template];
        let (mut variant, mut mesh) = (0usize, 0usize);
        if let Some(m) = &t.model {
            variant = m.variant.max(0) as usize;
            if m.random_variant {
                variant = self.rng.rand_below(m.variants.len() as u32) as usize;
            }
            mesh = m.mesh.max(0) as usize;
        }
        Some((variant, mesh))
    }

    /// The animal of [`Wild::new_animal`] with its variant chosen (`FUN_14010c8d0`: placed,
    /// turned, wild; a male takes the variant's second mesh).
    fn insert_animal(
        &mut self,
        template: usize,
        (variant, mut mesh): (usize, usize),
        position: [f32; 3],
        yaw: f32,
        male: bool,
    ) -> Option<usize> {
        let kind = self.wildlife.animal_kinds.get(&template)?.clone();
        if let Some(m) = &self.templates[template].model {
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
                livestock: self
                    .wildlife
                    .livestock_kinds
                    .contains_key(&template)
                    .then(Livestock::default),
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
        self.register_animal_in(a, true);
    }

    /// [`Wild::register_animal`]; without `buckets` the owner registers the entity.
    fn register_animal_in(&mut self, a: usize, buckets: bool) {
        let template = self.wildlife.animals[&a].template;
        let position = self.wildlife.animals[&a].character.position;
        let e = if buckets {
            self.create(template, position)
        } else {
            self.place(template, position)
        };
        debug_assert_eq!(e, a);
        self.wildlife.kinds.insert(a, Kind::Animal);
        // CharacterComponent's start (`FUN_14010ee90`): into the tree.
        let ch = &self.wildlife.animals[&a].character;
        let slot = self.wildlife.tree.insert(
            crate::behaviour::character_tree::Who::Animal(a),
            ch.position,
            ch.desc.radius,
        );
        self.wildlife
            .animals
            .get_mut(&a)
            .unwrap()
            .character
            .tree_slot = slot;
        let d = self.wildlife.animal_kinds[&template].desc.clone();
        let life = self.rng.rand_float_tol() * d.life_range + d.life;
        let n = self.n();
        let cell = self.cell_of(position);
        let animal = self.wildlife.animals.get_mut(&a).unwrap();
        animal.life = life;
        if animal.flags & WILD != 0 {
            animal.home = cell;
            animal.area = area_around(n, cell, d.area);
            self.update_animal(a, 0);
        }
        // LivestockComponent's activation (`FUN_140129140`): without a pasture it waits in the
        // town's list.
        let animal = &self.wildlife.animals[&a];
        if animal.livestock.is_some_and(|l| l.pasture.is_none()) {
            self.wildlife.unassigned.push(a);
        }
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
            Some(Kind::Animal) => {
                // CharacterComponent's scheduled notify (`FUN_14010e950`): MoveTo's follow
                // check (`FUN_1400cde40`).
                if let Some(animal) = self.wildlife.animals.get_mut(&e) {
                    let following = self.wildlife.tree.follows(animal.character.tree_slot);
                    animal.character.stack.notify_moves(following);
                }
                self.update_animal(e, elapsed);
                self.update_livestock(e, elapsed);
            }
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

    /// AnimalComponent's scheduled update.
    ///
    /// Original: `FUN_14010b8f0`.
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
        // A breeding male: once grown, young every `breed` years and up to as much again.
        let animal = self.wildlife.animals.get_mut(&a).unwrap();
        if animal.male && animal.flags & BIRTH == 0 && animal.flags & BREEDER != 0 {
            if animal.breed != 0 || animal.age <= d.adult {
                if 0 < animal.breed {
                    animal.breed -= dt;
                    if animal.breed <= 0 {
                        // `FUN_14011de20(.., 0x200000, 0)`.
                        animal.flags |= BIRTH;
                        animal.breed = 0;
                    }
                }
            } else {
                let years = self.rng.rand_float() * d.breed + d.breed;
                self.wildlife.animals.get_mut(&a).unwrap().breed = ticks(years * 3600.0);
            }
        }
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
                // A home under an occupant other than a trading post (`+0x00`, its
                // TradeComponent) or no longer ground: a new home and area.
                let taken = home >= 0
                    && self.cells.occupants[home as usize]
                        .is_some_and(|o| !self.templates[self.placed[o].template].trading_post);
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

    /// A hunter has the animal: it stops what it does and stands.
    ///
    /// Original: `FUN_14010c730`.
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
        let group = {
            let animal = &self.wildlife.animals[&a];
            self.wildlife
                .groups
                .get(&(animal.template, animal.variant))
                .cloned()
        };
        let animal = self.wildlife.animals.get_mut(&a).unwrap();
        match &group {
            Some(g) => animal.character.animation.play(g, anim, 0),
            None => animal.character.animation = AnimationStack::new(anim, 0),
        }
        animal.timer = ticks(secs);
        if turn {
            // FUN_14010b720: (second draw, first draw, 0) normalized; the direction that
            // normalized again, the heading of the first (`Vector3::Heading`).
            let y = self.rng.rand_float_tol();
            let x = self.rng.rand_float_tol();
            let v = crate::math::normalize3([x, y, 0.0]);
            let ch = &mut self.wildlife.animals.get_mut(&a).unwrap().character;
            ch.flag_28 = false;
            ch.direction = crate::math::normalize3(v);
            ch.target_heading = crate::math::heading(v[0], v[1]);
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

    /// LivestockComponent's scheduled update (`FUN_140129020`): in a pasture, an animal with a
    /// product adds one to the pasture's count every `interval` × 300 s.
    fn update_livestock(&mut self, a: usize, dt: i64) {
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
    fn turn_wild(&mut self, a: usize) {
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
    fn pen(&mut self, a: usize) {
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
    /// `RandFloat` × half its lifespan old, which is still unset here (before the setup), so none.
    /// The owner registers the entities.
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
