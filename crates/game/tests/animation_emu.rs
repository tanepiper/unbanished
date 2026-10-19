//! Pose/clock/event parity against the shipped Runtime functions (tools/emu/animation.py).
use banished_formats::types::{animation_group, AnimationGroup};
use banished_game::behaviour::animation::{sample_pose, Playback};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../game-data")
}

fn group<'a>(cache: &'a mut HashMap<String, AnimationGroup>, key: &str) -> &'a AnimationGroup {
    cache.entry(key.to_owned()).or_insert_with(|| {
        let bytes = std::fs::read(root().join("extracted").join(key)).unwrap();
        animation_group(&bytes).unwrap()
    })
}

#[test]
fn original_pose_samples_match() {
    let Ok(data) = std::fs::read(root().join("derived/emu/animation/poses.bin")) else {
        eprintln!("no animation ground truth; run tools/emu/animation.py");
        return;
    };
    let mut cache = HashMap::new();
    let mut seen = HashSet::new();
    let mut cursor = 0;
    let mut total = 0;
    fn u32(data: &[u8], cursor: &mut usize) -> u32 {
        let value = u32::from_le_bytes(data[*cursor..*cursor + 4].try_into().unwrap());
        *cursor += 4;
        value
    }
    fn i64(data: &[u8], cursor: &mut usize) -> i64 {
        let value = i64::from_le_bytes(data[*cursor..*cursor + 8].try_into().unwrap());
        *cursor += 8;
        value
    }
    while cursor < data.len() {
        let length = u32(&data, &mut cursor) as usize;
        let key = std::str::from_utf8(&data[cursor..cursor + length]).unwrap();
        cursor += length;
        let index = u32(&data, &mut cursor) as usize;
        let time = i64(&data, &mut cursor);
        let delta = i64(&data, &mut cursor);
        let count = u32(&data, &mut cursor) as usize;
        let pose = sample_pose(&group(&mut cache, key).clips[index], time, delta);
        assert_eq!(pose.len(), count);
        for (bone, transform) in pose.into_iter().enumerate() {
            let mut rotation = transform.rotation;
            // Native samples use the opposite quaternion convention. The root is explicitly
            // replaced by identity, so its zero signs need no convention conversion.
            if bone != 0 {
                for value in &mut rotation[..3] {
                    *value = -*value;
                }
            }
            for (component, value) in rotation
                .into_iter()
                .chain(transform.translation)
                .enumerate()
            {
                let expected = u32(&data, &mut cursor);
                assert_eq!(value.to_bits(), expected,
                    "{key} clip {index}, time {time}, delta {delta}, bone {bone}, component {component}");
            }
        }
        total += 1;
        seen.insert((key.to_owned(), index));
    }
    assert_eq!(cache.len(), 15);
    assert_eq!(seen.len(), 64);
    assert!(total > 10_000);
    eprintln!("{total} pose samples across all 64 shipped clips matched original bits");
}

#[test]
fn original_clock_completion_and_event_intervals_match() {
    let Ok(data) = std::fs::read_to_string(root().join("derived/emu/animation/clocks.tsv")) else {
        eprintln!("no animation ground truth; run tools/emu/animation.py");
        return;
    };
    let mut cache = HashMap::new();
    let mut total = 0;
    for line in data.lines() {
        let fields: Vec<_> = line.split('\t').collect();
        let index = fields[1].parse().unwrap();
        let clip = &group(&mut cache, fields[0]).clips[index];
        let mut playback = Playback::new(index, fields[2].parse().unwrap());
        playback.speed = fields[4].parse().unwrap();
        let events = playback.update(clip, fields[3].parse().unwrap());
        assert_eq!(
            playback.elapsed,
            fields[5].parse().unwrap(),
            "clock: {line}"
        );
        assert_eq!(
            playback.is_animating(clip),
            fields[6] == "1",
            "completion: {line}"
        );
        let expected: Vec<usize> = fields[7]
            .split(',')
            .filter(|s| !s.is_empty())
            .map(|s| s.parse().unwrap())
            .collect();
        assert_eq!(events, expected, "events: {line}");
        total += 1;
    }
    assert_eq!(total, 3072);
    eprintln!("{total} clock/completion/event cases matched the original");
}
