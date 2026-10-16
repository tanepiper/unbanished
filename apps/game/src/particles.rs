//! Particle systems (Runtime `Graphics::ParticleSystem`) and the entities' ParticleComponents
//! (vtable `0x140160380`). See docs/PARTICLES.md.
//!
//! A system emits `rate` particles a second (up to its most alive), each with a random life,
//! rotation, spin, size and an initial velocity in a cone; every update pushes them by the
//! force, spins and grows them, moves their colour from the start colour to the end colour over
//! `color_span` of their life and fades them in and out (`ParticleSystemInstance::Update`,
//! `0x1800a6550`). The runtime's force, spin and growth changes go by the frame time squared,
//! as there. Particles draw on the runtime's own Random (`Particle::_rng`), never the game's.
//!
//! An entity's ParticleComponent has one system per description slot. A slot starts
//! (`FUN_1400533b0`) at its model node, at the points model's point of that name, or at the
//! model's root (the map rectangle's middle on the ground for slot flag 3), fading its alpha in
//! over the slot's time; it fades out the same way (`FUN_1400539f0`, event 3) and goes when
//! the alpha reaches 0 (`FUN_140053490`).
//!
//! The tornado's funnel is its own particle class (`Tornado`, update `FUN_1400d7940`): its
//! particles are placed, not pushed, each on a spiral that widens and rises with its age about
//! a centre that wobbles round the entity.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use banished_formats::game::{ParticleDescription, ParticleSystem, TornadoParticle};
use banished_formats::pkg::Archive;
use banished_game::random::Random;
use glam::Vec3;

/// `DAT_1801647ec`.
const DEG: f32 = 0.017_453_292;

/// One particle (`0x58` bytes) and its motion (`0x14` bytes).
#[derive(Clone, Copy, Debug)]
struct Particle {
    life: f32,
    age: f32,
    pos: Vec3,
    color: Vec3,
    alpha: f32,
    rotation: f32,
    size: f32,
    velocity: Vec3,
    spin: f32,
    growth: f32,
}

/// A particle resource: a `ParticleSystem` or the `Tornado` class.
#[derive(Clone, Debug)]
pub enum System {
    Standard(ParticleSystem),
    Tornado(TornadoParticle),
}

impl System {
    fn material(&self) -> Option<&String> {
        match self {
            System::Standard(s) => s.material.as_ref(),
            System::Tornado(t) => t.material.as_ref(),
        }
    }
}

/// `Math::Transform::FromDirection` (Runtime `0x180022650`): the rotation whose +Y is `d`
/// (`x, y, z, w`). The other axes come from an up of +Z (−Y when `d.z` is above 0.999, +Y
/// when below −0.999): `x = |d × up|`, `z = |x × d|`, then `Quaternion(x, d, z)`
/// (`0x180022450`).
fn from_direction(d: Vec3) -> [f32; 4] {
    let up = if 0.999 < d.z {
        Vec3::new(0.0, -1.0, 0.0)
    } else if d.z < -0.999 {
        Vec3::new(0.0, 1.0, 0.0)
    } else {
        Vec3::new(0.0, 0.0, 1.0)
    };
    let unit = |v: Vec3| {
        let len = v.length();
        if len != 0.0 {
            v * (1.0 / len)
        } else {
            v
        }
    };
    let x = unit(d.cross(up));
    let z = unit(x.cross(d));
    let (a, b, c) = (x, d, z);
    let part = |v: f32| v.max(0.0).sqrt() * 0.5;
    [
        part(1.0 + a.x - b.y - c.z).copysign(c.y - b.z),
        part(1.0 - a.x + b.y - c.z).copysign(a.z - c.x),
        part(1.0 - a.x - b.y + c.z).copysign(b.x - a.y),
        part(1.0 + a.x + b.y + c.z),
    ]
}

/// `q · v · q⁻¹` as the spawn writes it (`x, y, z, w`).
fn rotate(q: [f32; 4], v: Vec3) -> Vec3 {
    let [x, y, z, w] = q;
    let t0 = w * v.x + z * v.y - y * v.z;
    let t1 = -z * v.x + w * v.y + x * v.z;
    let t2 = y * v.x - x * v.y + w * v.z;
    let t3 = -x * v.x - y * v.y - z * v.z;
    Vec3::new(
        t0 * w - t3 * x - t2 * y + t1 * z,
        t1 * w + t2 * x - t3 * y - t0 * z,
        t2 * w - t1 * x + t0 * y - t3 * z,
    )
}

/// A running system (`ParticleSystemInstance`): its particles and emission state, at a world
/// position with no rotation.
#[derive(Clone, Debug)]
pub struct Instance {
    pub system: Arc<System>,
    pub position: Vec3,
    /// The geometry's alpha (`+0x7c`), the component's fade.
    pub alpha: f32,
    /// Emitting (a tornado stops forming particles at the end of its path).
    pub emitting: bool,
    rate: f32,
    elapsed: f32,
    since_last: f32,
    /// The Tornado instance's wobble angle (`+0xd8`, degrees).
    turn: f32,
    particles: Vec<Particle>,
}

impl Instance {
    pub fn new(system: Arc<System>, position: Vec3) -> Instance {
        Instance {
            rate: match &*system {
                System::Standard(s) => s.rate,
                System::Tornado(t) => t.rate,
            },
            system,
            position,
            alpha: 0.0,
            emitting: true,
            elapsed: 0.0,
            since_last: 0.0,
            turn: 0.0,
            particles: Vec::new(),
        }
    }

    #[cfg(test)]
    fn is_empty(&self) -> bool {
        self.particles.is_empty()
    }

    fn spawn(&mut self, s: &ParticleSystem, rng: &mut Random) {
        let tol = |rng: &mut Random| rng.rand_float_tol();
        let life = tol(rng) * s.life[1] + s.life[0];
        // The spawn box: z, y, x drawn in that order.
        let rz = tol(rng);
        let ry = tol(rng);
        let rx = tol(rng);
        let half = 0.5;
        let pos = Vec3::new(
            s.spawn_box[0] * rx * s.spawn_size * half,
            s.spawn_box[1] * ry * s.spawn_size * half,
            s.spawn_box[2] * rz * s.spawn_size * half,
        );
        let color = |c: u32| {
            Vec3::new(
                ((c >> 16) & 0xff) as f32 / 255.0,
                ((c >> 8) & 0xff) as f32 / 255.0,
                (c & 0xff) as f32 / 255.0,
            )
        };
        let rotation = tol(rng) * s.rotation[1] + s.rotation[0];
        let size = tol(rng) * s.size[1] + s.size[0];
        let mut spin = tol(rng) * s.spin[1] + s.spin[0];
        if rng.rand() & 1 == 0 {
            spin = -spin;
        }
        let speed = tol(rng) * s.speed[1] + s.speed[0];
        let elevation = tol(rng) * s.cone[1] * DEG;
        let azimuth = tol(rng) * s.cone[0] * DEG;
        let (sa, ca) = azimuth.sin_cos();
        let (sb, cb) = elevation.sin_cos();
        // Turned to the emission direction (`Transform::FromDirection`; the instance has no
        // rotation, so its direction is the system's). Every shipped system emits along +Y,
        // which this leaves as is; a mod's may not.
        let cone = Vec3::new(-cb * sa * speed, cb * ca * speed, sb * speed);
        let velocity = rotate(from_direction(Vec3::from(s.direction)), cone);
        self.particles.push(Particle {
            life,
            age: 0.0,
            pos,
            color: color(s.colors[0]),
            alpha: 0.0,
            rotation,
            size,
            velocity,
            spin,
            growth: 0.0,
        });
    }

    /// One update by `dt` seconds.
    pub fn update(&mut self, dt: f32, rng: &mut Random) {
        match &*self.system.clone() {
            System::Standard(s) => self.update_standard(s, dt, rng),
            System::Tornado(t) => self.update_tornado(t, dt, rng),
        }
    }

    /// `FUN_1400d7940`: emit at `+0x5c` a second, then place every particle by its age `t`
    /// (of its life) on the funnel:
    /// - the wobble `W = (cos((t − 0.2)·2π)/2 + 1/2)(t + 1/2) − 0.33` of `+0x50` about the
    ///   instance's angle (which turns `+0x54`° a second);
    /// - the spiral `R = |(t − 0.4)³|·3.7 + 0.05 + 4·max(t − 0.3, 0)²` of `+0x48` at the
    ///   angle `+0x58·t/π` (radians);
    /// - up `+0x4c·t` plus `|rotation|/40`;
    ///
    /// spinning 90° a second, sized `R·+0x70` within `+0x68`..`+0x6c`, coloured and faded
    /// from the start colour to the end colour over its life.
    fn update_tornado(&mut self, s: &TornadoParticle, dt: f32, rng: &mut Random) {
        self.elapsed += dt;
        self.since_last += dt;
        let interval = 1.0 / s.rate;
        let rgba = |c: u32| {
            [
                ((c >> 16) & 0xff) as f32 / 255.0,
                ((c >> 8) & 0xff) as f32 / 255.0,
                (c & 0xff) as f32 / 255.0,
                (c >> 24) as f32 / 255.0,
            ]
        };
        let (c0, c1) = (rgba(s.colors[0]), rgba(s.colors[1]));
        while interval < self.since_last && self.particles.len() < s.max_particles as usize {
            self.since_last -= interval;
            let life = rng.rand_float_tol() * s.life[1] + s.life[0];
            let rotation = rng.rand_float_tol() * 180.0;
            self.particles.push(Particle {
                life,
                age: 0.0,
                pos: Vec3::ZERO,
                color: Vec3::new(c0[0], c0[1], c0[2]),
                alpha: 0.0,
                rotation,
                size: s.size[0],
                velocity: Vec3::ZERO,
                spin: 0.0,
                growth: 0.0,
            });
        }
        self.turn += dt * s.wobble_turn;
        let (wy, wx) = (self.turn * DEG).sin_cos();
        let (fade_in, fade_out) = (s.fade_in as f32 * 1e-6, s.fade_out as f32 * 1e-6);
        for p in self.particles.iter_mut().rev() {
            p.age += dt;
            let t = p.age / p.life;
            let wobble = ((((t - 0.2) * std::f32::consts::PI * 2.0).cos() * 0.5 + 0.5) * (t + 0.5)
                - 0.33)
                * s.wobble;
            let rise = (t - 0.3).max(0.0);
            let r = ((t - 0.4) * (t - 0.4) * (t - 0.4)).abs() * 3.7 + 0.05 + rise * rise * 4.0;
            let (sy, sx) = (s.spiral_turn * std::f32::consts::FRAC_1_PI * t).sin_cos();
            p.pos = Vec3::new(
                s.radius * sy * r + wy * wobble,
                s.radius * sx * r + wx * wobble,
                p.rotation.abs() / 40.0 + t * s.rise,
            );
            p.rotation += dt * 90.0;
            if 180.0 < p.rotation {
                p.rotation -= 360.0;
            }
            p.size = (r * s.size_scale).clamp(s.size[0], s.size[1]);
            p.color = Vec3::new(
                (c1[0] - c0[0]) * t + c0[0],
                (c1[1] - c0[1]) * t + c0[1],
                (c1[2] - c0[2]) * t + c0[2],
            );
            let mut fade = p.age / fade_in;
            if 1.0 < fade {
                fade = 1.0 - (fade_out - p.life + p.age) / fade_out;
            }
            p.alpha = fade.clamp(0.0, 1.0) * ((c1[3] - c0[3]) * t + c0[3]);
        }
        self.particles.retain(|p| p.age < p.life);
    }

    /// `ParticleSystemInstance::Update` by `dt` seconds.
    fn update_standard(&mut self, s: &ParticleSystem, dt: f32, rng: &mut Random) {
        let dt2 = dt * dt;
        self.elapsed += dt;
        self.since_last += dt;
        self.rate += dt2 * s.rate_change * s.rate_scale;
        let duration = s.duration as f32 * 1e-6;
        if self.rate < 0.0 || (0.0 < duration && duration < self.elapsed) {
            self.rate = 0.0;
        }
        if self.emitting && 0.0 < self.rate {
            let interval = 1.0 / self.rate;
            while interval < self.since_last && self.particles.len() < s.max_particles as usize {
                self.since_last -= interval;
                self.spawn(s, rng);
            }
        }
        let rgba = |c: u32| {
            [
                ((c >> 16) & 0xff) as f32 / 255.0,
                ((c >> 8) & 0xff) as f32 / 255.0,
                (c & 0xff) as f32 / 255.0,
                (c >> 24) as f32 / 255.0,
            ]
        };
        let (c0, c1) = (rgba(s.colors[0]), rgba(s.colors[1]));
        let force = Vec3::from(s.force) * s.force_amount * s.force_scale * dt2;
        let span = 1.0 / (s.color_span[1] - s.color_span[0]);
        let (fade_in, fade_out) = (s.fade_in as f32 * 1e-6, s.fade_out as f32 * 1e-6);
        for p in &mut self.particles {
            p.age += dt;
            p.velocity = (force + p.velocity) * s.damping;
            p.pos += p.velocity * dt;
            p.spin = (dt2 * s.spin_change + p.spin) * s.spin_damping;
            p.rotation += p.spin * dt;
            p.growth = (dt2 * s.growth + p.growth) * s.growth_damping;
            p.size += p.growth * dt;
            let t = ((p.age / p.life - s.color_span[0]) * span).clamp(0.0, 1.0);
            p.color = Vec3::new(
                (c1[0] - c0[0]) * t + c0[0],
                (c1[1] - c0[1]) * t + c0[1],
                (c1[2] - c0[2]) * t + c0[2],
            );
            let mut fade = p.age / fade_in;
            if 1.0 < fade {
                fade = 1.0 - (fade_out - p.life + p.age) / fade_out;
            }
            p.alpha = ((c1[3] - c0[3]) * t + c0[3]) * fade.clamp(0.0, 1.0);
        }
        self.particles.retain(|p| p.age < p.life);
    }

    /// `ParticleInstance::Draw`: newest first, a quad each.
    fn vertices(&self, out: &mut Vec<f32>) {
        for p in self.particles.iter().rev() {
            let pos = self.position + p.pos;
            let alpha = p.alpha * self.alpha;
            for (uv, corner) in [
                ([0.0, 0.0], [-1.0, 1.0]),
                ([1.0, 0.0], [1.0, 1.0]),
                ([1.0, 1.0], [1.0, -1.0]),
                ([0.0, 1.0], [-1.0, -1.0]),
            ] {
                out.extend_from_slice(&[
                    pos.x,
                    pos.y,
                    pos.z,
                    p.color.x,
                    p.color.y,
                    p.color.z,
                    alpha,
                    uv[0],
                    uv[1],
                    p.size * corner[0],
                    p.size * corner[1],
                    p.rotation * DEG,
                ]);
            }
        }
    }
}

/// One draw: a material's particles (12 floats a vertex, 4 vertices a particle).
#[derive(Clone, Debug, Default)]
pub struct ParticleBatch {
    pub material: String,
    /// `ParticleWaterMaterial`: unlit quads lying in the ground plane.
    pub flat: bool,
    pub vertices: Vec<f32>,
}

/// A slot's running system and its fade (`+0x10` of the component's `0x18` records).
struct Slot {
    instance: Instance,
    /// The slot's fade time (s) and the alpha's rate now: `±1 / time`, 0 when settled.
    fade: f32,
    fade_rate: f32,
}

/// What a slot wants this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Want {
    On,
    Off,
}

/// The entities' particle components and the loaded systems.
#[derive(Default)]
pub struct Particles {
    systems: HashMap<String, Option<Arc<System>>>,
    /// Material references by system, and whether they are flat.
    materials: HashMap<String, (String, bool)>,
    descriptions: HashMap<String, Option<Arc<ParticleDescription>>>,
    /// Model nodes' bind positions by mesh, and points models' named points.
    nodes: HashMap<String, Option<Vec<(String, Vec3)>>>,
    points: HashMap<String, Option<Vec<(String, Vec3)>>>,
    slots: BTreeMap<(usize, usize), Slot>,
    rng: Option<Random>,
}

impl Particles {
    pub fn system(&mut self, archive: &Archive, reference: &str) -> Option<Arc<System>> {
        self.systems
            .entry(reference.to_string())
            .or_insert_with(|| {
                let data = archive.read_ref(&reference.replace('\\', "/")).ok()?;
                let s = match banished_formats::game::particle_system(&data) {
                    Ok(s) => System::Standard(s),
                    Err(e) => match banished_formats::game::tornado_particle(&data) {
                        Ok(t) => System::Tornado(t),
                        Err(_) => {
                            log::warn!("{reference}: {e}");
                            return None;
                        }
                    },
                };
                Some(Arc::new(s))
            })
            .clone()
    }

    /// A template's `Template\X.rsc:particle`.
    pub fn description(
        &mut self,
        archive: &Archive,
        template: &str,
    ) -> Option<Arc<ParticleDescription>> {
        self.descriptions
            .entry(template.to_string())
            .or_insert_with(|| {
                let reference = template.replace('\\', "/");
                let reference = reference.strip_suffix(":resource")?;
                let data = archive.read_ref(&format!("{reference}:particle")).ok()?;
                banished_formats::game::particle_description(&data)
                    .ok()
                    .map(Arc::new)
            })
            .clone()
    }

    fn material(&mut self, archive: &Archive, system: &System) -> Option<(String, bool)> {
        let reference = system.material()?.clone();
        if let Some(m) = self.materials.get(&reference) {
            return Some(m.clone());
        }
        let data = archive.read_ref(&reference.replace('\\', "/")).ok()?;
        let mi = banished_formats::types::material_instance(&data).ok()?;
        let flat = mi
            .material
            .as_deref()
            .is_some_and(|m| m.contains("ParticleWater"));
        self.materials
            .insert(reference.clone(), (reference.clone(), flat));
        Some((reference, flat))
    }

    /// Drive slot `(entity, slot)` (`system`, `position`, fade and prewarm seconds): `On`
    /// starts it unless it is running (`FUN_140053fc0`: a slot fading out starts afresh), `Off`
    /// fades it out (event 3, `FUN_1400539f0`).
    pub fn set(
        &mut self,
        key: (usize, usize),
        want: Want,
        system: Option<(Arc<System>, Vec3, f32, f32)>,
    ) {
        let rng = self.rng.get_or_insert_with(|| Random::new(0x5eed));
        let running = self.slots.get(&key).map(|s| 0.0 <= s.fade_rate);
        match (want, running) {
            (Want::On, Some(true)) | (Want::Off, None) => {}
            (Want::On, _) => {
                let Some((system, position, fade, prewarm)) = system else {
                    return;
                };
                // `FUN_1400533b0`: a new instance, run through the prewarm in the game
                // clock's steps (1/30 s), alpha from 0.
                let mut instance = Instance::new(system, position);
                let mut t = 0.0;
                while t < prewarm {
                    instance.update(1.0 / 30.0, rng);
                    t += 1.0 / 30.0;
                }
                if fade <= 0.0 {
                    instance.alpha = 1.0;
                }
                self.slots.insert(
                    key,
                    Slot {
                        instance,
                        fade,
                        fade_rate: if 0.0 < fade { 1.0 / fade } else { 0.0 },
                    },
                );
            }
            (Want::Off, Some(_)) => {
                let s = self.slots.get_mut(&key).unwrap();
                s.fade_rate = if 0.0 < s.fade {
                    -1.0 / s.fade
                } else {
                    -f32::INFINITY
                };
            }
        }
    }

    /// Drop every slot of entities not in `alive`.
    pub fn retain(&mut self, mut alive: impl FnMut(usize) -> bool) {
        self.slots.retain(|&(e, _), _| alive(e));
    }

    /// Slot `key`'s alpha (the geometry's `+0x7c`) while it runs.
    pub fn alpha(&self, key: (usize, usize)) -> Option<f32> {
        self.slots.get(&key).map(|s| s.instance.alpha)
    }

    pub fn is_running(&self, key: (usize, usize)) -> bool {
        self.slots.contains_key(&key)
    }

    /// `FUN_140053490` then each system's update.
    pub fn update(&mut self, dt: f32) {
        let rng = self.rng.get_or_insert_with(|| Random::new(0x5eed));
        self.slots.retain(|_, s| {
            if s.fade_rate != 0.0 {
                s.instance.alpha += dt * s.fade_rate;
                if 1.0 < s.instance.alpha {
                    s.instance.alpha = 1.0;
                    s.fade_rate = 0.0;
                } else if s.instance.alpha < 0.0 {
                    return false;
                }
            }
            if s.instance.alpha <= 0.0 && s.fade_rate <= 0.0 {
                return false;
            }
            s.instance.update(dt, rng);
            true
        });
    }

    /// This frame's draws, by material.
    pub fn batches(&mut self, archive: &Archive) -> Vec<ParticleBatch> {
        let mut out: Vec<ParticleBatch> = Vec::new();
        let systems: Vec<Arc<System>> = self
            .slots
            .values()
            .map(|s| s.instance.system.clone())
            .collect();
        let materials: Vec<Option<(String, bool)>> =
            systems.iter().map(|s| self.material(archive, s)).collect();
        for (slot, material) in self.slots.values().zip(materials) {
            let Some((material, flat)) = material else {
                continue;
            };
            let i = match out.iter().position(|b| b.material == material) {
                Some(i) => i,
                None => {
                    out.push(ParticleBatch {
                        material,
                        flat,
                        vertices: Vec::new(),
                    });
                    out.len() - 1
                }
            };
            slot.instance.vertices(&mut out[i].vertices);
        }
        out.retain(|b| !b.vertices.is_empty());
        out
    }
}

impl Particles {
    fn node(&mut self, archive: &Archive, mesh: &str, name: &str) -> Option<Vec3> {
        let nodes = self.nodes.entry(mesh.to_string()).or_insert_with(|| {
            let data = archive
                .read_ref(&crate::scene_assets::with_object(mesh))
                .ok()?;
            let m = banished_formats::types::graphics_mesh(&data).ok()?;
            let locals: Vec<_> = m
                .bones
                .iter()
                .map(banished_formats::skeleton::local_bind)
                .collect();
            let world = banished_formats::skeleton::world_transforms(&m.bones, &locals);
            Some(
                m.bones
                    .iter()
                    .zip(world)
                    .map(|(b, w)| (b.name.clone(), Vec3::from(w.translation)))
                    .collect(),
            )
        });
        nodes
            .as_ref()?
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, p)| *p)
    }

    fn point(&mut self, archive: &Archive, list: &str, name: &str) -> Option<Vec3> {
        let points = self.points.entry(list.to_string()).or_insert_with(|| {
            let data = archive
                .read_ref(&crate::scene_assets::with_object(list))
                .ok()?;
            let l = banished_formats::game::point_list(&data).ok()?;
            Some(
                l.points
                    .into_iter()
                    .map(|p| (p.name, Vec3::from(p.position)))
                    .collect(),
            )
        });
        points
            .as_ref()?
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, p)| *p)
    }

    /// The entities' ParticleComponents against the town this frame:
    /// - a slot following entity events 2 and 3 (`+0x12`) runs while its building has
    ///   residents (`ResidenceComponent::AddResident`/`RemoveResident`) or workers of its own
    ///   profession (`FUN_140134ef0`/`FUN_140134cb0`);
    /// - the FireDescription's slot while the building burns (`FireComponent::Ignite`/`PutOut`);
    /// - a field's, orchard's or pasture's slot while it is blighted (DiseaseComponent).
    pub fn drive(
        &mut self,
        archive: &Archive,
        town: &crate::start::StartingTown,
        grid: &banished_game::terrain::HeightGrid,
    ) {
        let c = &town.construction;
        let mut seen = std::collections::BTreeSet::new();
        for (e, p) in town.placed.iter().enumerate() {
            if !p.alive {
                continue;
            }
            let template = &town.templates[p.template];
            let Some(desc) = self.description(archive, &template.reference) else {
                continue;
            };
            seen.insert(e);
            let site = c.sites.get(&e);
            let site_template = site.map(|s| &c.templates[s.template]);
            let fire_slot = site_template
                .and_then(|t| t.fire.as_ref())
                .map(|f| f.particle as usize);
            let occupied = c
                .homes
                .get(&e)
                .is_some_and(|h| h.families.iter().any(|f| !f.is_empty()))
                || c.workforce.workplaces.get(&e).is_some_and(|w| {
                    !w.workers.is_empty() && w.profession == w.description_profession
                });
            let burning = site
                .and_then(|s| s.fire.as_ref())
                .is_some_and(|f| 0 < f.burn);
            let sick = site
                .and_then(|s| s.disease.as_ref())
                .is_some_and(|b| 0 < b.left);
            // A tornado's system follows it (`FUN_140058630`) and fades out once it reaches
            // the end of its line (`FUN_140054080(0)`).
            let tornado = c.tornadoes.get(&e);
            for (k, slot) in desc.slots.iter().enumerate() {
                let key = (e, k);
                if let Some(t) = tornado {
                    if let Some(s) = self.slots.get_mut(&key) {
                        s.instance.position = Vec3::from(t.position);
                    }
                }
                let want = if let Some(t) = tornado.filter(|_| k == 0) {
                    if t.emitting {
                        Want::On
                    } else {
                        Want::Off
                    }
                } else if slot.flags[1] {
                    Want::On
                } else if slot.flags[2] {
                    if occupied {
                        Want::On
                    } else {
                        Want::Off
                    }
                } else if fire_slot == Some(k) {
                    if burning {
                        Want::On
                    } else {
                        Want::Off
                    }
                } else if sick {
                    Want::On
                } else {
                    Want::Off
                };
                if want == Want::Off || self.is_running(key) {
                    self.set(key, want, None);
                    continue;
                }
                let Some(system) = slot.system.as_deref().and_then(|r| self.system(archive, r))
                else {
                    continue;
                };
                // Where: the map rectangle's middle on the ground (flag 3), the points
                // model's point, the model node, else the model's root.
                let matrix = glam::Mat4::from_cols_array(&crate::models::placement_matrix(
                    p.position,
                    [p.scale[0], p.scale[1], p.scale[2]],
                    p.yaw,
                ));
                let local = if slot.flags[3] || template.model.is_none() {
                    None
                } else if let Some(list) = &desc.extra {
                    self.point(archive, list, &slot.node)
                } else {
                    let mesh = template
                        .model
                        .as_ref()
                        .and_then(|m| m.variants.get(p.variant))
                        .and_then(|v| v.meshes.get(p.mesh))
                        .cloned()
                        .flatten();
                    mesh.and_then(|m| self.node(archive, &m, &slot.node))
                };
                let position = if let Some(t) = tornado {
                    Vec3::from(t.position)
                } else if slot.flags[3] || template.model.is_none() {
                    let n = town.cells.size as i32;
                    let (x, y) = p.cell;
                    let (w, h) = p.footprint;
                    let cell = (y + (h >> 1)) * n + (w >> 1) + x;
                    Vec3::from(banished_game::town::cell_ground(
                        town.cells.size,
                        grid,
                        cell,
                    ))
                } else {
                    matrix.transform_point3(local.unwrap_or(Vec3::ZERO))
                };
                self.set(
                    key,
                    Want::On,
                    Some((
                        system,
                        position,
                        slot.fade as f32 * 1e-6,
                        slot.prewarm as f32 * 1e-6,
                    )),
                );
            }
        }
        self.retain(|e| seen.contains(&e));
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_emission_direction_turns_the_cone() {
        use glam::Vec3;
        let v = Vec3::new(0.3, 2.0, -0.4);
        // +Y leaves the velocity as it is.
        let same = super::rotate(super::from_direction(Vec3::Y), v);
        assert!((same - v).length() < 1e-6);
        // +Z turns the cone's axis (+Y) up.
        let up = super::rotate(super::from_direction(Vec3::Z), Vec3::Y);
        assert!((up - Vec3::Z).length() < 1e-6, "{up}");
        let side = super::rotate(super::from_direction(Vec3::X), Vec3::Y);
        assert!((side - Vec3::X).length() < 1e-6, "{side}");
    }

    use super::*;

    fn smoke() -> ParticleSystem {
        ParticleSystem {
            instanced: true,
            instances: 3,
            max_particles: 32,
            material: None,
            rate: 5.0,
            rate_change: 0.0,
            rate_scale: 1.0,
            duration: 0,
            direction: [0.0, 1.0, 0.0],
            spawn_size: 0.0,
            spawn_box: [1.0; 3],
            cone: [180.0, 20.0],
            speed: [0.5, 0.0],
            force_scale: 1.0,
            damping: 1.0,
            rotation: [0.0, 180.0],
            spin: [30.0, 5.0],
            spin_change: 0.0,
            spin_damping: 1.0,
            life: [4.5, 1.0],
            force_amount: 1.0,
            force: [15.0, 4.0, 10.0],
            size: [0.15, 0.05],
            growth: 9.0,
            growth_damping: 1.0,
            colors: [0xc0c0_c0c0, 0xc0ff_ffff],
            color_span: [0.0, 1.0],
            fade_in: 100_000,
            fade_out: 3_400_000,
        }
    }

    #[test]
    fn chimney_smoke_rises_grows_and_lives_out() {
        let mut rng = Random::new(1);
        let mut i = Instance::new(Arc::new(System::Standard(smoke())), Vec3::ZERO);
        i.alpha = 1.0;
        for _ in 0..90 {
            i.update(1.0 / 30.0, &mut rng);
        }
        // Five a second for three seconds, none dead yet (lives from 3.5 s).
        assert!(
            (14..=16).contains(&i.particles.len()),
            "{}",
            i.particles.len()
        );
        let oldest = i.particles[0];
        assert!(0.0 < oldest.pos.z && 0.0 < oldest.pos.x, "{:?}", oldest.pos);
        assert!(0.5 < oldest.size, "{}", oldest.size);
        i.emitting = false;
        for _ in 0..200 {
            i.update(1.0 / 30.0, &mut rng);
        }
        assert!(i.is_empty());
    }

    #[test]
    fn the_tornado_funnel_rises_and_widens() {
        let t = TornadoParticle {
            max_particles: 750,
            material: None,
            radius: 20.0,
            rise: 75.0,
            wobble: 26.0,
            wobble_turn: 30.0,
            spiral_turn: 400.0,
            rate: 20.0,
            life: [30.0, 2.0],
            size: [2.5, 15.0],
            size_scale: 30.0,
            colors: [0xffc0_c0c0, 0x8090_9090],
            fade_in: 50_000,
            fade_out: 1_000_000,
        };
        let mut rng = Random::new(1);
        let mut i = Instance::new(Arc::new(System::Tornado(t)), Vec3::ZERO);
        // The slot's prewarm.
        for _ in 0..32 * 30 {
            i.update(1.0 / 30.0, &mut rng);
        }
        // Twenty a second living 28 to 32 s.
        assert!(
            (560..=640).contains(&i.particles.len()),
            "{}",
            i.particles.len()
        );
        let oldest = i.particles[0];
        let newest = *i.particles.last().unwrap();
        assert!(newest.pos.z < oldest.pos.z && oldest.pos.z < 80.0);
        assert!(newest.size < oldest.size && oldest.size <= 15.0 && 2.5 <= newest.size);
        assert!(oldest.alpha < newest.alpha);
        assert!(i
            .particles
            .iter()
            .all(|p| (-180.0..=180.0).contains(&p.rotation)));
    }

    #[test]
    fn slots_fade_in_and_out() {
        let mut p = Particles::default();
        let sys = Some((Arc::new(System::Standard(smoke())), Vec3::ZERO, 1.0, 0.0));
        p.set((1, 0), Want::On, sys.clone());
        p.update(0.5);
        assert!(p.is_running((1, 0)));
        p.update(0.6);
        assert_eq!(p.slots[&(1, 0)].instance.alpha, 1.0);
        // Asking again changes nothing.
        p.set((1, 0), Want::On, sys);
        assert_eq!(p.slots[&(1, 0)].instance.alpha, 1.0);
        p.set((1, 0), Want::Off, None);
        p.update(0.5);
        assert!(p.is_running((1, 0)));
        p.update(0.6);
        assert!(!p.is_running((1, 0)));
    }
}
