//! New Game resource seeding: the start structures' draws and the procedural rock, ore and tree
//! pass with spawned herbs, mushrooms and berries. See docs/RESOURCE-SEEDING.md.
//!
//! The original builds full component entities here. This port keeps only what the pass reads and
//! writes — the map cells it stamps, the growth and model choices and every draw from the game's
//! shared random stream, in the original order. It also runs scheduled growth, periodic child
//! placement and natural-resource removal; see docs/GAME-LOOP-STEP-18.md.
//!
//! Original: `FUN_1400a87d0`, `FUN_1400a7490`, `FUN_1400a6780`.

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

    // Game +0xd0 |= 1
    /// The profile's start structures, relative to the start cell.
    ///
    /// Each is built in "instant" mode (`Game |= 1`), so the build component shows the last
    /// (finished) mesh. Component setup draws nothing from the stream except the model's variant;
    /// buildings do not stamp the map on activation.
    ///
    /// Original: `FUN_1400a7490`, `FUN_14010cfb0`.
    pub fn place_structures(
        &mut self,
        structures: &[(PlacedStructure, Option<usize>)],
        sx: i32,
        sy: i32,
    ) {
        let n = self.n();
        let corner = |s: &Self, x: i32, y: i32| {
            let i = n * y + x;
            let half = (n >> 1) as f32;
            let (px, py) = ((i % n) as f32 - half, (i / n) as f32 - half);
            let (z, _) = s.ground(px, py);
            [px, py, z]
        };
        for (record, template) in structures {
            let Some(t) = *template else { continue };
            // Size from the template, otherwise the record's two integers; swapped when turned
            // by ±90 or 270 degrees.
            let (mut w, mut h) = self.templates[t].size.unwrap_or((record.a, record.b));
            if matches!(record.rotation, 90 | -90 | 270) {
                std::mem::swap(&mut w, &mut h);
            }
            let (x, y) = (sx + record.x, sy + record.y);
            let p0 = corner(self, x, y);
            let p1 = corner(self, x + w, y + h);
            let position = [
                (p1[0] + p0[0]) * HALF,
                (p1[1] + p0[1]) * HALF,
                (p1[2] + p0[2]) * HALF,
            ];
            self.construct(t);
            // The yard is an entity with storage/map components but no model. Retain it;
            // only model-bearing structures consume the model-variant draw.
            let (variant, mesh) = self.templates[t].model.as_ref().map_or((0, 0), |model| {
                let variant = self.rng.rand_below(model.variants.len() as u32) as usize;
                let mesh = model
                    .variants
                    .get(variant)
                    .map_or(0, |v| v.meshes.len().saturating_sub(1));
                (variant, mesh)
            });
            self.placed.push(Placed {
                template: t,
                bucket: self.templates[t].bucket,
                cell: (x, y),
                footprint: (w, h),
                position,
                yaw: record.rotation.rem_euclid(360) as f32,
                variant,
                mesh,
                growth: 0.0,
                growth_state: None,
                spawn_state: None,
                alive: true,
                removal_requested: false,
                animation: None,
                scale: [ONE; 4],
            });
        }
    }

    /// `FUN_1400c8bf0` setup pass for a natural resource: the model component picks a variant
    /// (`FUN_140129f90`) and the growth component its maximum (`FUN_140125bc0`), in template
    /// order (map, model, naturalresource, growth, spawn).
    fn construct(&mut self, template: usize) -> (usize, usize, f32) {
        let t = &self.templates[template];
        let (mut variant, mut mesh) = (0usize, 0usize);
        if let Some(m) = &t.model {
            variant = m.variant.max(0) as usize;
            if m.random_variant {
                variant = self.rng.rand_below(m.variants.len() as u32) as usize;
            }
            mesh = m.mesh.max(0) as usize;
        }
        let mut max_growth = 1.0;
        if let Some(g) = &t.growth {
            max_growth = if g.max_base <= 0.0 {
                -1.0
            } else {
                self.rng.rand_float_tol() * g.max_range + g.max_base
            };
        }
        (variant, mesh, max_growth)
    }

    /// `FUN_14012ba10`: jitter the entity inside its cell and pick a yaw.
    fn position(&mut self, template: usize, index: i32) -> ([f32; 3], f32) {
        let scale = self.templates[template]
            .natural
            .as_ref()
            .map_or(0.0, |n| n.position_scale);
        let (x, y) = self.cell_position(index);
        let (z, _) = self.ground(x, y);
        let dy = self.rng.rand_float_tol();
        let dx = self.rng.rand_float_tol();
        let position = [dx * scale + x, dy * scale + y, z + 0.0];
        let yaw = self.rng.rand_float_tol() * YAW_RANGE;
        (position, yaw)
    }

    /// `FUN_140126280`: set the growth from a 0..1 fraction of the maximum and draw the timer.
    fn set_growth(
        &mut self,
        template: usize,
        max_growth: f32,
        fraction: f32,
    ) -> (Growth, f32, [f32; 4]) {
        Growth::initialise(
            self.templates[template].growth.as_ref().unwrap(),
            max_growth,
            fraction,
            self.rng,
        )
    }

    /// `FUN_1400c9d70` → `FUN_140129840`: register and stamp the map footprint (natural
    /// resources: one cell of the description's type).
    fn register(&mut self, placed: Placed) -> usize {
        let id = self.placed.len();
        if let Some(map) = self.templates[placed.template].map.clone() {
            if map.stamp && !map.no_stamp {
                let i = (placed.cell.1 * self.n() + placed.cell.0) as usize;
                // FUN_1400a30f0 queues the old stamped entity's removal before replacing it.
                if let Some(old) = self.cells.objects[i] {
                    self.remove(old);
                }
                self.cells.set_type(i, CellType(map.cell_type as u8));
                self.cells.objects[i] = Some(id);
            }
        }
        self.placed.push(placed);
        id
    }

    /// `FUN_140126100`: at most `max_neighbours` occupied cells and `max_same` of the same
    /// growth type among the 8 neighbours. Cached external-reference identities replace the
    /// original GrowthDescription pointer comparison (including descriptions shared by templates).
    pub(crate) fn density_ok(&self, template: usize, x: i32, y: i32) -> bool {
        let Some(growth) = &self.templates[template].growth else {
            return true;
        };
        let n = self.n();
        let (mut occupied, mut same) = (0, 0);
        for cy in y - 1..y + 2 {
            for cx in x - 1..x + 2 {
                if (cx == x && cy == y) || cx < 0 || cy < 0 || cx >= n || cy >= n {
                    continue;
                }
                if let Some(o) = self.cells.objects[(cy * n + cx) as usize] {
                    occupied += 1;
                    if self.templates[self.placed[o].template].growth_reference
                        == self.templates[template].growth_reference
                    {
                        same += 1;
                    }
                }
            }
        }
        !(growth.max_same < same || growth.max_neighbours < occupied)
    }

    /// `FUN_1400a4aa0(map, x, y, 4, 0.98, 1000, 0, …)`: buildable ground with no entity and a
    /// flat enough surface.
    fn spawn_spot_ok(&self, x: i32, y: i32) -> bool {
        let n = self.n();
        if x < 0 || y < 0 || x >= n || y >= n {
            return false;
        }
        let i = y * n + x;
        if self.cells.types[i as usize] != CellType::GROUND {
            return false;
        }
        if self.cells.occupants[i as usize].is_some() || self.cells.flags[i as usize] != 0 {
            return false;
        }
        let (px, py) = self.cell_position(i);
        let mut normal = [0.0f32; 3];
        let h = self
            .cells
            .floors
            .ground_normal(self.cells.size, self.terrain, px, py, &mut normal);
        SPAWN_NORMAL <= normal[2] && (h - 0.0).abs() <= SPAWN_HEIGHT_TOLERANCE
    }

    /// `FUN_140056620`: spawn children around a newly grown parent.
    fn spawn(&mut self, parent: usize) {
        let template = self.placed[parent].template;
        let growth = self.placed[parent].growth;
        for (entry, (child, lo, hi)) in self.templates[template]
            .spawn
            .clone()
            .into_iter()
            .enumerate()
        {
            if !(lo <= growth && growth <= hi) {
                continue;
            }
            let step = self.templates[child]
                .growth
                .as_ref()
                .map_or(1.0, |g| g.max_base);
            let attempts = (growth / step) as i32 + 1;
            for _ in 0..attempts {
                self.spawn_child(parent, entry, true, false);
            }
        }
    }

    /// Startup uses random_growth=true; scheduled spawning uses false. The child is registered in
    /// the PARENT TEMPLATE's bucket (not its own template's bucket).
    ///
    /// Original: `FUN_1400560e0`.
    pub fn spawn_child(
        &mut self,
        parent: usize,
        entry: usize,
        random_growth: bool,
        disabled: bool,
    ) -> Option<usize> {
        if disabled {
            return None;
        }
        let parent_template = self.placed[parent].template;
        let child = self.templates[parent_template].spawn[entry].0;
        let (px, py) = self.placed[parent].cell;
        let g = self.templates[child].growth.clone()?;
        if self.rng.rand_float() > ONE / g.spawn_one_in as f32 {
            return None;
        }
        let x = px + self.rng.rand_tol(g.spawn_radius);
        let y = py + self.rng.rand_tol(g.spawn_radius);
        let n = self.n();
        if x < 0 || y < 0 || x >= n || y >= n || !self.spawn_spot_ok(x, y) {
            return None;
        }
        if !self.density_ok(child, x, y) {
            return None;
        }
        let (variant, mesh, max_growth) = self.construct(child);
        let (position, yaw) = self.position(child, n * y + x);
        let (state, growth, scale) = self.set_growth(child, max_growth, 0.0);
        let id = self.register(Placed {
            template: child,
            bucket: self.templates[parent_template].bucket,
            cell: (x, y),
            footprint: (1, 1),
            position,
            yaw,
            variant,
            mesh,
            growth,
            growth_state: Some(state),
            spawn_state: self.spawn_state(child),
            alive: true,
            removal_requested: false,
            animation: None,
            scale,
        });
        if random_growth {
            let fraction = self.rng.rand_float();
            let (state, growth, scale) = self.set_growth(child, max_growth, fraction);
            self.placed[id].growth = growth;
            self.placed[id].scale = scale;
            self.placed[id].growth_state = Some(state);
        }
        Some(id)
    }

    /// CreateNaturalResource's start on a free cell: clone the template, place it in the cell,
    /// growth 0, then a random model variant and mesh, and register it.
    ///
    /// Original: `FUN_1400cbb20`, `FUN_1400c8bf0`, `FUN_14012ba10`, `FUN_140126280`,
    /// `FUN_14012a8a0`.
    pub fn plant(&mut self, template: usize, cell: i32) -> usize {
        let (_, _, max_growth) = self.construct(template);
        let (position, yaw) = self.position(template, cell);
        let (state, growth, scale) = self.set_growth(template, max_growth, 0.0);
        let variants = self.templates[template]
            .model
            .as_ref()
            .map_or(1, |m| m.variants.len().max(1));
        let variant = self.rng.rand_below(variants as u32) as usize;
        let meshes = self.templates[template]
            .model
            .as_ref()
            .and_then(|m| m.variants.get(variant))
            .map_or(1, |v| v.meshes.len().max(1));
        let mesh = self.rng.rand_below(meshes as u32) as usize;
        let n = self.n();
        self.register(Placed {
            template,
            bucket: self.templates[template].bucket,
            cell: (cell % n, cell / n),
            footprint: (1, 1),
            position,
            yaw,
            variant,
            mesh,
            growth,
            growth_state: Some(state),
            spawn_state: self.spawn_state(template),
            alive: true,
            removal_requested: false,
            animation: None,
            scale,
        })
    }

    fn spawn_state(&self, template: usize) -> Option<Spawn> {
        self.templates[template]
            .updates
            .contains(&ScheduledComponent::Spawn)
            .then(|| Spawn::new(self.templates[template].spawn.len()))
    }

    /// Port of FUN_140125070: reproduce using the parent's template and model variant. Unlike
    /// FUN_1400560e0's startup children, this path initializes the child's growth at zero and
    /// selects a mesh after the timer draw; it never draws a random initial growth fraction.
    fn periodic_spawn(&mut self, parent: usize) -> Option<usize> {
        let template = self.placed[parent].template;
        let g = self.templates[template].growth.as_ref()?;
        if self.rng.rand_float() > ONE / g.spawn_one_in as f32 {
            return None;
        }
        let (px, py) = self.placed[parent].cell;
        let x = px + self.rng.rand_tol(g.spawn_radius);
        let y = py + self.rng.rand_tol(g.spawn_radius);
        if !self.spawn_spot_ok(x, y) || !self.density_ok(template, x, y) {
            return None;
        }
        let variant = self.placed[parent].variant;
        let (_, _, maximum) = self.construct(template);
        let (position, yaw) = self.position(template, y * self.n() + x);
        let (state, growth, scale) = self.set_growth(template, maximum, 0.0);
        let meshes = self.templates[template]
            .model
            .as_ref()?
            .variants
            .get(variant)?
            .meshes
            .len();
        let mesh = self.rng.rand_below(meshes as u32) as usize;
        Some(self.register(Placed {
            template,
            bucket: self.templates[template].bucket,
            cell: (x, y),
            footprint: (1, 1),
            position,
            yaw,
            variant,
            mesh,
            growth,
            growth_state: Some(state),
            spawn_state: self.spawn_state(template),
            alive: true,
            removal_requested: false,
            animation: None,
            scale,
        }))
    }

    /// Port of FUN_140125a30 and FUN_1400a33b0 for one-cell natural-resource removal. Keep
    /// handles stable; the scheduler swaps its bucket entries separately at the end of the tick.
    fn remove(&mut self, id: usize) {
        let p = &mut self.placed[id];
        if !p.alive {
            return;
        }
        p.alive = false;
        self.removed.push(id);
        let (x, y) = p.cell;
        let i = y as usize * self.cells.size + x as usize;
        if let Some(d) = &self.templates[p.template].growth {
            if p.growth != 0.0 {
                let factor = if self.cells.previous_template[i] == Some(p.template) {
                    d.f3c
                } else {
                    d.f40
                };
                self.cells.growth_bytes[i] =
                    adjust_map_byte(self.cells.growth_bytes[i], p.growth.min(1.0) * factor);
                self.cells.previous_template[i] = Some(p.template);
            }
        }
        if self.cells.objects[i] == Some(id) {
            let ty = self.terrain.natural_cell_type(self.cells.size, i);
            self.cells.set_type(i, ty);
            self.cells.objects[i] = None;
        }
    }

    // +0xd0
    /// One iteration of the placed entities' growth components. `disabled` is the game's bit 10,
    /// which stops spawning and the change at full growth, but not growth or the decay in frost.
    /// The callbacks of other components are not installed yet.
    ///
    /// Original: `FUN_1400c9560`, then `FUN_140125520`.
    pub fn iterate(
        &mut self,
        scheduler: &mut EntityScheduler<usize>,
        dt: f32,
        temperature: f32,
        disabled: bool,
    ) -> bool {
        self.iterate_with(scheduler, dt, temperature, disabled, &mut |_, _, _, _| {
            false
        })
    }

    /// [`Seeder::iterate`] with the scheduled update of entities without growth (`visit`
    /// returns whether the models changed).
    pub fn iterate_with(
        &mut self,
        scheduler: &mut EntityScheduler<usize>,
        dt: f32,
        temperature: f32,
        disabled: bool,
        visit: &mut dyn FnMut(&mut Seeder, &mut EntityScheduler<usize>, usize, i64) -> bool,
    ) -> bool {
        scheduler.advance(dt);
        // FUN_1400c9560's bit-0x04 component phase precedes scheduled growth. Within a tree
        // the ModelComponent (FUN_140129e30) runs before NaturalResource (FUN_14012b110).
        let mut changed = self.animate(ticks(dt));
        for bucket in 0..5 {
            for (_, id, elapsed) in scheduler.visit_bucket(bucket) {
                // The citizens (bucket 1) are visited for the agents (`take_visits`).
                if id >= crate::scheduler::EXTRA {
                    continue;
                }
                changed |= self.scheduled_growth(scheduler, id, elapsed, temperature, disabled);
                // SpawnComponent (`FUN_140056430`) follows Growth in the template's list.
                changed |= self.scheduled_spawn(scheduler, id, elapsed, disabled);
                // The entity's other scheduled components (FruitComponent after Growth,
                // wildlife).
                if self.placed[id].alive {
                    changed |= visit(self, scheduler, id, elapsed);
                }
            }
        }
        // FUN_1400c9560 drains removal requests only after all five buckets.
        for id in self.removed.drain(..) {
            scheduler.remove(id);
        }
        changed
    }

    /// SpawnComponent's scheduled update (`FUN_140056430`) for one visited entity: each entry
    /// whose growth band holds the parent's growth runs its clock, and every child growth
    /// period (`+0x28 × 300`) it tries a child (`FUN_1400560e0`, without random growth).
    fn scheduled_spawn(
        &mut self,
        scheduler: &mut EntityScheduler<usize>,
        id: usize,
        elapsed: i64,
        disabled: bool,
    ) -> bool {
        let p = &self.placed[id];
        if !p.alive || p.spawn_state.is_none() {
            return false;
        }
        let growth = p.growth;
        let entries: Vec<(f32, f32, f32)> = self.templates[p.template]
            .spawn
            .iter()
            .map(|&(child, lo, hi)| {
                let period = self.templates[child].growth.as_ref().map_or(0.0, |g| g.f28);
                (lo, hi, period)
            })
            .collect();
        let attempts = self.placed[id]
            .spawn_state
            .as_mut()
            .unwrap()
            .update(growth, elapsed, &entries);
        let mut changed = false;
        for (entry, count) in attempts {
            for _ in 0..count {
                if let Some(child) = self.spawn_child(id, entry, false, disabled) {
                    let handle = scheduler.register(child, self.placed[child].bucket);
                    debug_assert_eq!(handle, child);
                    changed = true;
                }
            }
        }
        changed
    }

    /// GrowthComponent's scheduled update for one visited entity; true when the models changed.
    fn scheduled_growth(
        &mut self,
        scheduler: &mut EntityScheduler<usize>,
        id: usize,
        elapsed: i64,
        temperature: f32,
        disabled: bool,
    ) -> bool {
        let mut changed = false;
        {
            let p = &mut self.placed[id];
            if !p.alive {
                return changed;
            }
            let Some(state) = &mut p.growth_state else {
                return changed;
            };
            let d = self.templates[p.template].growth.as_ref().unwrap();
            let result = state.update(d, &mut p.growth, elapsed, temperature, disabled);
            if !disabled {
                for _ in 0..result.spawn_attempts {
                    if let Some(child) = self.periodic_spawn(id) {
                        let handle = scheduler
                            .register(child, self.templates[self.placed[child].template].bucket);
                        debug_assert_eq!(handle, child);
                        changed = true;
                    }
                }
            }
            if let Some(adjustment) = result.map_adjustment {
                let p = &self.placed[id];
                let i = (p.cell.1 * self.n() + p.cell.0) as usize;
                self.cells.growth_bytes[i] =
                    adjust_map_byte(self.cells.growth_bytes[i], adjustment);
                self.cells.previous_template[i] = Some(p.template);
                if self.templates[p.template]
                    .natural
                    .as_ref()
                    .is_some_and(|n| n.flag)
                {
                    // FUN_14012bc50 -> FUN_14012b910 -> FUN_14012a5c0: use the selected
                    // model variant's animation group, index 2, start time zero.
                    let p = &mut self.placed[id];
                    p.removal_requested = true;
                    p.animation = self.templates[p.template]
                        .animations
                        .get(p.variant)
                        .and_then(Option::as_ref)
                        .and_then(|g| g.clips.get(2))
                        .map(|_| Playback::new(2, 0));
                } else {
                    self.remove(id);
                    changed = true;
                }
            }
        }
        changed
    }

    /// A single post-iteration smoothing phase.
    ///
    /// Original: `FUN_1400c98a0`, `FUN_140125370`.
    pub fn smooth(&mut self) -> bool {
        let mut changed = false;
        for p in self.placed.iter_mut() {
            if !p.alive {
                continue;
            }
            if let Some(state) = &mut p.growth_state {
                let d = self.templates[p.template].growth.as_ref().unwrap();
                changed |= state.smooth(d, p.growth, &mut p.scale);
            }
        }
        changed
    }

    /// Port of FUN_140129e30 -> Runtime MeshInstance::Update (0x180103090), followed by
    /// FUN_14012b110's completion/removal callback. The clock advances per simulation iteration,
    /// while the published pose uses the old clock; pause runs neither callback.
    fn animate(&mut self, dt: i64) -> bool {
        let mut finished = Vec::new();
        let mut changed = false;
        for (id, p) in self.placed.iter_mut().enumerate() {
            if !p.alive {
                continue;
            }
            let Some(playback) = &mut p.animation else {
                if p.removal_requested {
                    finished.push(id);
                }
                continue;
            };
            let Some(clip) = self.templates[p.template]
                .animations
                .get(p.variant)
                .and_then(Option::as_ref)
                .and_then(|g| g.clips.get(playback.clip))
            else {
                continue;
            };
            if !playback.is_animating(clip) {
                continue;
            }
            for event in playback.update(clip, dt) {
                let Some(reference) = &clip.events[event].resource else {
                    continue;
                };
                // `AnimAudioEvent` slot +0x40 (`FUN_14008d310`): `RandFloat()` from the game
                // stream against the probability, then `PlayEffect3D(effect, 4)` at the entity.
                let audio = self.templates[p.template]
                    .audio_events
                    .iter()
                    .find(|(r, _, _)| r == reference);
                if let Some((_, effect, probability)) = audio {
                    if self.rng.rand_float() <= *probability {
                        if let Some(effect) = effect {
                            self.animation_events.push((id, effect.clone()));
                        }
                    }
                }
            }
            changed = true;
            if !playback.is_animating(clip) && p.removal_requested {
                finished.push(id);
            }
        }
        for id in finished {
            self.remove(id);
            changed = true;
        }
        changed
    }

    fn perlin(&mut self, persistence: f32, octaves: f32) -> PerlinNoise {
        let a = self.rng.rand_tol(0x7ff_ffff);
        let b = self.rng.rand_tol(0x7ff_ffff);
        PerlinNoise::new(persistence, octaves, b, a)
    }

    /// Seed every template of the profile's list over the map, away from the start `(sx, sy)` by
    /// more than `sqrt(radius_sq)` cells.
    ///
    /// Original: `FUN_1400a6780`.
    pub fn seed(&mut self, list: &[usize], sx: i32, sy: i32, radius_sq: i32) {
        // Three fields the pass builds and never reads.
        for _ in 0..3 {
            self.perlin(ONE, ONE);
        }
        let n = self.n();
        for &template in list {
            let Some(nr) = self.templates[template].natural.clone() else {
                continue;
            };
            let variants = self.templates[template]
                .model
                .as_ref()
                .map_or(0, |m| m.variants.len());
            let clusters: Vec<PerlinNoise> = if nr.cluster {
                (0..variants)
                    .map(|_| self.perlin(nr.cluster_persistence, nr.cluster_octaves))
                    .collect()
            } else {
                Vec::new()
            };
            let field = self.perlin(nr.noise_persistence, nr.noise_octaves);
            let inv = ONE / nr.noise_scale;
            for y in 0..n {
                for x in 0..n {
                    let v = field.generate(x as f32 * inv, y as f32 * inv);
                    if nr.noise_min > v * HALF + HALF {
                        continue;
                    }
                    if self.rng.rand_float() > nr.chance {
                        continue;
                    }
                    let index = n * y + x;
                    if self.cells.objects[index as usize].is_some()
                        || (y - sy) * (y - sy) + (x - sx) * (x - sx) < radius_sq
                    {
                        continue;
                    }
                    let (cx, cy) = self.cell_position(index);
                    let (h, up) = self.ground(cx, cy);
                    if h < nr.height_min || (nr.height_max <= h && h != nr.height_max) {
                        continue;
                    }
                    if up < nr.normal_min {
                        continue;
                    }
                    if !self.density_ok(template, x, y) {
                        continue;
                    }
                    let (mut variant, mut mesh, max_growth) = self.construct(template);
                    let (position, yaw) = self.position(template, index);
                    let (mut growth, mut scale, mut growth_state) = (1.0, [ONE; 4], None);
                    if self.templates[template].growth.is_some() {
                        let fraction = self.rng.rand_float();
                        let (state, value, output) =
                            self.set_growth(template, max_growth, fraction);
                        growth = value;
                        scale = output;
                        growth_state = Some(state);
                    }
                    let id = self.register(Placed {
                        template,
                        bucket: self.templates[template].bucket,
                        cell: (x, y),
                        footprint: (1, 1),
                        position,
                        yaw,
                        variant,
                        mesh,
                        growth,
                        growth_state,
                        spawn_state: self.spawn_state(template),
                        alive: true,
                        removal_requested: false,
                        animation: None,
                        scale,
                    });
                    if !self.templates[template].spawn.is_empty() {
                        self.spawn(id);
                    }
                    if nr.cluster {
                        let inv = ONE / nr.cluster_scale;
                        variant = 0;
                        for (k, c) in clusters.iter().enumerate() {
                            let v = c.generate(x as f32 * inv, y as f32 * inv);
                            if nr.cluster_threshold <= v * HALF + HALF {
                                variant = k;
                                break;
                            }
                        }
                        if self.rng.rand_below(nr.random_variant_one_in as u32) == 0 {
                            variant = self.rng.rand_below(variants as u32) as usize;
                        }
                        let meshes = self.templates[template].model.as_ref().unwrap().variants
                            [variant]
                            .meshes
                            .len();
                        mesh = self.rng.rand_below(meshes as u32) as usize;
                        self.placed[id].variant = variant;
                        self.placed[id].mesh = mesh;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
