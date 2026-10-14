//! The main menu music decodes from the player's own data (skipped without game-data/).

use banished_formats::pkg::{Archive, Package};
use std::path::PathBuf;

#[test]
fn menu_music_decodes() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../game-data/original/WinData");
    let mut archive = Archive::default();
    for name in ["data1.pkg", "patch1.pkg", "patch0.pkg", "data0.pkg"] {
        let Ok(bytes) = std::fs::read(dir.join(name)) else {
            eprintln!("game data missing; skipping");
            return;
        };
        archive.add(Package::parse(bytes).unwrap());
    }
    let (pcm, looping) =
        banished_audio::load_sound(&archive, "Audio\\Music\\Menu.rsc:music").unwrap();
    let seconds = pcm.samples.len() as f32 / pcm.channels as f32 / pcm.sample_rate as f32;
    eprintln!(
        "menu music: {} ch, {} Hz, {seconds:.1} s, looping {looping}",
        pcm.channels, pcm.sample_rate
    );
    assert!(seconds > 10.0);
}
