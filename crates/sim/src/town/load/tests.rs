use super::*;
use unbanished_formats::pkg::Package;

fn root() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn archive() -> Option<Archive> {
    let dir = root().join("game-data/original/WinData");
    let mut a = Archive::default();
    for name in ["data1.pkg", "patch1.pkg", "patch0.pkg", "data0.pkg"] {
        a.add(Package::parse(std::fs::read(dir.join(name)).ok()?).ok()?);
    }
    Some(a)
}

/// The saved wells are the town's again, each a placed well in its saved slot, and the
/// water's points and blocks are the saved ones.
#[test]
fn vanilla_saves_restore_their_wells() {
    let Some(archive) = archive() else {
        eprintln!("game data missing; skipping");
        return;
    };
    let mut total = 0;
    for name in [
        "Save2.sav",
        "Save3.sav",
        "Save4.sav",
        "Save5.sav",
        "Save83.sav",
    ] {
        let Ok(bytes) = std::fs::read(root().join("save-game").join(name)) else {
            continue;
        };
        let saved = SavedGame::parse(&bytes).unwrap();
        let town = StartingTown::from_save(&archive, &saved).unwrap();
        let w = &town.construction.water;
        let Some(Value::List(wells)) = saved.state.town.at("town.wells") else {
            panic!("{name}: no wells");
        };
        assert_eq!(w.wells.len(), wells.len(), "{name}");
        for (slot, e) in w.wells.iter().enumerate() {
            let Some(e) = *e else { continue };
            total += 1;
            let t = &town.templates[town.placed[e].template];
            assert!(t.reference.contains("Well"), "{name}: {}", t.reference);
            assert!(
                w.blocks.iter().flatten().any(|&v| v == -1 - slot as i32),
                "{name}: well {slot} is in no block"
            );
        }
        if let Some(Value::List(points)) = saved.state.town.at("town.water_sources") {
            assert_eq!(w.points.len(), points.len(), "{name}");
        }
    }
    eprintln!("{total} wells restored");
}

/// The vanilla sample saves (no mods): every entity with a map footprint is placed where it
/// was, each stamped cell points at an entity covering it, and growth and spawn state carry
/// over.
#[test]
fn vanilla_saves_restore_their_placed_entities() {
    let Some(archive) = archive() else {
        eprintln!("game data missing; skipping");
        return;
    };
    for name in [
        "Save2.sav",
        "Save3.sav",
        "Save4.sav",
        "Save5.sav",
        "Save83.sav",
    ] {
        let Ok(bytes) = std::fs::read(root().join("save-game").join(name)) else {
            eprintln!("{name} missing; skipping");
            continue;
        };
        let saved = SavedGame::parse(&bytes).unwrap();
        assert!(saved.metadata.mods.is_empty(), "{name} is not vanilla");
        let town = StartingTown::from_save(&archive, &saved).unwrap();
        // Entities with a footprint, then wildlife and boats (placed since stage 3c).
        let placed: Vec<_> = saved
            .entities()
            .filter(|(_, e)| {
                [
                    "MapComponent",
                    "PopulationComponent",
                    "HerdComponent",
                    "AnimalComponent",
                    "BoatComponent",
                ]
                .iter()
                .any(|c| saved.component(e, c).is_some())
            })
            .collect();
        assert_eq!(town.placed.len(), placed.len(), "{name}");
        for (p, (_, e)) in town.placed.iter().zip(&placed) {
            let Some(map) = saved.component(e, "MapComponent") else {
                continue;
            };
            let map = &map.fields;
            assert_eq!(
                p.cell,
                (map.i32("map.x").unwrap(), map.i32("map.y").unwrap())
            );
            assert_eq!(
                p.growth_state.is_some(),
                saved.component(e, "GrowthComponent").is_some()
            );
            if let Some(s) = &p.spawn_state {
                assert_eq!(s.timers.len(), town.templates[p.template].spawn.len());
            }
        }
        let n = town.cells.size as i32;
        let mut stamped = 0;
        for (i, o) in town.cells.objects.iter().enumerate() {
            let Some(o) = *o else { continue };
            stamped += 1;
            let (x, y) = (i as i32 % n, i as i32 / n);
            let p = &town.placed[o];
            let (w, h) = p.footprint;
            assert!(
                (p.cell.0..p.cell.0 + w.max(1)).contains(&x)
                    && (p.cell.1..p.cell.1 + h.max(1)).contains(&y),
                "{name}: cell ({x}, {y}) points at entity {o} at {:?} {:?}",
                p.cell,
                p.footprint
            );
        }
        eprintln!(
            "{name}: {} placed, {stamped} stamped cells",
            town.placed.len()
        );
    }
}
