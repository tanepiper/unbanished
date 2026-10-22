//! One ambient sound emitter and its slots: starting, moving, fading and stopping them.

use super::*;

/// An AmbientEmitterComponent slot (`+0x38`, 0x10 bytes): 3D handle, volume, effect.
#[derive(Clone)]
pub(super) struct Slot {
    pub(super) handle: u32,
    pub(super) volume: f32,
    pub(super) effect: Option<Rc<Effect>>,
}

pub(super) struct Emitter {
    pub(super) position: Vec3,
    pub(super) slots: Vec<Slot>,
    /// `AmbientEmitterDescription +0x20`: activation starts the slots.
    pub(super) autostart: bool,
}

/// The `{file}:ambientemitter` AmbientEmitterDescription as an unregistered emitter at the
/// origin (the component's position `+0x28` starts at zero).
pub(super) fn load_emitter(audio: &mut Audio, archive: &Archive, file: &str) -> Option<Emitter> {
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
pub(super) fn empty_emitter() -> Emitter {
    Emitter {
        position: Vec3::ZERO,
        slots: Vec::new(),
        autostart: false,
    }
}

impl Emitter {
    /// `FUN_1400435c0`: (re)start a slot on the effect's shared 3D effect.
    pub(super) fn start_slot(&mut self, audio: &mut Audio, i: usize) {
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
    pub(super) fn activate(&mut self, audio: &mut Audio) {
        if !self.autostart {
            return;
        }
        for i in 0..self.slots.len() {
            self.start_slot(audio, i);
        }
    }

    /// `FUN_1400432c0`: move every started slot's emitter.
    pub(super) fn move_to(&mut self, audio: &mut Audio, to: Vec3) {
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
    pub(super) fn deactivate(&mut self, audio: &mut Audio) {
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
    pub(super) fn set_volume(&mut self, audio: &mut Audio, i: usize, volume: f32) {
        if let Some(effect) = self.slots.get(i).map(|s| s.effect.clone()) {
            self.set(audio, i, effect, volume);
        }
    }

    /// `FUN_140043370`: give slot `i` an effect and a volume.
    pub(super) fn set(
        &mut self,
        audio: &mut Audio,
        i: usize,
        effect: Option<Rc<Effect>>,
        volume: f32,
    ) {
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
