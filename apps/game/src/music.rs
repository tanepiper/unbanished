//! The game's music player (`Game +0x80`, `FUN_1400a95b0`, run once per frame): when the
//! current track has ended, play a track from `Game/MusicTracks.rsc` whose month range holds
//! the current month and whose mood range holds the town's mood, avoiding the last one.

use banished_formats::audio::{music_list, MusicTrack};
use banished_formats::pkg::Archive;

use crate::sound::{sound_type, Audio};

const TRACKS: &str = "Game/MusicTracks.rsc:resource";

#[derive(Default)]
pub struct MusicPlayer {
    /// `+0x0`: the playing effect (initially -1, never playing).
    handle: u32,
    /// `+0x10`: the last track's effect.
    last: Option<String>,
    tracks: Vec<MusicTrack>,
}

/// The mood (`FUN_1400a95b0`): `(mean health / max + mean happiness / max) / 2`, 0 without
/// citizens.
pub fn mood(health: f32, health_max: f32, happiness: f32, happiness_max: f32, people: u32) -> f32 {
    if people == 0 {
        return 0.0;
    }
    (health / health_max + happiness / happiness_max) * 0.5
}

/// The candidates' indices, then the choice: one candidate plays; with more, `Rand(n)`, moved
/// one on when it would repeat the last track, and wrapped to the first.
pub fn choose(
    tracks: &[MusicTrack],
    month: i32,
    mood: f32,
    last: Option<&str>,
    rand: impl FnOnce(u32) -> u32,
) -> Option<usize> {
    let candidates: Vec<usize> = (0..tracks.len())
        .filter(|&i| {
            let t = &tracks[i];
            t.months.0 <= month && month <= t.months.1 && t.mood.0 <= mood && mood <= t.mood.1
        })
        .collect();
    let n = candidates.len() as u32;
    let mut i = match n {
        0 => return None,
        1 => 0,
        _ => rand(n),
    };
    if n > 1 && tracks[candidates[i as usize]].effect.as_deref() == last {
        i += 1;
    }
    if i >= n {
        i = 0;
    }
    Some(candidates[i as usize])
}

impl MusicPlayer {
    pub fn load(archive: &Archive) -> MusicPlayer {
        let tracks = archive
            .read_ref(TRACKS)
            .ok()
            .and_then(|d| music_list(&d).map_err(|e| log::warn!("{TRACKS}: {e}")).ok())
            .unwrap_or_default();
        MusicPlayer {
            handle: 0,
            last: None,
            tracks,
        }
    }

    /// Decode, into the effect cache, the track [`MusicPlayer::update`] would start now,
    /// without drawing from the audio stream or playing anything. Called while the loading
    /// screen shows, so that the first gameplay frame does not decode the first track. If the
    /// month or the mood changes before that frame, `update` still makes its own choice.
    pub fn preload(&self, audio: &mut Audio, archive: &Archive, month: i32, mood: f32) {
        if audio.scene.is_effect_playing(self.handle) {
            return;
        }
        let month = if month > 11 { 0 } else { month };
        let peek = |n| audio.peek_rand_below(n);
        let Some(i) = choose(&self.tracks, month, mood, self.last.as_deref(), peek) else {
            return;
        };
        if let Some(effect) = &self.tracks[i].effect {
            audio.effect(archive, &effect.replace('\\', "/"));
        }
    }

    /// `FUN_1400a95b0`. `month` is `(int)(seconds / 3600 × 12)`, 0 past 11.
    pub fn update(&mut self, audio: &mut Audio, archive: &Archive, month: i32, mood: f32) {
        if audio.scene.is_effect_playing(self.handle) {
            return;
        }
        let month = if month > 11 { 0 } else { month };
        let Some(i) = choose(&self.tracks, month, mood, self.last.as_deref(), |n| {
            audio.rand_below(n)
        }) else {
            return;
        };
        let Some(effect) = self.tracks[i].effect.clone() else {
            return;
        };
        self.handle = audio.play(archive, &effect.replace('\\', "/"), sound_type::MUSIC);
        self.last = Some(effect);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(name: &str, mood: (f32, f32), months: (i32, i32)) -> MusicTrack {
        MusicTrack {
            effect: Some(name.into()),
            mood,
            months,
        }
    }

    #[test]
    fn filters_by_month_and_mood_and_avoids_repeats() {
        let tracks = [
            track("a", (0.0, 1.01), (0, 11)),
            track("winter", (0.0, 1.01), (9, 11)),
            track("sad", (0.0, 0.3), (0, 11)),
        ];
        // Spring, content town: only "a".
        assert_eq!(choose(&tracks, 2, 0.8, None, |_| unreachable!()), Some(0));
        // Winter: "a" or "winter"; a repeat of "a" moves on.
        assert_eq!(choose(&tracks, 10, 0.8, Some("a"), |_| 0), Some(1));
        // A repeat of the last candidate wraps to the first.
        assert_eq!(choose(&tracks, 10, 0.8, Some("winter"), |_| 1), Some(0));
        assert_eq!(choose(&tracks, 2, 1.5, None, |_| 0), None);
    }
}
