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
mod animals;
mod herds;
mod livestock;
mod population;
use herds::{area_around, inside, nearest_ground};

// vtable 0x1401604b0; vtable 0x140169fb0; +0x80

use std::collections::BTreeMap;

use unbanished_formats::game::{
    AnimalDescription, CharacterDescription, HerdDescription, PopulationDescription,
};
use unbanished_formats::types::AnimationGroup;

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
    /// The population description.
    pub desc: PopulationDescription,
    /// The herd template it spawns, if any.
    pub herd: Option<usize>,
}

/// The settings of a kind of herd.
#[derive(Debug, Clone)]
pub struct HerdKind {
    /// The herd description.
    pub desc: HerdDescription,
    /// The animal template.
    pub animal: usize,
}

/// The settings of a kind of animal.
#[derive(Debug, Clone)]
pub struct AnimalKind {
    /// The animal description.
    pub desc: AnimalDescription,
    /// The raw-material (pile) templates a hunted animal leaves.
    pub materials: Vec<usize>,
    /// The character description: speeds and turning.
    pub character: CharacterDescription,
}

/// A livestock template's LivestockDescription, resolved: the product's pile template, every
/// `interval` × 300 s, and the stock slot.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LivestockKind {
    /// The raw-material template it produces, if any.
    pub product: Option<usize>,
    /// The time between products, in seconds.
    pub interval: f32,
    /// The stock slot its product counts towards.
    pub group: i32,
}

// +0x28; +0x30
/// LivestockComponent: the pasture (entity), towards the next product.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Livestock {
    /// The pasture, if any.
    pub pasture: Option<usize>,
    /// The time towards the next product.
    pub timer: i64,
}

// +0x28; +0x30; +0x40; +0x44
/// PastureComponent's livestock side: the kind (animal template), its animals in joining order, the
/// most it keeps (the window's slider), products gathered; with the MapComponent's area (x, y,
/// width, height).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Pasture {
    /// The animal template it keeps, if any.
    pub kind: Option<usize>,
    /// Its animals.
    pub livestock: Vec<usize>,
    /// The most animals it keeps.
    pub limit: i32,
    /// The amount produced that is not yet in piles.
    pub produced: i32,
    /// The area (x, y, width, height).
    pub area: [i16; 4],
}

/// PopulationComponent.
#[derive(Debug, Clone, PartialEq)]
pub struct Population {
    /// The entity.
    pub entity: usize,
    /// The template.
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
    /// The entity.
    pub entity: usize,
    /// The template.
    pub template: usize,
    // +0x28, +0x30
    /// The population.
    pub population: usize,
    /// The herd's block in the population.
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
    /// The template.
    pub template: usize,
    /// The character: position and movement.
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
    /// The age in years.
    pub age: f32,
    /// The lifespan in years.
    pub life: f32,
    // +0x50
    /// Until a breeding male's young (livestock only).
    pub breed: i64,
    /// LivestockComponent.
    pub livestock: Option<Livestock>,
    /// The model variant.
    pub variant: usize,
    /// The mesh.
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
    /// The animal populations.
    pub populations: Vec<Population>,
    /// The herds.
    pub herds: Vec<Herd>,
    /// Animals by entity.
    pub animals: BTreeMap<usize, Animal>,
    /// Templates by placed-object template index.
    pub population_kinds: BTreeMap<usize, PopulationKind>,
    /// The herd templates, by placed-object template index.
    pub herd_kinds: BTreeMap<usize, HerdKind>,
    /// The animal templates, by placed-object template index.
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
    /// The pastures, by entity.
    pub pastures: BTreeMap<usize, Pasture>,
    /// The town's list of pastures.
    pub pasture_list: Vec<usize>,
    /// The nomads, by entity.
    pub nomads: BTreeMap<usize, crate::people::immigration::Nomad>,
    /// The nomad templates, by template.
    pub nomad_kinds: BTreeMap<usize, crate::people::immigration::NomadKind>,
    /// The trader boats, by entity.
    pub boats: BTreeMap<usize, crate::commerce::boat::Boat>,
    /// The boat templates, by template.
    pub boat_kinds: BTreeMap<usize, crate::commerce::boat::BoatKind>,
}

/// The town state wildlife reads and writes.
pub struct Wild<'a, 't> {
    /// The animals.
    pub wildlife: &'a mut Wildlife,
    /// The placed objects.
    pub placed: &'a mut Vec<Placed>,
    /// The object templates.
    pub templates: &'a [Template],
    /// The gameplay map.
    pub cells: &'a Cells,
    /// The ground's heights.
    pub terrain: &'a HeightGrid<'t>,
    /// The game's random numbers.
    pub rng: &'a mut Random,
    /// The entity scheduler.
    pub scheduler: &'a mut EntityScheduler<usize>,
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

    fn kill(&mut self, a: usize) {
        self.wildlife.kill(a);
    }
}
