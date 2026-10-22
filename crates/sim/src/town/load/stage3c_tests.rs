use super::tests_support::*;

#[test]
fn vanilla_saves_restore_wildlife() {
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
            continue;
        };
        let saved = crate::saved::SavedGame::parse(&bytes).unwrap();
        let town = crate::town::StartingTown::from_save(&archive, &saved).unwrap();
        let count = |t: &str| {
            saved
                .state
                .objects
                .values()
                .filter(|o| o.type_name == t)
                .count()
        };
        let w = &town.wildlife;
        assert_eq!(
            w.populations.len(),
            count("PopulationComponent"),
            "{name}: populations"
        );
        assert_eq!(w.herds.len(), count("HerdComponent"), "{name}: herds");
        assert_eq!(w.animals.len(), count("AnimalComponent"), "{name}: animals");
        assert_eq!(w.boats.len(), count("BoatComponent"), "{name}: boats");
        for (h, herd) in w.herds.iter().enumerate() {
            for a in &herd.animals {
                assert_eq!(
                    w.animals.get(a).and_then(|a| a.herd),
                    Some(h),
                    "{name}: animal {a} of herd {h}"
                );
            }
        }
        let mut kept = 0;
        for (&p, pasture) in &w.pastures {
            for a in &pasture.livestock {
                kept += 1;
                assert_eq!(
                    w.animals
                        .get(a)
                        .and_then(|a| a.livestock)
                        .and_then(|l| l.pasture),
                    Some(p),
                    "{name}: animal {a} of pasture {p}"
                );
            }
        }
        eprintln!(
            "{name}: {} herds, {} animals ({kept} in pastures), {} boats",
            w.herds.len(),
            w.animals.len(),
            w.boats.len()
        );
    }
}
