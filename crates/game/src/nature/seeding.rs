//! New Game resource seeding: the start structures' draws and the procedural rock, ore and tree
//! pass with spawned herbs, mushrooms and berries. See docs/RESOURCE-SEEDING.md.
//!
//! The original builds full component entities here. This port keeps only what the pass reads and
//! writes — the map cells it stamps, the growth and model choices and every draw from the game's
//! shared random stream, in the original order. It also runs scheduled growth, periodic child
//! placement and natural-resource removal; see docs/GAME-LOOP-STEP-18.md.
//!
//! Original: `FUN_1400a87d0`, `FUN_1400a7490`, `FUN_1400a6780`.

mod growth;
mod spawning;
mod start;

use unbanished_formats::game::{
    GrowthDescription, MapDescription, ModelDescription, NaturalResourceDescription,
    PlacedStructure,
};

use crate::behaviour::animation::Playback;
use crate::nature::growth::{adjust_map_byte, Growth};
use crate::nature::spawn::Spawn;
use crate::noise::PerlinNoise;
use crate::random::Random;
use crate::scheduler::{ticks, EntityScheduler};
use crate::terrain::{CellType, HeightGrid};
use std::sync::Arc;
use unbanished_formats::types::AnimationGroup;

/// The descriptions of one template that seeding reads.
#[derive(Debug, Clone)]
pub struct Template {
    /// `Template\X.rsc:resource`.
    pub reference: String,
    // +0x20
    /// ComponentDescription: scheduled bucket; 6 means unscheduled.
    pub bucket: usize,
    /// The MapDescription: the footprint on the map.
    pub map: Option<MapDescription>,
    /// The ModelDescription: the model's variants.
    pub model: Option<ModelDescription>,
    /// ModelDescription variant animation groups, cached in the original ExternalFactory.
    pub animations: Vec<Option<Arc<AnimationGroup>>>,
    /// The `AnimAudioEvent`s its clips fire: event resource, SoundEffect and probability.
    pub audio_events: Vec<(String, Option<String>, f32)>,
    /// The NaturalResourceDescription (trees, rocks, crops).
    pub natural: Option<NaturalResourceDescription>,
    /// The GrowthDescription.
    pub growth: Option<GrowthDescription>,
    /// FruitDescription (orchard trees).
    pub fruit: Option<crate::production::orchard::FruitTemplate>,
    // Compared by pointer in FUN_140126100.
    /// The reference of the growth description, to compare two resources' growth.
    pub growth_reference: Option<String>,
    /// Spawn entries: (index into the template table, lo, hi).
    pub spawn: Vec<(usize, f32, f32)>,
    /// Scheduled component callbacks, in the ComponentDescription's construction order.
    pub updates: Vec<ScheduledComponent>,
    // CreatePlacedDescription +0x44/+0x48
    /// `CreatePlacedDescription`: building footprint size.
    pub size: Option<(i32, i32)>,
    // +0x28; +0x24; +0x20
    /// `StorageDescription`: accepted flags, capacity, capacity per footprint cell.
    pub storage: Option<(u32, i32, bool)>,
    /// Has a `StorageLocationDescription` (a town storage the starting goods go to).
    pub storage_location: bool,
    /// The `StorageLocationDescription`'s accepted flags.
    pub storage_location_flags: u32,
    /// Has a StorageDisplayComponent (its stored goods are drawn on its cells).
    pub storage_display: bool,
    /// A market's radius.
    pub market_radius: Option<i32>,
    /// Whether it is a market (a town storage with a radius).
    pub market_location: bool,
    /// Whether it has an InteractDescription.
    pub has_interact: bool,
    /// InteractDescription "use" points in model space (its point list's category 0).
    pub use_points: Vec<[f32; 3]>,
    // +0x20
    /// A natural resource's pile template (NaturalResourceDescription).
    pub pile: Option<usize>,
    // +0x40; +0x44
    /// A pile template's raw material (its RawMaterialDescription).
    pub pile_material: Option<usize>,
    /// A pile template's yields: (uneducated, educated).
    pub pile_yield: (i32, i32),
    /// InteractDescription "build" points in model space (category 1): where builders stand
    /// and materials are delivered.
    pub build_points: Vec<[f32; 3]>,
    /// The "use" points' yaws, in degrees.
    pub use_yaws: Vec<f32>,
    /// The "create" points (category 2) where a workplace leaves its output.
    pub create_points: Vec<[f32; 3]>,
    /// The InteractDescription's other named points (name, position, yaw): the trading
    /// post's "boat".
    pub named_points: Vec<(String, [f32; 3], f32)>,
    /// FloorDescription: its picking mesh's triangles in model space (bridge pieces, docks,
    /// porches).
    pub floor: Option<std::sync::Arc<Vec<[[f32; 3]; 3]>>>,
    // +0x28
    /// FloorDescription: the floor is added on activation (bridge pieces); the others are added
    /// when their MultiObject is built.
    ///
    /// Original: `FUN_140124810`, `FUN_14012aa30`.
    pub floor_on_activate: bool,
    /// TerraformDescription (tunnels).
    pub terraform: Option<crate::world::terraform::TerraformTemplate>,
    /// TornadoDescription.
    pub tornado: Option<crate::disasters::tornado::TornadoTemplate>,
    /// QuarryDescription.
    pub quarry: Option<crate::production::quarry::QuarryTemplate>,
    /// Has a TradeComponent (the trading post).
    pub trading_post: bool,
    /// Has a TrackerComponent (workplaces whose production the window shows).
    pub tracker: bool,
    // FUN_1400e02d0 sets only the MapComponent.
    /// Whether this is a fenced area's template (it has a `createfenced` component). Its model is
    /// never placed, so it stays at the origin with no rotation; `Placed::yaw` is its rotation on
    /// the map.
    pub fenced_area: bool,
}

/// The kinds of scheduled component the port runs for placed entities.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScheduledComponent {
    /// The growth update.
    Growth,
    /// The spawn update.
    Spawn,
}

/// A placed entity.
#[derive(Debug, Clone, PartialEq)]
pub struct Placed {
    /// Index into the template table.
    pub template: usize,
    // +0x1f
    /// Entity: actual registration bucket, which can differ from its own template.
    pub bucket: usize,
    /// Map cell.
    pub cell: (i32, i32),
    // +0x2c/+0x2e
    /// MapComponent, after rotation; profile yards have variable dimensions.
    pub footprint: (i32, i32),
    /// Map position (cells from the terrain centre) and ground height.
    pub position: [f32; 3],
    /// The heading, in degrees.
    pub yaw: f32,
    /// The model variant.
    pub variant: usize,
    /// The mesh.
    pub mesh: usize,
    // NaturalResourceComponent +0x28; the constructor FUN_140036eb0 sets 1.0.
    /// How grown the resource is (1.0 by default).
    pub growth: f32,
    /// The growth component, if any.
    pub growth_state: Option<Growth>,
    /// The spawn component, if any.
    pub spawn_state: Option<Spawn>,
    /// Whether it is still in the world.
    pub alive: bool,
    /// NaturalResourceComponent flag 0x200000.
    ///
    /// Original: `FUN_14012bc50`.
    pub removal_requested: bool,
    /// The animation it plays, if any.
    pub animation: Option<Playback>,
    // MeshInstance +0x134
    /// Mesh instance scale (`MeshInstance`).
    pub scale: [f32; 4],
}

impl Placed {
    // +0x148
    /// The entity transform: `FromYawPitchOffset(yaw, 0, position)`, or for a fenced area, whose
    /// model is never placed, the identity at its position.
    pub fn model(&self, t: &Template) -> crate::math::Transform {
        if t.fenced_area {
            crate::math::Transform {
                q: [0.0, 0.0, 0.0, 1.0],
                p: self.position,
            }
        } else {
            crate::math::Transform::of_model(self.yaw, self.position)
        }
    }
}

/// Stamps for [`Cells::types_revision`], unique across every `Cells`.
static TYPES_REVISION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

fn next_types_revision() -> u64 {
    TYPES_REVISION.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

// Game +0xf8
/// The gameplay map cells seeding reads and writes (`Game`, 48-byte cells).
pub struct Cells {
    /// The map's size in cells.
    pub size: usize,
    // +0x29
    /// The type of each cell. Change it only with [`Cells::set_type`], which keeps [`Cells::types_revision`] current.
    pub types: Vec<CellType>,
    /// A new stamp whenever `types` may have changed (a new map, any `set_type`), so readers
    /// such as the path regions can skip comparing the whole map when it has not.
    pub types_revision: u64,
    // +0x08
    /// Entity on the cell (index into the placed list).
    pub objects: Vec<Option<usize>>,
    // +0x00; +0x08; used by FUN_1400a4aa0.
    /// The entity occupying each cell, for placement checks (separate from the objects stamped
    /// into the cell).
    pub occupants: Vec<Option<usize>>,
    // +0x28
    /// Placement admission flag, deliberately unnamed.
    pub flags: Vec<u8>,
    // +0x10
    /// External ComponentDescription reference from a growth transition.
    pub previous_template: Vec<Option<usize>>,
    // +0x2c; zero in FUN_14001b3f0's constructor.
    /// A byte per cell that growth adjusts.
    pub growth_bytes: Vec<u8>,
    // +0x1c..+0x27
    /// Per-cell layer counters: 0..=5 the HappinessComponent kinds, 8 the fishing docks.
    ///
    /// Original: `FUN_1400a4e60`.
    pub layers: Vec<[u8; 12]>,
    // +0x90
    /// Map: registrations per layer.
    pub layer_totals: [i16; 12],
    // +0x2a
    /// The road kind the cell has or will have (0 stone, 1 dirt; `0xff` none).
    pub road_kind: Vec<u8>,
    // +0x2b
    /// The road kind built (`0x80` while it is being built).
    pub road_built: Vec<u8>,
    /// Bumped whenever a road is planned, taken back or finished (the road decals' rebuild).
    ///
    /// Original: `FUN_1400b4840`.
    pub road_revision: u32,
    // +0x2d
    /// The floors (bridge decks) that characters walk on.
    pub floors: crate::world::floor::Floors,
    /// Cells that turned walkable or unwalkable, in the order they did: the path sectors' marks,
    /// which every type write in the original makes on the spot.
    ///
    /// Original: `FUN_1400ae160`.
    pub sector_marks: Vec<u32>,
}

impl Cells {
    // +0x00; +0x08
    /// The entity a cell answers for, the occupant if there is one, else the object. EditTool
    /// selection, the removal tool and the job code all read cells this way.
    ///
    /// Original: `FUN_1400a5750`.
    pub fn entity_at(&self, i: usize) -> Option<usize> {
        self.occupants[i].or(self.objects[i])
    }

    /// The cells of a map of `size × size` cells with these natural types.
    pub fn new(size: usize, types: Vec<CellType>) -> Cells {
        Cells {
            size,
            types_revision: next_types_revision(),
            objects: vec![None; types.len()],
            occupants: vec![None; types.len()],
            flags: vec![0; types.len()],
            previous_template: vec![None; types.len()],
            growth_bytes: vec![0; types.len()],
            layers: vec![[0; 12]; types.len()],
            layer_totals: [0; 12],
            road_kind: vec![0xff; types.len()],
            road_built: vec![0; types.len()],
            floors: crate::world::floor::Floors::new(types.len()),
            types,
            road_revision: 0,
            sector_marks: Vec::new(),
        }
    }

    // +0x29; each writer tests the masks 0xe1f and 0x1e0 before FUN_1400ae160.
    /// Write a cell's type. When the cell turns walkable or unwalkable, its path sector is marked
    /// for rebuilding.
    pub fn set_type(&mut self, i: usize, ty: CellType) {
        let walk = |t: CellType| (1u32 << (t.0 & 0x1f)) & crate::pathfind::WALKABLE != 0;
        if walk(self.types[i]) != walk(ty) {
            self.sector_marks.push(i as u32);
        }
        self.types[i] = ty;
        self.types_revision = next_types_revision();
    }

    /// Add `delta` to `layer` on every cell within `radius` of `(cx, cy)` (a byte, wrapping) and to
    /// the layer's total.
    ///
    /// Original: `FUN_1400a4e60`.
    pub fn add_layer(&mut self, layer: usize, cx: i32, cy: i32, radius: i32, delta: i32) {
        let n = self.size as i32;
        for y in cy - radius..=cy + radius {
            for x in cx - radius..=cx + radius {
                let (dx, dy) = (cx - x, cy - y);
                if x < 0 || y < 0 || x >= n || y >= n || dx * dx + dy * dy > radius * radius {
                    continue;
                }
                let b = &mut self.layers[(n * y + x) as usize][layer];
                *b = b.wrapping_add(delta as u8);
            }
        }
        self.layer_totals[layer] = self.layer_totals[layer].wrapping_add(delta as i16);
    }
}

/// `DAT_140178814`, `DAT_14017884c`.
const HALF: f32 = 0.5;
const ONE: f32 = 1.0;
/// `DAT_140178920`: yaw range (degrees) of the jittered transform.
pub(crate) const YAW_RANGE: f32 = 180.0;
/// `FUN_1400560e0` spot check: normal z and height tolerance (`DAT_140178840`, `DAT_140178948`).
const SPAWN_NORMAL: f32 = 0.98;
const SPAWN_HEIGHT_TOLERANCE: f32 = 1000.0;

/// What the growth and spawn updates need: the cells, the templates, the random numbers, and the
/// lists to fill.
pub struct Seeder<'a> {
    /// The object templates.
    pub templates: &'a [Template],
    /// The ground's heights.
    pub terrain: &'a HeightGrid<'a>,
    /// The gameplay map.
    pub cells: &'a mut Cells,
    /// The game's random numbers.
    pub rng: &'a mut Random,
    /// The placed objects.
    pub placed: &'a mut Vec<Placed>,
    /// The entities to remove, in the order they asked. They are removed after all buckets.
    ///
    /// Original: drained by `FUN_1400c9560`.
    pub removed: &'a mut Vec<usize>,
    /// `AnimAudioEvent`s that passed their probability: owning entity and the SoundEffect to play
    /// there.
    ///
    /// Original: `FUN_14008d310`.
    pub animation_events: &'a mut Vec<(usize, String)>,
}

impl<'a> Seeder<'a> {
    fn n(&self) -> i32 {
        self.cells.size as i32
    }

    /// Map position of a cell centre: `cell − (n/2 − 0.5)`.
    fn cell_position(&self, index: i32) -> (f32, f32) {
        let n = self.n();
        let c = (n >> 1) as f32 - HALF;
        ((index % n) as f32 - c, (index / n) as f32 - c)
    }

    /// `FUN_1400a48b0(.., 0, normal)`: the ground, or a floor above it, and the normal's z.
    fn ground(&self, x: f32, y: f32) -> (f32, f32) {
        let mut normal = [0.0f32; 3];
        let h = self
            .cells
            .floors
            .ground_normal(self.cells.size, self.terrain, x, y, &mut normal);
        (h, normal[2])
    }
}

#[cfg(test)]
mod tests;
