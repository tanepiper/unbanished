//! Growth: the iteration over scheduled entities, a scheduled growth step, smoothing and animation.

use super::*;

impl Seeder<'_> {
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

    /// GrowthComponent's scheduled update for one visited entity; true when the models changed.
    pub(super) fn scheduled_growth(
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
    pub(super) fn animate(&mut self, dt: i64) -> bool {
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
}
