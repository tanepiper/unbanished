use super::*;
use unbanished_formats::audio::{EffectSound, SoundVariant};

#[derive(Default)]
struct Fake {
    next: u32,
    playing: Vec<(Channel, f32, f32)>,
    stopped: Vec<Channel>,
}

impl Mixer for Fake {
    fn play(&mut self, _: &Sound, p: PlayParams) -> Option<Channel> {
        self.next += 1;
        let c = Channel::test(self.next);
        self.playing.push((c, p.volume, p.pitch));
        Some(c)
    }
    fn stop(&mut self, c: Channel) {
        self.stopped.push(c);
    }
    fn set_volume(&mut self, _: Channel, _: f32) {}
}

fn effect(starts: &[f32], radius: f32, max_length: i64) -> Rc<Effect> {
    let pcm = crate::Pcm {
        channels: 1,
        sample_rate: 100,
        samples: vec![0.0; 10],
    };
    let sound = Sound::new(pcm, false);
    Rc::new(Effect {
        reference: "test".into(),
        desc: SoundEffect {
            radius,
            inner_radius: 1.0,
            sounds: starts
                .iter()
                .map(|&s| EffectSound {
                    start: s,
                    variants: vec![SoundVariant {
                        buffer: Some("x".into()),
                        pitch_min: -1.0,
                        pitch_max: 1.0,
                        volume: 0.5,
                    }],
                })
                .collect(),
            max_length,
        },
        buffers: starts.iter().map(|_| vec![Some(sound.clone())]).collect(),
    })
}

#[test]
fn timeline_starts_sounds_in_their_window_and_frees_when_ended() {
    let mut scene = Scene::default();
    let mut mixer = Fake::default();
    let fx = effect(&[0.0, 0.5], 10.0, 1_000_000);
    let h = scene.play_effect(&fx, sound_type::EFFECT, 0);
    let mut seq = 0u32;
    let mut rand = || {
        seq += 1;
        seq
    };
    scene.update(&mut mixer, 100_000, 0, &mut rand, Vec3::ZERO, 0.0);
    assert_eq!(mixer.playing.len(), 1);
    // Volume: variant 0.5 × effect 1 × effects 100%.
    assert_eq!(mixer.playing[0].1, 0.5);
    scene.update(&mut mixer, 500_000, 0, &mut rand, Vec3::ZERO, 0.0);
    assert_eq!(mixer.playing.len(), 2);
    let (a, b) = (mixer.playing[0].0, mixer.playing[1].0);
    scene.on_ended(a);
    scene.update(&mut mixer, 100_000, 0, &mut rand, Vec3::ZERO, 0.0);
    assert!(scene.is_effect_playing(h));
    scene.on_ended(b);
    scene.update(&mut mixer, 100_000, 0, &mut rand, Vec3::ZERO, 0.0);
    assert!(!scene.is_effect_playing(h));
}

#[test]
fn music_type_uses_its_volume_and_type_zero_is_silent() {
    let mut scene = Scene::default();
    let mut mixer = Fake::default();
    let fx = effect(&[0.0], 10.0, 0);
    scene.play_effect(&fx, sound_type::MUSIC, 0);
    scene.play_effect(&fx, 0, 0);
    scene.update(&mut mixer, 1, 0, &mut || 0, Vec3::ZERO, 0.0);
    assert_eq!(mixer.playing.len(), 1);
    assert_eq!(mixer.playing[0].1, 0.25);
}

#[test]
fn three_d_effect_follows_the_loudest_emitter() {
    let mut scene = Scene::default();
    let mut mixer = Fake::default();
    let fx = effect(&[0.0], 10.0, 0);
    let h = scene.play_effect_3d(&fx, sound_type::AMBIENT, 0, Vec3::ZERO);
    scene.add_emitter(h, Vec3::new(100.0, 0.0, 0.0), 1.0);
    // Out of reach: nothing plays.
    scene.update(&mut mixer, 1, 0, &mut || 0, Vec3::ZERO, 0.0);
    assert_eq!(scene.active(), (0, 1));
    // A second emitter at 5.5 (half-way from inner 1 to radius 10).
    scene.add_emitter(h, Vec3::new(5.5, 0.0, 0.0), 1.0);
    scene.update(&mut mixer, 1, 0, &mut || 0, Vec3::ZERO, 0.0);
    scene.update(&mut mixer, 1, 0, &mut || 0, Vec3::ZERO, 0.0);
    assert_eq!(scene.active(), (1, 1));
    assert!((mixer.playing[0].1 - 0.25).abs() < 1e-6);
    assert_eq!(scene.emitter_count(h), 2);
    assert_eq!(scene.find_effect_3d(&fx), h);
}

#[test]
fn pause_stops_channels_and_resume_continues() {
    let mut scene = Scene::default();
    let mut mixer = Fake::default();
    let fx = effect(&[0.0], 10.0, 0);
    scene.play_effect(&fx, sound_type::MUSIC, 0);
    scene.update(&mut mixer, 1, 0, &mut || 0, Vec3::ZERO, 0.0);
    scene.pause(&mut mixer, sound_type::MUSIC, true);
    assert_eq!(mixer.stopped.len(), 1);
    scene.pause(&mut mixer, sound_type::MUSIC, false);
    scene.update(&mut mixer, 1, 0, &mut || 0, Vec3::ZERO, 0.0);
    // Unpausing reopens the window at 0: the paused sound starts again (resumed at the
    // effect's elapsed time).
    assert_eq!(mixer.playing.len(), 2);
    scene.update(&mut mixer, 1, 0, &mut || 0, Vec3::ZERO, 0.0);
    assert_eq!(mixer.playing.len(), 2);
}
