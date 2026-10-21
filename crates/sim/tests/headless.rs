//! The core runs with no window, no GPU and no interface: a save loads, plays and is written
//! again with `unbanished-sim` alone, and two runs from the same save end in the same state.
//! The weather is left out here (no environment), so the state is not the game app's.

use std::path::{Path, PathBuf};

use unbanished_formats::pkg::{Archive, Package};
use unbanished_sim::saved::SavedGame;
use unbanished_sim::simulation::Simulation;
use unbanished_sim::town::map::Map;
use unbanished_sim::town::StartingTown;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn archive() -> Option<Archive> {
    let dir = root().join("game-data/original/WinData");
    let mut archive = Archive::default();
    for name in ["data1.pkg", "patch1.pkg", "patch0.pkg", "data0.pkg"] {
        archive.add(Package::parse(std::fs::read(dir.join(name)).ok()?).ok()?);
    }
    Some(archive)
}

/// Loads `saved`, runs `iterations` iterations of 1/60 s, and returns the written state and
/// the living citizens.
fn play(archive: &Archive, saved: &SavedGame, iterations: usize) -> (Vec<u8>, usize) {
    let mut sim = Simulation::new(0);
    sim.restore(saved);
    let map = Map::from_save(archive, saved).expect("the save's map");
    sim.heights = Some(map.heights);
    sim.environment_template = map.environment.template.clone();
    sim.environment_months = map.environment.months;
    sim.town = Some(StartingTown::from_save(archive, saved).expect("the save's town"));
    for _ in 0..iterations {
        sim.iterate(archive, 1.0 / 60.0);
    }
    let town = sim.town.as_ref().unwrap();
    let mut state = saved.state.clone();
    town.export_into(archive, &mut state);
    let alive = town.citizens.iter().filter(|c| c.alive).count();
    (state.encode().expect("encode"), alive)
}

#[test]
fn a_save_plays_and_saves_with_the_core_alone() {
    let Some(archive) = archive() else {
        eprintln!("game data missing; skipping");
        return;
    };
    let Ok(bytes) = std::fs::read(root().join("save-game/Save83.sav")) else {
        eprintln!("save-game/Save83.sav missing; skipping");
        return;
    };
    let saved = SavedGame::parse(&bytes).unwrap();
    let (first, alive) = play(&archive, &saved, 600);
    let (second, _) = play(&archive, &saved, 600);
    assert!(alive > 0, "nobody is alive");
    assert_ne!(
        first,
        saved.state.encode().unwrap(),
        "ten seconds changed nothing"
    );
    assert!(first == second, "two runs from the same save differ");
    let reloaded = SavedGame::new(
        unbanished_formats::savegame::GameState::decode(&first).unwrap(),
        saved.metadata.clone(),
    );
    let town = StartingTown::from_save(&archive, &reloaded).expect("the written state loads");
    assert_eq!(town.citizens.len(), alive);
}
