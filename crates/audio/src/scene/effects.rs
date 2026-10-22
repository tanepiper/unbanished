//! Sound effects and their cache: loading, preparing and adding effects.

use super::*;

/// A SoundEffect with its decoded buffers (per sound, per variant).
pub struct Effect {
    /// The effect's resource reference.
    pub reference: String,
    /// The effect: its timeline of sounds, variants, radius and length.
    pub desc: SoundEffect,
    /// The decoded sounds, for each sound of the timeline and each variant (`None` when one could
    /// not be decoded).
    pub buffers: Vec<Vec<Option<Sound>>>,
}

/// Loads each effect once.
#[derive(Default)]
pub struct EffectCache {
    effects: HashMap<String, Option<Rc<Effect>>>,
}

/// Effects read and decoded by [`EffectCache::prepare`], for [`EffectCache::add`].
pub struct Prepared {
    keys: Vec<String>,
    /// Per key: the description, or why it did not load (`None`: not in the packages).
    descs: Vec<Result<SoundEffect, Option<String>>>,
    /// Every variant's buffer reference, in order, and its decoded samples.
    buffers: Vec<Option<String>>,
    decoded: Vec<Option<Result<(crate::Pcm, bool), String>>>,
}

impl Effect {
    /// Max length in microseconds (0 = endless).
    pub(super) fn max_length(&self) -> i64 {
        self.desc.max_length
    }
}

impl EffectCache {
    /// An effect by its reference, loaded and decoded the first time it is asked for.
    pub fn get(
        &mut self,
        archive: &unbanished_formats::pkg::Archive,
        reference: &str,
    ) -> Option<Rc<Effect>> {
        let key = reference.replace('/', "\\");
        if !self.effects.contains_key(&key) {
            self.preload(archive, &[key.as_str()]);
        }
        self.effects[&key].clone()
    }

    /// Load every effect of `references` that is not loaded yet, as [`EffectCache::get`] would
    /// one by one.
    pub fn preload(&mut self, archive: &unbanished_formats::pkg::Archive, references: &[&str]) {
        let missing = self.missing(references);
        if !missing.is_empty() {
            self.add(Self::prepare(archive, &missing));
        }
    }

    /// The effects of `references` that are not loaded yet.
    pub fn missing<'a>(&self, references: &[&'a str]) -> Vec<&'a str> {
        references
            .iter()
            .copied()
            .filter(|r| !self.effects.contains_key(&r.replace('/', "\\")))
            .collect()
    }

    /// Read and decode effects without the cache, so that it can run on another thread while
    /// the caller loads something else. Each sound buffer decodes on its own (a pure function
    /// of its data), so all the buffers of all the effects decode in parallel (native).
    pub fn prepare(archive: &unbanished_formats::pkg::Archive, references: &[&str]) -> Prepared {
        let mut keys: Vec<String> = Vec::new();
        for r in references {
            let key = r.replace('/', "\\");
            if !keys.contains(&key) {
                keys.push(key);
            }
        }
        let descs: Vec<Result<SoundEffect, Option<String>>> = keys
            .iter()
            .map(|key| {
                let path = key.replace('\\', "/");
                let bytes = archive.read_ref(&path).map_err(|_| None)?;
                unbanished_formats::audio::sound_effect(&bytes).map_err(|e| Some(e.to_string()))
            })
            .collect();
        let buffers: Vec<Option<String>> = descs
            .iter()
            .flatten()
            .flat_map(|d| d.sounds.iter().flat_map(|s| &s.variants))
            .map(|v| Some(v.buffer.as_deref()?.replace('\\', "/")))
            .collect();
        let decoded = unbanished_formats::par::map(&buffers, |r| {
            r.as_deref().map(|r| crate::load_sound(archive, r))
        });
        Prepared {
            keys,
            descs,
            buffers,
            decoded,
        }
    }

    /// Put prepared effects in the cache (an effect already there is kept).
    pub fn add(&mut self, prepared: Prepared) {
        let Prepared {
            keys,
            descs,
            buffers,
            decoded,
        } = prepared;
        let mut decoded = buffers.iter().zip(decoded).map(|(r, d)| match d? {
            Ok((pcm, looping)) => Some(Sound::new(pcm, looping)),
            Err(e) => {
                log::warn!("{}: {e}", r.as_deref().unwrap_or_default());
                None
            }
        });
        for (key, desc) in keys.into_iter().zip(descs) {
            let desc = desc.map_err(|e| e.map(|e| log::warn!("{key}: {e}"))).ok();
            let loaded = desc.map(|desc| {
                let buffers = desc
                    .sounds
                    .iter()
                    .map(|s| decoded.by_ref().take(s.variants.len()).collect())
                    .collect();
                Rc::new(Effect {
                    reference: key.clone(),
                    desc,
                    buffers,
                })
            });
            self.effects.entry(key).or_insert(loaded);
        }
    }
}
