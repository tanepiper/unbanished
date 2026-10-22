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

mod drive;
mod instance;
pub use instance::Instance;
#[cfg(test)]
use instance::{from_direction, rotate};

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use glam::Vec3;
use unbanished_formats::game::{ParticleDescription, ParticleSystem, TornadoParticle};
use unbanished_formats::pkg::Archive;
use unbanished_game::random::Random;

/// `DAT_1801647ec`.
const DEG: f32 = 0.017_453_292;

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
                let s = match unbanished_formats::game::particle_system(&data) {
                    Ok(s) => System::Standard(s),
                    Err(e) => match unbanished_formats::game::tornado_particle(&data) {
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
                unbanished_formats::game::particle_description(&data)
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
        let mi = unbanished_formats::types::material_instance(&data).ok()?;
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
                    // `FUN_1400532f0`: the slot's scene object goes.
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

#[cfg(test)]
mod tests;
