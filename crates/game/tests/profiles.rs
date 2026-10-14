//! Decode the shipped New Game profiles from the player's own data (skipped without game-data/).

use banished_formats::game::map_generator;
use banished_formats::pkg::{Archive, Package};
use std::path::PathBuf;

fn archive() -> Option<Archive> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../game-data/original/WinData");
    let mut a = Archive::default();
    for name in ["data1.pkg", "patch1.pkg", "patch0.pkg", "data0.pkg"] {
        a.add(Package::parse(std::fs::read(dir.join(name)).ok()?).ok()?);
    }
    Some(a)
}

#[test]
fn start_conditions_decode() {
    let Some(archive) = archive() else {
        eprintln!("game data missing; skipping");
        return;
    };
    for name in ["Hard", "Medium", "Easy"] {
        let r = format!("Game/StartCondition/{name}.rsc:resource");
        let g = map_generator(&archive.read_ref(&r).unwrap()).unwrap();
        eprintln!("{name}: {g:#?}");
        assert_eq!(g.natural_resources.len(), 3);
    }
}
