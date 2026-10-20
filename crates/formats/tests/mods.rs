//! Mod packages (`.pkm`) and the loaded-resource registry, against downloaded mods kept in the
//! git-ignored `game-data/mods/downloads` (skipped when absent).

use unbanished_formats::crc::type_id;
use unbanished_formats::pkg::{Archive, Package};
use unbanished_formats::registry::{roots, Registry};

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

/// `Registry::build` reads the resources in parallel; the result must equal the plain
/// depth-first walk that reads one resource at a time.
#[test]
fn the_registry_matches_a_walk_in_order() {
    use std::collections::{BTreeMap, BTreeSet};
    use unbanished_formats::pkg::ref_to_entry;
    use unbanished_formats::registry::references;
    let Some(a) = base() else {
        return;
    };
    let key = |r: &str| {
        let r = r.replace('/', "\\").to_ascii_lowercase();
        if r.contains(':') {
            r
        } else {
            format!("{r}:resource")
        }
    };
    let roots = roots(&[]);
    let mut loaded = BTreeMap::new();
    let mut seen = BTreeSet::new();
    let mut stack: Vec<String> = roots.iter().rev().cloned().collect();
    while let Some(r) = stack.pop() {
        let k = key(&r);
        if !seen.insert(k.clone()) {
            continue;
        }
        let Ok(data) = a.read(&ref_to_entry(&k)) else {
            continue;
        };
        let Ok(header) = unbanished_formats::crs::header(&data) else {
            continue;
        };
        let written = if r.contains(':') {
            r.replace('/', "\\")
        } else {
            format!("{}:resource", r.replace('/', "\\"))
        };
        loaded.insert(k, (header.type_id, written));
        stack.extend(references(&data).into_iter().rev());
    }
    assert_eq!(Registry::build(&a, &roots).loaded, loaded);
}
