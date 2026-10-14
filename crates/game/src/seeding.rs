//! New Game resource seeding (`FUN_1400a87d0` → `FUN_1400a7490`, `FUN_1400a6780`): the start
//! structures' draws and the procedural rock, ore and tree pass with spawned herbs, mushrooms and
//! berries. See docs/RESOURCE-SEEDING.md.
//!
//! The original builds full component entities here. This port keeps only what the pass reads
//! and writes — the map cells it stamps, the growth and model choices and every draw from the
//! game's shared random stream, in the original order — and returns the placed entities for the
//! renderer and later simulation.

use banished_formats::game::{
    GrowthDescription, MapDescription, ModelDescription, NaturalResourceDescription,
    PlacedStructure,
};

use crate::noise::PerlinNoise;
use crate::random::Random;
use crate::terrain::{CellType, HeightGrid};

/// The descriptions of one template that seeding reads.
#[derive(Debug, Clone)]
pub struct Template {
    /// `Template\X.rsc:resource`.
    pub reference: String,
    pub map: Option<MapDescription>,
    pub model: Option<ModelDescription>,
    pub natural: Option<NaturalResourceDescription>,
    pub growth: Option<GrowthDescription>,
    /// Spawn entries: (index into the template table, lo, hi).
    pub spawn: Vec<(usize, f32, f32)>,
    /// `CreatePlacedDescription +0x44/+0x48`: building footprint size.
    pub size: Option<(i32, i32)>,
    /// `StorageDescription`: accepted flags `+0x28`, capacity `+0x24`, capacity per footprint
    /// cell `+0x20`.
    pub storage: Option<(u32, i32, bool)>,
    /// Has a `StorageLocationDescription` (a town storage the starting goods go to).
    pub storage_location: bool,
}

/// A placed entity.
#[derive(Debug, Clone, PartialEq)]
pub struct Placed {
    /// Index into the template table.
    pub template: usize,
    /// Map cell.
    pub cell: (i32, i32),
    /// Map position (cells from the terrain centre) and ground height.
    pub position: [f32; 3],
    /// Degrees.
    pub yaw: f32,
    pub variant: usize,
    pub mesh: usize,
    /// Growth value (`NaturalResourceComponent +0x28`); 0 without a growth component.
    pub growth: f32,
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
}

impl Cells {
    pub fn new(size: usize, types: Vec<CellType>) -> Cells {
        Cells {
            size,
            objects: vec![None; types.len()],
            types,
        }
    }
}

/// `DAT_140178814`, `DAT_14017884c`.
const HALF: f32 = 0.5;
const ONE: f32 = 1.0;
/// `DAT_140178920`: yaw range (degrees) of the jittered transform.
const YAW_RANGE: f32 = 180.0;
/// `DAT_1401787f4`: width growth divisor of `tall` growth shapes.
const TALL_WIDTH: f32 = 0.15;
/// `FUN_1400560e0` spot check: normal z and height tolerance (`DAT_140178840`, `DAT_140178948`).
const SPAWN_NORMAL: f32 = 0.98;
const SPAWN_HEIGHT_TOLERANCE: f32 = 1000.0;

pub struct Seeder<'a> {
    pub templates: &'a [Template],
    pub terrain: &'a HeightGrid<'a>,
    pub cells: Cells,
    pub rng: &'a mut Random,
    pub placed: Vec<Placed>,
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
            let Some(model) = self.templates[t].model.as_ref() else {
                continue;
            };
            let variant = self.rng.rand_below(model.variants.len() as u32) as usize;
            let mesh = model
                .variants
                .get(variant)
                .map_or(0, |v| v.meshes.len().saturating_sub(1));
            self.placed.push(Placed {
                template: t,
                cell: (x, y),
                position,
                yaw: record.rotation.rem_euclid(360) as f32,
                variant,
                mesh,
                growth: 0.0,
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
    fn set_growth(&mut self, template: usize, max_growth: f32, fraction: f32) -> (f32, [f32; 4]) {
        let growth = fraction * max_growth;
        let clamped = growth.min(ONE);
        let tall = self.templates[template]
            .growth
            .as_ref()
            .is_some_and(|g| g.tall);
        let scale = if tall {
            let width = (clamped / TALL_WIDTH).clamp(0.0, ONE);
            [width, width, growth.min(ONE), ONE]
        } else {
            [clamped; 4]
        };
        // Growth timer: `RandFloat · timer_months · 300` seconds (not simulated yet).
        self.rng.rand_float();
        (growth, scale)
    }

    /// `FUN_1400c9d70` → `FUN_140129840`: register and stamp the map footprint (natural
    /// resources: one cell of the description's type).
    fn register(&mut self, placed: Placed) -> usize {
        let id = self.placed.len();
        let t = &self.templates[placed.template];
        if let Some(map) = &t.map {
            if map.stamp && !map.no_stamp {
                let i = (placed.cell.1 * self.n() + placed.cell.0) as usize;
                self.cells.types[i] = CellType(map.cell_type as u8);
                self.cells.objects[i] = Some(id);
            }
        }
        self.placed.push(placed);
        id
    }

    /// `FUN_140126100`: at most `max_neighbours` occupied cells and `max_same` of the same
    /// growth type among the 8 neighbours. The original compares growth-description pointers;
    /// every shipped growth description belongs to exactly one template, so templates are
    /// compared here.
    fn density_ok(&self, template: usize, x: i32, y: i32) -> bool {
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
                    if self.placed[o].template == template {
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
        let (px, py) = self.cell_position(i);
        let mut normal = [0.0f32; 3];
        let h = self.terrain.ground(px, py, &mut normal);
        SPAWN_NORMAL <= normal[2] && (h - 0.0).abs() <= SPAWN_HEIGHT_TOLERANCE
    }

    /// `FUN_140056620`: spawn children around a newly grown parent.
    fn spawn(&mut self, parent: usize) {
        let template = self.placed[parent].template;
        let growth = self.placed[parent].growth;
        let (px, py) = self.placed[parent].cell;
        for (child, lo, hi) in self.templates[template].spawn.clone() {
            if !(lo <= growth && growth <= hi) {
                continue;
            }
            let step = self.templates[child]
                .growth
                .as_ref()
                .map_or(1.0, |g| g.max_base);
            let attempts = (growth / step) as i32 + 1;
            for _ in 0..attempts {
                self.spawn_one(child, px, py);
            }
        }
    }

    /// `FUN_1400560e0(…, random_growth = true)`.
    fn spawn_one(&mut self, child: usize, px: i32, py: i32) {
        let Some(g) = self.templates[child].growth.clone() else {
            return;
        };
        if self.rng.rand_float() > ONE / g.spawn_one_in as f32 {
            return;
        }
        let x = px + self.rng.rand_tol(g.spawn_radius);
        let y = py + self.rng.rand_tol(g.spawn_radius);
        let n = self.n();
        if x < 0 || y < 0 || x >= n || y >= n || !self.spawn_spot_ok(x, y) {
            return;
        }
        if !self.density_ok(child, x, y) {
            return;
        }
        let (variant, mesh, max_growth) = self.construct(child);
        let (position, yaw) = self.position(child, n * y + x);
        let (mut growth, mut scale) = self.set_growth(child, max_growth, 0.0);
        let id = self.register(Placed {
            template: child,
            cell: (x, y),
            position,
            yaw,
            variant,
            mesh,
            growth,
            scale,
        });
        let fraction = self.rng.rand_float();
        (growth, scale) = self.set_growth(child, max_growth, fraction);
        self.placed[id].growth = growth;
        self.placed[id].scale = scale;
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
                    let (mut growth, mut scale) = (0.0, [ONE; 4]);
                    if self.templates[template].growth.is_some() {
                        let fraction = self.rng.rand_float();
                        (growth, scale) = self.set_growth(template, max_growth, fraction);
                    }
                    let id = self.register(Placed {
                        template,
                        cell: (x, y),
                        position,
                        yaw,
                        variant,
                        mesh,
                        growth,
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
