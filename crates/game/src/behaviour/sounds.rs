//! The sound events a character's animation fires (`AnimAudioEvent`): a hammer on an anvil, an
//! axe, a chicken. Each one fired draws from the game's random numbers, so they are part of the
//! simulation even though they only make a sound. See docs/AUDIO.md.

use crate::random::Random;

// Runtime MeshInstance::Update (0x180103090) calls AnimationState::Update (0x1801003b0) with its
// last argument true, so the first layer's events fire on every update (ApplyEvents,
// 0x1800fd230). The event's slot +0x40 is FUN_14008d310: probability +0x28, effect +0x20, at the
// entity's transform (+0x10 is its position).
/// The sound events the characters' clips can fire, and the sounds that passed their draw.
#[derive(Debug, Clone, Default)]
pub struct AnimationSounds {
    /// Each `AnimAudioEvent` the clips name: the event resource, its SoundEffect and its
    /// probability.
    pub events: Vec<(String, Option<String>, f32)>,
    /// The sounds to play since the caller last took them: position and SoundEffect.
    pub fired: Vec<([f32; 3], String)>,
    /// How many templates [`AnimationSounds::learn`] has read.
    templates_seen: usize,
}

impl AnimationSounds {
    /// Take in the sound events of templates loaded since the last call (the animals' clips name
    /// theirs through their templates).
    pub fn learn(&mut self, templates: &[crate::nature::seeding::Template]) {
        for t in templates.iter().skip(self.templates_seen) {
            for e in &t.audio_events {
                if !self.events.iter().any(|(r, _, _)| *r == e.0) {
                    self.events.push(e.clone());
                }
            }
        }
        self.templates_seen = templates.len();
    }

    /// For each event a character's clip fired, in order: one `RandFloat` from the game's
    /// generator against the event's probability; when it passes, its sound plays at the
    /// character. An event that is not a sound draws nothing.
    ///
    /// Deviation: the original updates every animated mesh in one pass at the start of the
    /// iteration, in the animation manager's order, so these draws come before any entity's
    /// update. The port draws for each citizen as its clip advances, and for each animal just
    /// before that animal's own update; the trees' draws come with the natural resources.
    ///
    /// Original: `FUN_14008d310`.
    pub fn fire(&mut self, resources: &[String], position: [f32; 3], rng: &mut Random) {
        for r in resources {
            let Some((_, effect, probability)) = self.events.iter().find(|(e, _, _)| e == r) else {
                continue;
            };
            if rng.rand_float() <= *probability {
                if let Some(effect) = effect {
                    self.fired.push((position, effect.clone()));
                }
            }
        }
    }
}
