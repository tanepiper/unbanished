//! Every SoundEffect, MusicList, AnimAudioEvent and ambient description in the shipped data
//! parses completely.

use std::path::PathBuf;

use banished_formats::audio;
use banished_formats::crc::type_id;
use banished_formats::crs;
use banished_formats::pkg::{Archive, Package};

fn archive() -> Option<Archive> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../game-data/original/WinData");
    if !dir.join("data0.pkg").exists() {
        eprintln!("game data not found; skipping");
        return None;
    }
    let mut archive = Archive::default();
    for name in ["data1.pkg", "patch1.pkg", "patch0.pkg", "data0.pkg"] {
        archive.add(Package::parse(std::fs::read(dir.join(name)).unwrap()).unwrap());
    }
    Some(archive)
}

#[test]
fn all_audio_resources_parse() {
    let Some(archive) = archive() else { return };
    let mut counts = std::collections::BTreeMap::<&str, usize>::new();
    for name in archive.names() {
        if !name.ends_with(".crs") {
            continue;
        }
        let data = archive.read(name).unwrap();
        let tid = crs::header(&data).unwrap().type_id;
        let class = if tid == type_id("SoundEffect") {
            let e = audio::sound_effect(&data).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert!(!e.sounds.is_empty(), "{name}");
            "SoundEffect"
        } else if tid == type_id("MusicList") {
            let l = audio::music_list(&data).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!(l.len(), 15);
            "MusicList"
        } else if tid == type_id("AnimAudioEvent") {
            audio::anim_audio_event(&data).unwrap_or_else(|e| panic!("{name}: {e}"));
            "AnimAudioEvent"
        } else if tid == type_id("AmbientEmitterDescription") {
            audio::ambient_emitter_description(&data).unwrap_or_else(|e| panic!("{name}: {e}"));
            "AmbientEmitterDescription"
        } else if tid == type_id("AmbientEnvironmentDescription") {
            let d = audio::ambient_environment_description(&data)
                .unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!(d.fade, (31.0, 36.0));
            "AmbientEnvironmentDescription"
        } else {
            continue;
        };
        *counts.entry(class).or_default() += 1;
    }
    eprintln!("{counts:?}");
    assert!(counts["SoundEffect"] > 30);
    assert_eq!(counts["MusicList"], 1);
}
