//! Mod packages (`.pkm`) and the loaded-resource registry, against downloaded mods kept in the
//! git-ignored `game-data/mods/downloads` (skipped when absent).

use banished_formats::crc::type_id;
use banished_formats::pkg::{Archive, Package};
use banished_formats::registry::{roots, Registry};

fn root() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn base() -> Option<Archive> {
    let dir = root().join("game-data/original/WinData");
    let mut a = Archive::default();
    for n in ["data1.pkg", "patch1.pkg", "patch0.pkg", "data0.pkg"] {
        a.add(Package::parse(std::fs::read(dir.join(n)).ok()?).ok()?);
    }
    Some(a)
}

fn pkm(name: &str) -> Option<Package> {
    let bytes = std::fs::read(root().join("game-data/mods/downloads").join(name)).ok()?;
    Some(Package::parse(bytes).unwrap())
}

/// Version 1 (UTF-16 strings, no preview) and version 3 mods read, every entry decodes.
#[test]
fn mod_packages_read() {
    for (file, name, version) in [
        ("apiary.pkm", "Sample - Apiary", 1),
        ("gatherable.pkm", "More Gatherables", 3),
    ] {
        let Some(p) = pkm(file) else {
            eprintln!("{file} missing; skipping");
            continue;
        };
        assert_eq!(p.header.name, name);
        assert_eq!(p.header.version, version);
        assert_eq!(
            p.header.icon.0 * p.header.icon.1 * 4,
            p.header.icon.2.len() as u32
        );
        for e in &p.entries {
            p.read(e).unwrap();
        }
    }
}

/// The game's resources reach every toolbar entry, profession and raw material; an enabled
/// mod's `<file>Resources.rsc` adds its own.
#[test]
fn the_registry_holds_the_game_and_its_mods() {
    let Some(mut a) = base() else {
        return;
    };
    let vanilla = Registry::build(&a, &roots(&[]));
    assert_eq!(vanilla.of_type(type_id("Profession")).len(), 22);
    assert_eq!(vanilla.of_type(type_id("RawMaterialDescription")).len(), 40);
    let Some(apiary) = pkm("apiary.pkm") else {
        return;
    };
    a.add_mod(apiary);
    let modded = Registry::build(&a, &roots(&["apiary".to_string()]));
    assert!(modded.contains(r"Apiary.rsc:apiary"));
    assert_eq!(modded.of_type(type_id("Profession")).len(), 23);
    assert_eq!(modded.of_type(type_id("RawMaterialDescription")).len(), 41);
}
