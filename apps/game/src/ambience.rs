//! Map ambience placed by `FUN_1400a57c0` (see docs/STARTING-AUDIO-EMITTERS.md, docs/AUDIO.md):
//!
//! - one `Audio/Effects/Environment.rsc:ambient` per `int_150`-cell square region. Its
//!   AmbientEnvironmentComponent (`FUN_140043c70`) weighs the region's cells against the
//!   description's terrain classes (cell-type mask, optional entity template, weight), picks the
//!   heaviest, and feeds its warm and cold effects to the two emitter slots at `t` and `1 - t`,
//!   where `t` is the temperature's place in the description's fade range;
//! - one `Audio/Effects/WindEffect.rsc:ambient` at height `float_170`.
//!
//! Emitters of the same SoundEffect share one 3D effect (`FindEffect3D`, `AddEmitter`), which
//! plays at the loudest emitter in reach.
//!
//! Deviations: the original registers these as entities in the entity manager (bucket 2 for
//! the environment); the port keeps them as an audio-only layer and re-weighs a region when the
//! map changes instead of on its scheduled visit. The river emitters (every `float_130` along
//! the river paths) wait for the in-game water geometry (`Game +0x110`).

use std::rc::Rc;

use banished_audio::Effect;
use banished_formats::audio::{
    ambient_emitter_description, ambient_environment_description, AmbientEnvironmentDescription,
};
use banished_formats::game::map_generator;
use banished_formats::pkg::Archive;
use glam::Vec3;

use crate::sound::{sound_type, Audio};
use crate::start::StartingTown;

const ENVIRONMENT: &str = "Audio/Effects/Environment.rsc";
const WIND: &str = "Audio/Effects/WindEffect.rsc";

/// An AmbientEmitterComponent slot (`+0x38`, 0x10 bytes): 3D handle, volume, effect.
#[derive(Clone)]
struct Slot {
    handle: u32,
    volume: f32,
    effect: Option<Rc<Effect>>,
}

struct Emitter {
    position: Vec3,
    slots: Vec<Slot>,
}

impl Emitter {
    /// `FUN_1400435c0`: (re)start a slot on the effect's shared 3D effect.
    fn start_slot(&mut self, audio: &mut Audio, i: usize) {
        let Some(effect) = self.slots[i].effect.clone() else {
            return;
        };
        let mut handle = audio.scene.find_effect_3d(&effect);
        if handle == 0 {
            handle = audio
                .scene
                .play_effect_3d(&effect, sound_type::AMBIENT, 0, Vec3::ZERO);
        }
        self.slots[i].handle = handle;
        audio
            .scene
            .add_emitter(handle, self.position, self.slots[i].volume);
    }

    /// Activation (`FUN_140043770`): every slot.
    fn activate(&mut self, audio: &mut Audio) {
        for i in 0..self.slots.len() {
            self.start_slot(audio, i);
        }
    }

    /// `FUN_140043370`: give slot `i` an effect and a volume.
    fn set(&mut self, audio: &mut Audio, i: usize, effect: Option<Rc<Effect>>, volume: f32) {
        let same = match (&self.slots[i].effect, &effect) {
            (Some(a), Some(b)) => Rc::ptr_eq(a, b),
            (None, None) => true,
            _ => false,
        };
        let handle = self.slots[i].handle;
        if handle == 0 {
            self.slots[i].effect = effect;
            self.slots[i].volume = volume;
        } else if !same {
            audio.scene.remove_emitter(handle, self.position);
            if audio.scene.emitter_count(handle) == 0 {
                audio.scene.stop_effect_3d(&mut audio.device, handle);
            }
            self.slots[i] = Slot {
                handle: 0,
                volume,
                effect,
            };
            self.start_slot(audio, i);
        } else if self.slots[i].volume != volume {
            self.slots[i].volume = volume;
            audio
                .scene
                .set_emitter_volume(handle, self.position, volume);
        }
    }
}

struct Region {
    /// Cells `x..x+w`, `y..y+h` (`+0x28..+0x34`).
    rect: (i32, i32, i32, i32),
    /// The heaviest terrain class.
    class: usize,
    emitter: Emitter,
}

pub struct Ambience {
    desc: Option<AmbientEnvironmentDescription>,
    regions: Vec<Region>,
    wind: Option<Emitter>,
    /// Loaded effects per terrain class: (warm, cold).
    effects: Vec<(Option<Rc<Effect>>, Option<Rc<Effect>>)>,
    fade_t: f32,
}

fn load_emitter(audio: &mut Audio, archive: &Archive, file: &str) -> Vec<Slot> {
    let d = archive
        .read_ref(&format!("{file}:ambientemitter"))
        .ok()
        .and_then(|b| ambient_emitter_description(&b).ok());
    d.map(|d| {
        d.effects
            .iter()
            .map(|e| Slot {
                handle: 0,
                volume: 1.0,
                effect: e
                    .as_deref()
                    .and_then(|r| audio.effect(archive, &r.replace('\\', "/"))),
            })
            .collect()
    })
    .unwrap_or_default()
}

impl Ambience {
    /// `FUN_1400a57c0`'s environment and wind emitters for the start condition's profile.
    pub fn create(
        audio: &mut Audio,
        archive: &Archive,
        profile_ref: &str,
        town: &StartingTown,
        temperature: f32,
    ) -> Ambience {
        let profile = archive
            .read_ref(&profile_ref.replace('\\', "/"))
            .ok()
            .and_then(|b| map_generator(&b).ok());
        let (step, wind_height) = profile
            .as_ref()
            .map_or((16, 30.0), |p| (p.int_150.max(1), p.float_170));
        let desc = archive
            .read_ref(&format!("{ENVIRONMENT}:ambientenvironment"))
            .ok()
            .and_then(|b| ambient_environment_description(&b).ok());
        let effects = desc.as_ref().map_or_else(Vec::new, |d| {
            d.terrains
                .iter()
                .map(|t| {
                    let load = |audio: &mut Audio, r: &Option<String>| {
                        r.as_deref()
                            .and_then(|r| audio.effect(archive, &r.replace('\\', "/")))
                    };
                    (load(audio, &t.warm), load(audio, &t.cold))
                })
                .collect()
        });
        let mut ambience = Ambience {
            desc,
            regions: Vec::new(),
            wind: None,
            effects,
            fade_t: f32::NAN,
        };
        let n = town.cells.size as i32;
        let slots = load_emitter(audio, archive, ENVIRONMENT);
        let half = (n >> 1) as f32 - 0.5;
        let mut y = 0;
        while y < n {
            let mut x = 0;
            while x < n {
                // `FUN_140044000`: the region's centre cell, at height 0.
                let c = (y + (step >> 1)) * n + x + (step >> 1);
                let position = Vec3::new((c % n) as f32 - half, (c / n) as f32 - half, 0.0);
                ambience.regions.push(Region {
                    rect: (x, y, step, step),
                    class: 0,
                    emitter: Emitter {
                        position,
                        slots: slots.clone(),
                    },
                });
                x += step;
            }
            y += step;
        }
        ambience.reweigh(town);
        ambience.fade(audio, temperature);
        for r in &mut ambience.regions {
            r.emitter.activate(audio);
        }
        let mut wind = Emitter {
            position: Vec3::new(0.0, 0.0, wind_height),
            slots: load_emitter(audio, archive, WIND),
        };
        wind.activate(audio);
        ambience.wind = Some(wind);
        ambience
    }

    #[cfg(test)]
    pub fn region_count(&self) -> usize {
        self.regions.len()
    }

    /// `FUN_140043c70`'s weighing: the class whose cells weigh most (the first on ties).
    pub fn reweigh(&mut self, town: &StartingTown) {
        let Some(desc) = &self.desc else { return };
        let n = town.cells.size as i32;
        for r in &mut self.regions {
            let mut weights = vec![0i32; desc.terrains.len()];
            let (x0, y0, w, h) = r.rect;
            for y in y0..(y0 + h).min(n) {
                for x in x0..(x0 + w).min(n) {
                    let i = (y * n + x) as usize;
                    let kind = town.cells.types[i].0 as u32;
                    let entity = town.cells.occupants[i].or(town.cells.objects[i]);
                    let template =
                        entity.map(|e| &town.templates[town.placed[e].template].reference);
                    for (k, t) in desc.terrains.iter().enumerate() {
                        if t.cell_mask & (1 << (kind & 31)) == 0 {
                            continue;
                        }
                        let matches = match (&t.entity, template) {
                            (None, _) | (_, None) => true,
                            (Some(want), Some(have)) => want
                                .replace('/', "\\")
                                .eq_ignore_ascii_case(&have.replace('/', "\\")),
                        };
                        if matches {
                            weights[k] += t.weight;
                            break;
                        }
                    }
                }
            }
            let mut best = 0;
            for k in 1..weights.len() {
                if weights[best] < weights[k] {
                    best = k;
                }
            }
            r.class = best;
        }
    }

    /// The temperature cross-fade: `t = clamp((temperature - lo) / (hi - lo))`, warm at `t`,
    /// cold at `1 - t`.
    pub fn fade(&mut self, audio: &mut Audio, temperature: f32) {
        let Some(desc) = &self.desc else { return };
        let (lo, hi) = desc.fade;
        let t = ((temperature - lo) / (hi - lo)).clamp(0.0, 1.0);
        self.fade_t = t;
        for r in &mut self.regions {
            let (warm, cold) = self.effects.get(r.class).cloned().unwrap_or((None, None));
            if r.emitter.slots.len() >= 2 {
                r.emitter.set(audio, 0, warm, t);
                r.emitter.set(audio, 1, cold, 1.0 - t);
            }
        }
    }
}
