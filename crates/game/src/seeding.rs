//! New Game resource seeding (`FUN_1400a87d0` → `FUN_1400a7490`, `FUN_1400a6780`): the start
//! structures' draws and the procedural rock, ore and tree pass with spawned herbs, mushrooms and
//! berries. See docs/RESOURCE-SEEDING.md.
//!
//! The original builds full component entities here. This port keeps only what the pass reads
//! and writes — the map cells it stamps, the growth and model choices and every draw from the
//! game's shared random stream, in the original order. It also runs scheduled growth, periodic
//! child placement and natural-resource removal; see docs/GAME-LOOP-STEP-18.md.

use banished_formats::game::{
    GrowthDescription, MapDescription, ModelDescription, NaturalResourceDescription,
    PlacedStructure,
};

use crate::animation::Playback;
use crate::growth::{adjust_map_byte, Growth};
use crate::noise::PerlinNoise;
use crate::random::Random;
use crate::scheduler::{ticks, EntityScheduler};
use crate::spawn::Spawn;
use crate::terrain::{CellType, HeightGrid};
use banished_formats::types::AnimationGroup;
use std::sync::Arc;

/// The descriptions of one template that seeding reads.
#[derive(Debug, Clone)]
pub struct Template {
    /// `Template\X.rsc:resource`.
    pub reference: String,
    /// ComponentDescription +0x20: scheduled bucket; 6 means unscheduled.
    pub bucket: usize,
    pub map: Option<MapDescription>,
    pub model: Option<ModelDescription>,
    /// ModelDescription variant animation groups, cached in the original ExternalFactory.
    pub animations: Vec<Option<Arc<AnimationGroup>>>,
    /// The `AnimAudioEvent`s its clips fire: event resource, SoundEffect and probability.
    pub audio_events: Vec<(String, Option<String>, f32)>,
    pub natural: Option<NaturalResourceDescription>,
    pub growth: Option<GrowthDescription>,
    /// FruitDescription (orchard trees).
    pub fruit: Option<crate::orchard::FruitTemplate>,
    /// Cached external GrowthDescription identity, used by FUN_140126100's pointer comparison.
    pub growth_reference: Option<String>,
    /// Spawn entries: (index into the template table, lo, hi).
    pub spawn: Vec<(usize, f32, f32)>,
    /// Scheduled component callbacks, in the ComponentDescription's construction order.
    pub updates: Vec<ScheduledComponent>,
    /// `CreatePlacedDescription +0x44/+0x48`: building footprint size.
    pub size: Option<(i32, i32)>,
    /// `StorageDescription`: accepted flags `+0x28`, capacity `+0x24`, capacity per footprint
    /// cell `+0x20`.
    pub storage: Option<(u32, i32, bool)>,
    /// Has a `StorageLocationDescription` (a town storage the starting goods go to).
    pub storage_location: bool,
    pub storage_location_flags: u32,
    /// Has a StorageDisplayComponent (its stored goods are drawn on its cells).
    pub storage_display: bool,
    pub market_radius: Option<i32>,
    pub market_location: bool,
    pub has_interact: bool,
    /// InteractDescription "use" points in model space (its point list's category 0).
    pub use_points: Vec<[f32; 3]>,
    /// A natural resource's pile template (NaturalResourceDescription `+0x20`).
    pub pile: Option<usize>,
    /// A pile template's raw material (its RawMaterialDescription) and yields `+0x40`,
    /// `+0x44` (uneducated, educated).
    pub pile_material: Option<usize>,
    pub pile_yield: (i32, i32),
    /// InteractDescription "build" points in model space (category 1): where builders stand
    /// and materials are delivered.
    pub build_points: Vec<[f32; 3]>,
    /// The "use" points' yaws (degrees), and the "create" points (category 2) where a
    /// workplace leaves its output.
    pub use_yaws: Vec<f32>,
    pub create_points: Vec<[f32; 3]>,
    /// The InteractDescription's other named points (name, position, yaw): the trading
    /// post's "boat".
    pub named_points: Vec<(String, [f32; 3], f32)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScheduledComponent {
    Growth,
    Spawn,
}

/// A placed entity.
#[derive(Debug, Clone, PartialEq)]
pub struct Placed {
    /// Index into the template table.
    pub template: usize,
    /// Entity +0x1f: actual registration bucket, which can differ from its own template.
    pub bucket: usize,
    /// Map cell.
    pub cell: (i32, i32),
    /// MapComponent +0x2c/+0x2e, after rotation; profile yards have variable dimensions.
    pub footprint: (i32, i32),
    /// Map position (cells from the terrain centre) and ground height.
    pub position: [f32; 3],
    /// Degrees.
    pub yaw: f32,
    pub variant: usize,
    pub mesh: usize,
    /// NaturalResourceComponent +0x28; its constructor FUN_140036eb0 defaults to 1.0.
    pub growth: f32,
    pub growth_state: Option<Growth>,
    pub spawn_state: Option<Spawn>,
    pub alive: bool,
    /// NaturalResourceComponent flag 0x200000 (FUN_14012bc50).
    pub removal_requested: bool,
    pub animation: Option<Playback>,
    /// Mesh instance scale (`MeshInstance +0x134`).
    pub scale: [f32; 4],
}

/// The gameplay map cells seeding reads and writes (`Game +0xf8`, 48-byte cells).
pub struct Cells {
    pub size: usize,
    /// `+0x29`.
    pub types: Vec<CellType>,
    /// `+0x08`: entity on the cell (index into the placed list).
    pub objects: Vec<Option<usize>>,
    /// +0x00: occupancy used by FUN_1400a4aa0; distinct from stamped objects at +0x08.
    pub occupants: Vec<Option<usize>>,
    /// +0x28: placement admission flag, deliberately unnamed.
    pub flags: Vec<u8>,
    /// +0x10: external ComponentDescription reference from a growth transition.
    pub previous_template: Vec<Option<usize>>,
    /// +0x2c: growth-adjusted map byte (zero in FUN_14001b3f0's constructor).
    pub growth_bytes: Vec<u8>,
    /// `+0x1c..+0x27`: per-cell layer counters (`FUN_1400a4e60`): 0..=5 the
    /// HappinessComponent kinds, 8 the fishing docks.
    pub layers: Vec<[u8; 12]>,
    /// Map `+0x90`: registrations per layer.
    pub layer_totals: [i16; 12],
}

impl Cells {
    pub fn new(size: usize, types: Vec<CellType>) -> Cells {
        Cells {
            size,
            objects: vec![None; types.len()],
            occupants: vec![None; types.len()],
            flags: vec![0; types.len()],
            previous_template: vec![None; types.len()],
            growth_bytes: vec![0; types.len()],
            layers: vec![[0; 12]; types.len()],
            layer_totals: [0; 12],
            types,
        }
    }

    /// `FUN_1400a4e60`: add `delta` to `layer` on every cell within `radius` of `(cx, cy)`
    /// (a byte, wrapping) and to the layer's total.
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
const YAW_RANGE: f32 = 180.0;
/// `FUN_1400560e0` spot check: normal z and height tolerance (`DAT_140178840`, `DAT_140178948`).
const SPAWN_NORMAL: f32 = 0.98;
const SPAWN_HEIGHT_TOLERANCE: f32 = 1000.0;

pub struct Seeder<'a> {
    pub templates: &'a [Template],
    pub terrain: &'a HeightGrid<'a>,
    pub cells: &'a mut Cells,
    pub rng: &'a mut Random,
    pub placed: &'a mut Vec<Placed>,
    /// Removal requests in callback order (FUN_1400c9560 drains this after all buckets).
    pub removed: &'a mut Vec<usize>,
    /// `AnimAudioEvent`s that passed their probability (`FUN_14008d310`): owning entity and
    /// the SoundEffect to play there.
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

    fn ground(&self, x: f32, y: f32) -> (f32, f32) {
        let mut normal = [0.0f32; 3];
        let h = self.terrain.ground(x, y, &mut normal);
        (h, normal[2])
    }

    /// `FUN_1400a7490`: the profile's start structures, relative to the start cell.
    ///
    /// Each is built in "instant" mode (`Game +0xd0 |= 1`), so the build component
    /// (`FUN_14010cfb0`) shows the last (finished) mesh. Component setup draws nothing from the
    /// stream except the model's variant; buildings do not stamp the map on activation.
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
                self.cells.types[i] = CellType(map.cell_type as u8);
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
        let h = self.terrain.ground(px, py, &mut normal);
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

    /// Port of FUN_1400560e0. Startup uses random_growth=true; scheduled spawning uses false.
    /// The child is registered in the PARENT TEMPLATE's bucket (not its own template's bucket).
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
        let Some(g) = self.templates[child].growth.clone() else {
            return None;
        };
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

    /// CreateNaturalResource's start (`FUN_1400cbb20`) on a free cell: clone the template
    /// (`FUN_1400c8bf0`), place it in the cell (`FUN_14012ba10`), growth 0 (`FUN_140126280`),
    /// then a random model variant and mesh (`FUN_14012a8a0`), and register it.
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
            self.cells.types[i] = self.terrain.natural_cell_type(self.cells.size, i);
            self.cells.objects[i] = None;
        }
    }

    /// Port of FUN_1400c9560 → FUN_140125520 for the placed entities' GrowthComponents.
    /// Other component callbacks are not installed yet. `disabled` is Game +0xd0 bit 10,
    /// which suppresses spawning and maximum transitions, but not growth or freezing decay.
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
                changed |= self.scheduled_growth(scheduler, id, elapsed, temperature, disabled);
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

    /// Port of FUN_1400c98a0 → FUN_140125370: a single post-iteration smoothing phase.
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

    /// `FUN_1400a6780`: seed every template of the profile's list over the map, away from the
    /// start `(sx, sy)` by more than `sqrt(radius_sq)` cells.
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
mod tests {
    use super::*;
    use crate::scheduler::ticks;
    use banished_formats::game::ModelVariant;

    fn template() -> Template {
        Template {
            reference: "test:resource".into(),
            bucket: 3,
            map: Some(MapDescription {
                cell_type: 4,
                footprint: String::new(),
                stamp: true,
                no_stamp: false,
                minimap: false,
                color: 0,
            }),
            model: Some(ModelDescription {
                variants: vec![
                    ModelVariant {
                        animations: None,
                        meshes: vec![None; 3]
                    };
                    2
                ],
                variant: 0,
                mesh: 0,
                random_variant: true,
            }),
            natural: None,
            fruit: None,
            animations: Vec::new(),
            audio_events: Vec::new(),
            growth: Some(GrowthDescription {
                max_base: 4.0,
                max_range: 1.0,
                f28: 2.0,
                b2c: false,
                f30: 60.0,
                f34: 20.0,
                b38: false,
                f3c: 0.5,
                f40: 0.5,
                b44: true,
                spawn_radius: 1,
                timer_months: 1,
                spawn_one_in: 1,
                threshold: 0.75,
                max_neighbours: 8,
                max_same: 8,
                f60: 0.0,
                tall: false,
            }),
            growth_reference: Some("test:growth".into()),
            spawn: Vec::new(),
            updates: vec![ScheduledComponent::Growth],
            size: None,
            storage: None,
            storage_location: false,
            storage_location_flags: 0,
            storage_display: false,
            market_radius: None,
            market_location: false,
            has_interact: false,
            use_points: Vec::new(),
            pile: None,
            pile_material: None,
            pile_yield: (0, 0),
            build_points: Vec::new(),
            use_yaws: Vec::new(),
            create_points: Vec::new(),
            named_points: Vec::new(),
        }
    }

    fn parent() -> Placed {
        Placed {
            template: 0,
            bucket: 3,
            cell: (8, 8),
            footprint: (1, 1),
            position: [0.5, 0.5, 0.0],
            yaw: 0.0,
            variant: 1,
            mesh: 2,
            growth: 1.0,
            growth_state: Some(Growth {
                maximum: 4.0,
                timer: 0,
                smoothed: 1.0,
                scheduled: true,
                smoothing: false,
                allow_negative: false,
            }),
            spawn_state: None,
            alive: true,
            removal_requested: false,
            animation: None,
            scale: [1.0; 4],
        }
    }

    #[test]
    fn periodic_children_start_at_zero_and_preserve_draw_order() {
        let templates = [template()];
        let heights = vec![0i16; 33 * 33];
        let terrain = HeightGrid {
            width: 33,
            heights: &heights,
        };
        let mut cells = Cells::new(16, vec![CellType::GROUND; 256]);
        cells.objects[8 * 16 + 8] = Some(0);
        cells.types[8 * 16 + 8] = CellType(4);
        let mut placed = vec![parent()];
        let mut removed = Vec::new();
        let mut animation_events = Vec::new();
        let mut rng = Random::new(290);
        let mut expected = rng.clone();
        // FUN_140125070's order, including the constructor's variant draw which is overwritten.
        expected.rand_float();
        let x = 8 + expected.rand_tol(1);
        let y = 8 + expected.rand_tol(1);
        assert_ne!((x, y), (8, 8));
        expected.rand_below(2);
        let maximum = expected.rand_float_tol() + 4.0;
        expected.rand_float_tol();
        expected.rand_float_tol();
        expected.rand_float_tol();
        let timer = ticks(expected.rand_float() * 300.0);
        let mesh = expected.rand_below(3) as usize;
        let mut world = Seeder {
            templates: &templates,
            terrain: &terrain,
            cells: &mut cells,
            rng: &mut rng,
            placed: &mut placed,
            removed: &mut removed,
            animation_events: &mut animation_events,
        };
        let child = world.periodic_spawn(0).unwrap();
        assert_eq!(world.placed[child].cell, (x, y));
        assert_eq!(world.placed[child].variant, 1);
        assert_eq!(world.placed[child].mesh, mesh);
        assert_eq!(world.placed[child].growth, 0.0);
        let state = world.placed[child].growth_state.as_ref().unwrap();
        assert_eq!(state.maximum.to_bits(), maximum.to_bits());
        assert_eq!(state.timer, timer);
        assert!(!state.smoothing);
        assert_eq!(world.rng.rand(), expected.rand());
        assert_eq!(
            world.cells.objects[y as usize * 16 + x as usize],
            Some(child)
        );
    }

    #[test]
    fn placement_gates_reject_before_constructor_draws() {
        let mut templates = [template()];
        templates[0].growth.as_mut().unwrap().spawn_radius = 0;
        let heights = vec![0i16; 33 * 33];
        let terrain = HeightGrid {
            width: 33,
            heights: &heights,
        };
        let mut cells = Cells::new(16, vec![CellType::GROUND; 256]);
        let mut placed = vec![parent()];
        let mut removed = Vec::new();
        let mut animation_events = Vec::new();
        let mut rng = Random::new(290);
        let mut world = Seeder {
            templates: &templates,
            terrain: &terrain,
            cells: &mut cells,
            rng: &mut rng,
            placed: &mut placed,
            removed: &mut removed,
            animation_events: &mut animation_events,
        };
        for gate in 0..3 {
            world.cells.types[136] = if gate == 0 {
                CellType(4)
            } else {
                CellType::GROUND
            };
            world.cells.flags[136] = if gate == 1 { 1 } else { 0 };
            world.cells.occupants[136] = if gate == 2 { Some(0) } else { None };
            let mut expected = world.rng.clone();
            expected.rand_float();
            expected.rand_tol(0);
            expected.rand_tol(0);
            assert!(world.periodic_spawn(0).is_none());
            assert_eq!(world.rng.rand(), expected.rand());
        }
        assert_eq!(world.placed.len(), 1);
    }

    #[test]
    fn newborns_wait_until_next_bucket_visit_and_removal_clears_occupancy() {
        let mut templates = [template()];
        templates[0].growth.as_mut().unwrap().max_range = 0.0;
        let heights = vec![0i16; 33 * 33];
        let terrain = HeightGrid {
            width: 33,
            heights: &heights,
        };
        let mut cells = Cells::new(16, vec![CellType::GROUND; 256]);
        cells.types[136] = CellType(4);
        cells.objects[136] = Some(0);
        let mut placed = vec![parent()];
        let mut removed = Vec::new();
        let mut animation_events = Vec::new();
        let mut rng = Random::new(290);
        let mut scheduler = EntityScheduler::default();
        scheduler.register(0, 3);
        let mut world = Seeder {
            templates: &templates,
            terrain: &terrain,
            cells: &mut cells,
            rng: &mut rng,
            placed: &mut placed,
            removed: &mut removed,
            animation_events: &mut animation_events,
        };
        assert!(world.iterate(&mut scheduler, 901.0, 60.0, false));
        assert!(world.placed.len() > 1);
        assert!(world.placed[1..].iter().all(|p| p.growth == 0.0));
        world.iterate(&mut scheduler, 1.0, 60.0, false);
        assert!(world.placed[1..].iter().all(|p| p.growth > 0.0));
        world.placed[0].growth_state.as_mut().unwrap().maximum = 1.0;
        world.iterate(&mut scheduler, 1.0, 60.0, false);
        assert!(!world.placed[0].alive);
        assert_eq!(world.cells.objects[136], None);
        assert_eq!(world.cells.types[136], CellType::GROUND);
        assert_eq!(world.cells.previous_template[136], Some(0));
        assert!(!scheduler.visit_bucket(3).iter().any(|v| v.1 == 0));
    }
}
