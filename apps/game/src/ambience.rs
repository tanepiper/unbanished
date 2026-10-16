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

use std::collections::HashMap;
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
const RIVER: &str = "Audio/Effects/RiverEffect.rsc";

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
    /// `AmbientEmitterDescription +0x20`: activation starts the slots.
    autostart: bool,
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

    /// Activation (`FUN_140043500`): with autostart, every slot (`FUN_140043770`).
    fn activate(&mut self, audio: &mut Audio) {
        if !self.autostart {
            return;
        }
        for i in 0..self.slots.len() {
            self.start_slot(audio, i);
        }
    }

    /// `FUN_1400432c0`: move every started slot's emitter.
    fn move_to(&mut self, audio: &mut Audio, to: Vec3) {
        if to == self.position {
            return;
        }
        for s in &self.slots {
            if s.handle != 0 {
                audio.scene.move_emitter(s.handle, self.position, to);
            }
        }
        self.position = to;
    }

    /// Removal (`FUN_1400436d0`, slot `+0x38`): leave every slot's 3D effect and stop the
    /// effect when no emitter is left.
    fn deactivate(&mut self, audio: &mut Audio) {
        for slot in &mut self.slots {
            if slot.handle != 0 {
                audio.scene.remove_emitter(slot.handle, self.position);
                if audio.scene.emitter_count(slot.handle) == 0 {
                    audio.scene.stop_effect_3d(&mut audio.device, slot.handle);
                }
            }
            slot.handle = 0;
        }
    }

    /// `SetEmitterVolume` on slot `i` when its volume changes.
    fn set_volume(&mut self, audio: &mut Audio, i: usize, volume: f32) {
        if let Some(effect) = self.slots.get(i).map(|s| s.effect.clone()) {
            self.set(audio, i, effect, volume);
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
    /// `RiverEffect.rsc:ambient` emitters along the river paths.
    rivers: Vec<Emitter>,
    /// The precipitation slots' ambient emitters (Rain: `RainEffect.rsc:ambient`).
    weather: Vec<Option<Emitter>>,
    /// Placed entities whose template has an `ambientemitter` component, by entity index:
    /// (template index, emitter).
    entities: HashMap<usize, (usize, Emitter)>,
    /// Per template index: its file's AmbientEmitterDescription, if any.
    entity_templates: HashMap<usize, Option<Emitter>>,
    /// Loaded effects per terrain class: (warm, cold).
    effects: Vec<(Option<Rc<Effect>>, Option<Rc<Effect>>)>,
    fade_t: f32,
}

/// The `{file}:ambientemitter` AmbientEmitterDescription as an unregistered emitter at the
/// origin (the component's position `+0x28` starts at zero).
fn load_emitter(audio: &mut Audio, archive: &Archive, file: &str) -> Option<Emitter> {
    let d = archive
        .read_ref(&format!("{file}:ambientemitter"))
        .ok()
        .and_then(|b| ambient_emitter_description(&b).ok())?;
    let slots = d
        .effects
        .iter()
        .map(|e| Slot {
            handle: 0,
            volume: 1.0,
            effect: e
                .as_deref()
                .and_then(|r| audio.effect(archive, &r.replace('\\', "/"))),
        })
        .collect();
    Some(Emitter {
        position: Vec3::ZERO,
        slots,
        autostart: d.autostart,
    })
}

/// Unregistered emitter with no slots (a missing description).
fn empty_emitter() -> Emitter {
    Emitter {
        position: Vec3::ZERO,
        slots: Vec::new(),
        autostart: false,
    }
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
            }
            _ => {
                if let Some(mut e) = self.weather[slot].take() {
                    e.deactivate(audio);
                }
            }
        }
    }

    /// Register the emitters of placed entities whose template lists an `ambientemitter`
    /// component (buildings: FireEffect in slot 0, CrowdEffect in slot 1 for houses; the
    /// tornado), and remove those of dead entities. Activation (`FUN_140043500`) starts nothing
    /// for them: only the Tornado's description has autostart. Their slots are started by
    /// building state:
    ///
    /// - fire: `FUN_140049f70` (FireDescription `+0x2c` slot, at the model position) and
    ///   `FUN_140049e20` (put out: the slot's emitter removed);
    /// - crowd: `FUN_14012fbe0` (a residence's first resident) and `FUN_14012ee40` (residence
    ///   update while occupied) start slot 1 at volume 1 at the model position;
    ///   `FUN_14012fa20` (the last resident leaves) removes it.
    ///
    /// [`Ambience::drive_entities`] starts and stops those slots from the town's state.
    pub fn sync_entities(&mut self, audio: &mut Audio, archive: &Archive, town: &StartingTown) {
        for (i, p) in town.placed.iter().enumerate() {
            let registered = self.entities.get(&i).map(|(t, _)| *t);
            if registered.is_some() && (!p.alive || registered != Some(p.template)) {
                if let Some((_, mut e)) = self.entities.remove(&i) {
                    e.deactivate(audio);
                }
            }
            if !p.alive || self.entities.contains_key(&i) {
                continue;
            }
            let template = self.entity_templates.entry(p.template).or_insert_with(|| {
                let reference = town.templates[p.template].reference.replace('\\', "/");
                let file = reference.split(':').next().unwrap_or(&reference);
                load_emitter(audio, archive, file)
            });
            if let Some(t) = template {
                let mut e = Emitter {
                    position: t.position,
                    slots: t.slots.clone(),
                    autostart: t.autostart,
                };
                e.activate(audio);
                self.entities.insert(i, (p.template, e));
            }
        }
    }

    /// The tornadoes' emitters follow them (`FUN_140058630` → `FUN_1400432c0`), the first
    /// slot's volume following their particle system's alpha (`+0x7c`, `alpha`).
    pub fn follow_tornadoes(
        &mut self,
        audio: &mut Audio,
        town: &StartingTown,
        alpha: impl Fn(usize) -> Option<f32>,
    ) {
        for (&e, t) in &town.construction.tornadoes {
            if let Some((_, emitter)) = self.entities.get_mut(&e) {
                emitter.move_to(audio, Vec3::from(t.position));
                if let Some(a) = alpha(e) {
                    emitter.set_volume(audio, 0, a);
                }
            }
        }
    }

    /// The buildings' slots from the town each frame: a burning building's FireDescription
    /// `+0x2c` slot (`FireComponent::Ignite`, put out by `PutOut`), and an occupied residence's
    /// crowd slot 1 at volume 1 (`ResidenceComponent::AddResident` and its update, stopped
    /// when the last resident leaves), both at the model's position.
    pub fn drive_entities(&mut self, audio: &mut Audio, town: &StartingTown) {
        let c = &town.construction;
        let keys: Vec<usize> = self.entities.keys().copied().collect();
        for e in keys {
            let Some(p) = town.placed.get(e) else {
                continue;
            };
            let at = Vec3::from(p.position);
            let site = c.sites.get(&e);
            let fire_slot = site
                .and_then(|s| c.templates[s.template].fire.as_ref())
                .map(|f| f.emitter);
            if let Some(slot) = fire_slot.and_then(|s| usize::try_from(s).ok()) {
                let burning = site
                    .and_then(|s| s.fire.as_ref())
                    .is_some_and(|f| 0 < f.burn);
                self.set_entity_slot(audio, e, slot, burning, at, None);
            }
            if let Some(home) = c.homes.get(&e) {
                let occupied = home.families.iter().any(|f| !f.is_empty());
                self.set_entity_slot(audio, e, 1, occupied, at, Some(1.0));
            }
        }
    }

    fn set_entity_slot(
        &mut self,
        audio: &mut Audio,
        entity: usize,
        slot: usize,
        on: bool,
        position: Vec3,
        volume: Option<f32>,
    ) {
        let running = self
            .entities
            .get(&entity)
            .and_then(|(_, e)| e.slots.get(slot))
            .is_some_and(|s| s.handle != 0);
        if on && !running {
            self.start_entity_slot(audio, entity, slot, position, volume);
        } else if !on && running {
            self.stop_entity_slot(audio, entity, slot);
        }
    }

    /// Start an entity emitter's slot at `position` (`FUN_1400432c0`, `FUN_1400435c0`), and set
    /// its volume when given (the crowd sets 1).
    pub fn start_entity_slot(
        &mut self,
        audio: &mut Audio,
        entity: usize,
        slot: usize,
        position: Vec3,
        volume: Option<f32>,
    ) {
        let Some((_, e)) = self.entities.get_mut(&entity) else {
            return;
        };
        if slot >= e.slots.len() {
            return;
        }
        e.move_to(audio, position);
        e.start_slot(audio, slot);
        if let Some(v) = volume {
            e.set_volume(audio, slot, v);
        }
    }

    /// Remove an entity emitter's slot from its 3D effect, stopping the effect when no emitter
    /// is left (`FUN_140049e20`, `FUN_14012fa20`).
    pub fn stop_entity_slot(&mut self, audio: &mut Audio, entity: usize, slot: usize) {
        let Some((_, e)) = self.entities.get_mut(&entity) else {
            return;
        };
        let Some(s) = e.slots.get_mut(slot) else {
            return;
        };
        if s.handle != 0 {
            audio.scene.remove_emitter(s.handle, e.position);
            if audio.scene.emitter_count(s.handle) == 0 {
                audio.scene.stop_effect_3d(&mut audio.device, s.handle);
            }
        }
        s.handle = 0;
    }

    #[cfg(test)]
    pub fn entity_emitters(&self) -> usize {
        self.entities.len()
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
