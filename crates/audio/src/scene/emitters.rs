//! 3D effects and their emitters: playing, finding and stopping them, and the emitters' positions and volumes.

use super::*;

impl Scene {
    pub(super) fn effect3d_mut(&mut self, handle: u32) -> Option<&mut Effect3D> {
        self.effects3d
            .get_mut(index3d(handle))?
            .as_mut()
            .filter(|e| (e.handle ^ handle) & 0xffff_e000 == 0)
    }

    /// `PlayEffect3D` with `FindFree3DEffect`: refused when the same effect already plays within
    /// its radius and started less than the retrigger window ago.
    ///
    /// Original: Runtime `0x1800e1700`.
    pub fn play_effect_3d(&mut self, effect: &Rc<Effect>, kind: u32, start: i64, at: Vec3) -> u32 {
        let mut window = self.step;
        if effect.max_length() != 0 {
            let w = micros(effect.max_length() as f32 / 1e6 / RETRIGGER_DIVISOR);
            if (window as f32) < w as f32 {
                window = w;
            }
        }
        let radius = effect.desc.radius;
        let mut free = None;
        for (i, slot) in self.effects3d.iter().enumerate() {
            match slot {
                None => {
                    if free.is_none() {
                        free = Some(i);
                    }
                }
                Some(e) => {
                    if Rc::ptr_eq(&e.effect, effect)
                        && (e.position - at).length_squared() < radius * radius
                        && (e.elapsed - start).abs() < window
                    {
                        return 0;
                    }
                }
            }
        }
        let Some(i) = free else { return 0 };
        self.counter = self.counter.wrapping_add(1);
        let handle = ((i as u32) * 2) | 1 | self.counter << 13;
        self.effects3d[i] = Some(Effect3D {
            position: at,
            kind,
            effect: effect.clone(),
            elapsed: start,
            playing: 0,
            handle,
            volume: 1.0,
            emitters: Vec::new(),
        });
        handle
    }

    /// `FindEffect3D`: the 3D effect playing this SoundEffect, or 0.
    pub fn find_effect_3d(&self, effect: &Rc<Effect>) -> u32 {
        self.effects3d
            .iter()
            .flatten()
            .find(|e| Rc::ptr_eq(&e.effect, effect))
            .map_or(0, |e| e.handle)
    }

    /// `AddEmitter`.
    pub fn add_emitter(&mut self, handle: u32, at: Vec3, volume: f32) {
        if let Some(e) = self.effect3d_mut(handle) {
            e.emitters.push((at, volume));
        }
    }

    /// `RemoveEmitter`: the first emitter at that position.
    pub fn remove_emitter(&mut self, handle: u32, at: Vec3) {
        if let Some(e) = self.effect3d_mut(handle) {
            if let Some(i) = e.emitters.iter().position(|(p, _)| *p == at) {
                e.emitters.remove(i);
            }
        }
    }

    /// `MoveEmitter`.
    pub fn move_emitter(&mut self, handle: u32, from: Vec3, to: Vec3) {
        if let Some(e) = self.effect3d_mut(handle) {
            if let Some(m) = e.emitters.iter_mut().find(|(p, _)| *p == from) {
                m.0 = to;
            }
        }
    }

    /// `SetEmitterVolume`.
    pub fn set_emitter_volume(&mut self, handle: u32, at: Vec3, volume: f32) {
        if let Some(e) = self.effect3d_mut(handle) {
            if let Some(m) = e.emitters.iter_mut().find(|(p, _)| *p == at) {
                m.1 = volume;
            }
        }
    }

    /// `GetEmitterCount`.
    pub fn emitter_count(&self, handle: u32) -> usize {
        self.effects3d
            .get(index3d(handle))
            .and_then(Option::as_ref)
            .filter(|e| (e.handle ^ handle) & 0xffff_e000 == 0)
            .map_or(0, |e| e.emitters.len())
    }

    /// `StopEffect3D`.
    pub fn stop_effect_3d(&mut self, mixer: &mut dyn Mixer, handle: u32) {
        let Some(e) = self.effect3d_mut(handle) else {
            return;
        };
        let playing = e.playing;
        if playing != 0 {
            self.stop_effect(mixer, playing);
        }
        self.effects3d[index3d(handle)] = None;
    }
}
