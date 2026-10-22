use super::*;

fn tone(frames: usize, channels: u16) -> Sound {
    Sound::new(
        Pcm {
            channels,
            sample_rate: 100,
            samples: (0..frames * channels as usize).map(|i| i as f32).collect(),
        },
        false,
    )
}

#[test]
fn first_free_channel_and_stale_handles() {
    let mut s = Slots::default();
    let a = s.insert('a').unwrap();
    let b = s.insert('b').unwrap();
    assert_eq!((a.index(), b.index()), (0, 1));
    assert_eq!(s.remove(a), Some('a'));
    // The freed channel is reused first; the old handle no longer reaches it.
    let c = s.insert('c').unwrap();
    assert_eq!(c.index(), 0);
    assert_eq!(s.get(a), None);
    assert_eq!(s.get(c), Some(&'c'));
    for _ in 2..CHANNELS {
        assert!(s.insert('x').is_some());
    }
    assert_eq!(s.len(), CHANNELS);
    assert!(
        s.insert('y').is_none(),
        "64 channels, as Device::FindFreeChannel"
    );
}

#[test]
fn wrap_and_pitch() {
    let t = tone(250, 2);
    assert_eq!(t.duration(), Duration::from_millis(2500));
    assert_eq!(
        t.wrap(Duration::from_millis(3000)),
        Duration::from_millis(500)
    );
    assert!((pitch_ratio(12.0) - 2.0).abs() < 1e-4);
    assert_eq!(pitch_ratio(0.0), 1.0);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn shared_source_loops_seeks_and_shares_samples() {
    use rodio::Source;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    let t = tone(3, 2);
    let cursor = Arc::new(AtomicUsize::new(0));
    let looping = Arc::new(AtomicBool::new(true));
    let mut src = native::SharedSource {
        sound: t.clone(),
        cursor: cursor.clone(),
        looping: looping.clone(),
    };
    let first: Vec<f32> = (&mut src).take(8).collect();
    assert_eq!(first, vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 0.0, 1.0]);
    src.try_seek(Duration::from_millis(20)).unwrap();
    assert_eq!(cursor.load(Ordering::Relaxed), 4);
    // Many channels can hold one buffer.
    assert_eq!(Arc::strong_count(&t.pcm), 2);
    looping.store(false, Ordering::Relaxed);
    assert_eq!(src.collect::<Vec<_>>(), vec![4.0, 5.0]);
}

/// Needs an audio output: `cargo test -p unbanished-audio -- --ignored`.
#[cfg(not(target_arch = "wasm32"))]
#[test]
#[ignore]
fn channels_play_together_and_free_on_end() {
    let mut device = Device::default();
    device.set_master_volume(0.0);
    let long = tone(100, 1);
    let short = tone(5, 1);
    let music = device
        .play(
            &long,
            PlayParams {
                looping: Some(true),
                ..Default::default()
            },
        )
        .unwrap()
        .unwrap();
    let effect = device.play(&short, PlayParams::default()).unwrap().unwrap();
    assert_ne!(music.index(), effect.index());
    assert!(device.is_playing(music) && device.is_active(effect));
    std::thread::sleep(Duration::from_millis(300));
    // The short effect ended and is freed; the looping music keeps its channel.
    assert_eq!(device.update(), vec![effect]);
    assert!(device.is_playing(music));
    assert_eq!(device.active(), 1);
    device.stop(music);
    assert_eq!(device.active(), 0);
}
