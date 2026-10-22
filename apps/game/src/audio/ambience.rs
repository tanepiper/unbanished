//! Map ambience placed by `FUN_1400a57c0` (see docs/STARTING-AUDIO-EMITTERS.md, docs/AUDIO.md):
//!
//! - one `Audio/Effects/Environment.rsc:ambient` per `int_150`-cell square region. Its
//!   AmbientEnvironmentComponent (`FUN_140043c70`) weighs the region's cells against the
//!   description's terrain classes (cell-type mask, optional entity template, weight), picks the
//!   heaviest, and feeds its warm and cold effects to the two emitter slots at `t` and `1 - t`,
//!   where `t` is the temperature's place in the description's fade range;
//! - one `Audio/Effects/WindEffect.rsc:ambient` at height `float_170`.
//!
//! Also the Rain effect's emitter (`FUN_140099790`) and the emitters of placed entities whose
//! template has an `ambientemitter` component (fire, crowd, tornado).
//!
//! Emitters of the same SoundEffect share one 3D effect (`FindEffect3D`, `AddEmitter`), which
//! plays at the loudest emitter in reach.
//!
//! Deviations: the original registers these as entities in the entity manager (bucket 2 for
//! the environment); the port keeps them as an audio-only layer and re-weighs a region when the
//! map changes instead of on its scheduled visit.
//!
//! River emitters: one `Audio/Effects/RiverEffect.rsc:ambient` every `float_130` units along the
//! river paths (MapComponent `+0x1e0`, see `World::river_paths`).

mod emitter;
mod entities;
use emitter::{empty_emitter, load_emitter, Emitter};

use std::collections::HashMap;
use std::rc::Rc;

use glam::Vec3;
use unbanished_audio::Effect;
use unbanished_formats::audio::{
    ambient_emitter_description, ambient_environment_description, AmbientEnvironmentDescription,
};
use unbanished_formats::game::map_generator;
use unbanished_formats::pkg::Archive;

use crate::audio::sound::{sound_type, Audio};
pub use crate::start::save::view_state::{AmbienceSave, RegionSave};
use crate::start::StartingTown;

const ENVIRONMENT: &str = "Audio/Effects/Environment.rsc";
const WIND: &str = "Audio/Effects/WindEffect.rsc";
const RIVER: &str = "Audio/Effects/RiverEffect.rsc";

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
    /// `RiverEffect.rsc:ambient` emitters along the river paths.
    rivers: Vec<Emitter>,
    /// The precipitation slots' ambient emitters (Rain: `RainEffect.rsc:ambient`).
    weather: Vec<Option<Emitter>>,
    /// Each weather emitter's ambient file.
    weather_files: Vec<Option<String>>,
    /// Placed entities whose template has an `ambientemitter` component, by entity index:
    /// (template index, emitter).
    entities: HashMap<usize, (usize, Emitter)>,
    /// Per template index: its file's AmbientEmitterDescription, if any.
    entity_templates: HashMap<usize, Option<Emitter>>,
    /// Loaded effects per terrain class: (warm, cold).
    effects: Vec<(Option<Rc<Effect>>, Option<Rc<Effect>>)>,
    fade_t: f32,
}

/// How many environment regions `FUN_1400a57c0` makes on a map of `n` cells a side: squares
/// of the start condition's `int_150` cells.
pub fn region_count(archive: &Archive, profile_ref: &str, n: usize) -> usize {
    let step = archive
        .read_ref(&profile_ref.replace('\\', "/"))
        .ok()
        .and_then(|b| map_generator(&b).ok())
        .map_or(16, |p| p.int_150.max(1) as usize);
    let side = n.div_ceil(step);
    side * side
}

/// Every sound effect [`Ambience::create`] gives its emitters: the environment's terrain
/// effects, the river, environment and wind emitters, and the `ambientemitter` of each living
/// placed entity's template. The loading screen decodes them into the effect cache
/// (`GameplayLoop::load`), so the first gameplay frame does not. Nothing is played or started
/// there: `create` still starts every emitter at the same moment, with the same shared effects.
pub fn preload_effects(archive: &Archive, town: &StartingTown) -> Vec<String> {
    let mut effects = Vec::new();
    let desc = archive
        .read_ref(&format!("{ENVIRONMENT}:ambientenvironment"))
        .ok()
        .and_then(|b| ambient_environment_description(&b).ok());
    for t in desc.iter().flat_map(|d| &d.terrains) {
        for r in [&t.warm, &t.cold].into_iter().flatten() {
            effects.push(r.replace('\\', "/"));
        }
    }
    let mut emitter = |file: &str| {
        let d = archive
            .read_ref(&format!("{file}:ambientemitter"))
            .ok()
            .and_then(|b| ambient_emitter_description(&b).ok());
        for r in d.iter().flat_map(|d| &d.effects).flatten() {
            effects.push(r.replace('\\', "/"));
        }
    };
    for file in [RIVER, ENVIRONMENT, WIND] {
        emitter(file);
    }
    let mut seen = std::collections::HashSet::new();
    for p in town.placed.iter().filter(|p| p.alive) {
        if seen.insert(p.template) {
            let reference = town.templates[p.template].reference.replace('\\', "/");
            let file = reference.split(':').next().unwrap_or(&reference);
            emitter(file);
        }
    }
    effects
}

impl Ambience {
    /// `FUN_1400a57c0`'s environment and wind emitters for the start condition's profile.
    pub fn create(
        audio: &mut Audio,
        archive: &Archive,
        profile_ref: &str,
        town: &StartingTown,
        rivers: &[Vec<[f32; 2]>],
        temperature: f32,
    ) -> Ambience {
        let profile = archive
            .read_ref(&profile_ref.replace('\\', "/"))
            .ok()
            .and_then(|b| map_generator(&b).ok());
        let (step, wind_height, river_spacing) = profile.as_ref().map_or((16, 30.0, 8.0), |p| {
            (p.int_150.max(1), p.float_170, p.float_130)
        });
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
            rivers: Vec::new(),
            weather: Vec::new(),
            weather_files: Vec::new(),
            entities: HashMap::new(),
            entity_templates: HashMap::new(),
            effects,
            fade_t: f32::NAN,
        };
        // River emitters first: along each path, a RiverEffect emitter at the point where the
        // walked length first exceeds the spacing, at height 0; the length then restarts.
        if let Some(river) = load_emitter(audio, archive, RIVER) {
            for path in rivers {
                let mut walked = 0.0f32;
                for w in path.windows(2) {
                    let (a, b) = (glam::Vec2::from(w[0]), glam::Vec2::from(w[1]));
                    walked += (b - a).length();
                    if river_spacing < walked {
                        let mut e = Emitter {
                            position: Vec3::new(b.x, b.y, 0.0),
                            slots: river.slots.clone(),
                            autostart: river.autostart,
                        };
                        e.activate(audio);
                        ambience.rivers.push(e);
                        walked = 0.0;
                    }
                }
            }
        }
        let n = town.cells.size as i32;
        let slots = load_emitter(audio, archive, ENVIRONMENT).map_or_else(Vec::new, |e| e.slots);
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
                        autostart: true,
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
        let mut wind = load_emitter(audio, archive, WIND).unwrap_or_else(empty_emitter);
        wind.position = Vec3::new(0.0, 0.0, wind_height);
        wind.activate(audio);
        ambience.wind = Some(wind);
        ambience.sync_entities(audio, archive, town);
        ambience
    }

    /// A precipitation slot's ambient emitter, following the environment's effect
    /// (`FUN_140099790`, `FUN_1400999b0`). `ambient` is the slot's ambient entity
    /// (`EnvironmentDescription` weather list `+8`); `intensity` is the effect's `+0xc0` while
    /// it exists.
    ///
    /// - When the effect starts, an entity with the ambient's AmbientEmitterComponent is
    ///   created (`FUN_1400c8bf0`, `FUN_1400c9d70`; activation starts its slots), moved to the
    ///   origin (`FUN_1400432c0`) and its first slot's volume set to 0.
    /// - Every iteration the first slot's volume follows the intensity.
    /// - When the effect ends the entity is removed, which stops its slots (`FUN_1400436d0`).
    pub fn weather(
        &mut self,
        audio: &mut Audio,
        archive: &Archive,
        slot: usize,
        ambient: Option<&str>,
        intensity: Option<f32>,
    ) {
        if self.weather.len() <= slot {
            self.weather.resize_with(slot + 1, || None);
            self.weather_files.resize(slot + 1, None);
        }
        let file = ambient
            .map(|r| r.replace('\\', "/"))
            .map(|r| match r.rsplit_once(':') {
                Some((file, _)) => file.to_string(),
                None => r,
            });
        match (intensity, file) {
            (Some(volume), Some(file)) => {
                let emitter = self.weather[slot].get_or_insert_with(|| {
                    let mut e = load_emitter(audio, archive, &file).unwrap_or_else(empty_emitter);
                    e.activate(audio);
                    e.set_volume(audio, 0, 0.0);
                    e
                });
                emitter.set_volume(audio, 0, volume);
                self.weather_files[slot] = Some(file);
            }
            _ => {
                self.weather_files[slot] = None;
                if let Some(mut e) = self.weather[slot].take() {
                    e.deactivate(audio);
                }
            }
        }
    }

    #[cfg(test)]
    pub fn weather_active(&self, slot: usize) -> bool {
        self.weather.get(slot).is_some_and(|w| w.is_some())
    }

    /// The 3D effect handles of a weather slot's emitter.
    #[cfg(test)]
    pub fn weather_handles(&self, slot: usize) -> Vec<u32> {
        self.weather
            .get(slot)
            .and_then(Option::as_ref)
            .map(|e| e.slots.iter().map(|s| s.handle).collect())
            .unwrap_or_default()
    }

    #[cfg(test)]
    pub fn river_count(&self) -> usize {
        self.rivers.len()
    }

    #[cfg(test)]
    pub fn region_count(&self) -> usize {
        self.regions.len()
    }

    /// `FUN_140043c70`'s weighing: the class whose cells weigh most (the first on ties).
    pub fn reweigh(&mut self, town: &StartingTown) {
        let Some(desc) = &self.desc else { return };
        let n = town.cells.size as i32;
        // Whether a template's reference matches each terrain's entity, worked out once per
        // template rather than per cell (string copies per cell made this a top frame cost).
        let same = |want: &str, have: &str| {
            let norm = |c: u8| {
                if c == b'/' {
                    b'\\'
                } else {
                    c.to_ascii_lowercase()
                }
            };
            want.len() == have.len()
                && want
                    .bytes()
                    .zip(have.bytes())
                    .all(|(x, y)| norm(x) == norm(y))
        };
        let mut matches: Vec<Option<Vec<bool>>> = vec![None; town.templates.len()];
        for r in &mut self.regions {
            let mut weights = vec![0i32; desc.terrains.len()];
            let (x0, y0, w, h) = r.rect;
            for y in y0..(y0 + h).min(n) {
                for x in x0..(x0 + w).min(n) {
                    let i = (y * n + x) as usize;
                    let kind = town.cells.types[i].0 as u32;
                    let entity = town.cells.occupants[i].or(town.cells.objects[i]);
                    let row = match entity {
                        Some(e) => {
                            let t = town.placed[e].template;
                            if matches[t].is_none() {
                                let have = &town.templates[t].reference;
                                matches[t] = Some(
                                    desc.terrains
                                        .iter()
                                        .map(|d| d.entity.as_deref().is_none_or(|w| same(w, have)))
                                        .collect(),
                                );
                            }
                            matches[t].as_ref()
                        }
                        None => None,
                    };
                    for (k, t) in desc.terrains.iter().enumerate() {
                        if t.cell_mask & (1 << (kind & 31)) == 0 {
                            continue;
                        }
                        if row.as_ref().is_none_or(|m| m[k]) {
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
    /// What a new game's save records of the map ambience (`FUN_1400a57c0`'s entities): each
    /// region's cells and centre with its warm and cold effects and their volumes, the wind's
    /// position and the river emitters' positions.
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    pub fn saved(&self) -> AmbienceSave {
        let terrains = self.desc.as_ref().map(|d| &d.terrains);
        let regions = self
            .regions
            .iter()
            .map(|r| {
                let t = terrains.and_then(|t| t.get(r.class));
                let volume = |i: usize| r.emitter.slots.get(i).map_or(0.0, |s| s.volume);
                RegionSave {
                    rect: r.rect,
                    position: r.emitter.position.to_array(),
                    effects: [
                        (t.and_then(|t| t.warm.clone()), volume(0)),
                        (t.and_then(|t| t.cold.clone()), volume(1)),
                    ],
                }
            })
            .collect();
        AmbienceSave {
            regions,
            wind: self.wind.as_ref().map(|w| w.position.to_array()),
            rivers: self.rivers.iter().map(|r| r.position.to_array()).collect(),
            weather: self
                .weather
                .iter()
                .zip(&self.weather_files)
                .filter_map(|(e, f)| {
                    let (e, f) = (e.as_ref()?, f.as_ref()?);
                    let volumes = e.slots.iter().map(|s| s.volume).collect();
                    Some((f.replace('/', "\\"), e.position.to_array(), volumes))
                })
                .collect(),
        }
    }

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
