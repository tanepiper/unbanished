//! The sounds of entities: buildings, citizens and tornadoes.

use super::*;

impl Ambience {
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

    pub(super) fn set_entity_slot(
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
}
