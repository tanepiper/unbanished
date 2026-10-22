use super::tests_support::*;

#[test]
fn vanilla_saves_restore_citizens() {
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
        let n = saved
            .state
            .objects
            .values()
            .filter(|o| o.type_name == "CitizenComponent")
            .count();
        assert_eq!(town.citizens.len(), n, "{name}: citizens");
        assert_eq!(town.agents.states.len(), n);
        let c = &town.construction;
        for (i, s) in town.agents.states.iter().enumerate() {
            // A family slot's home lists the citizen in that family.
            if let (Some(h), true) = (s.home, s.home_slot >= 0) {
                let home = c
                    .homes
                    .get(&h)
                    .or_else(|| c.sites.get(&h).and_then(|x| x.home.as_ref()));
                let family = home.and_then(|h| h.families.get(s.home_slot as usize));
                assert!(
                    family.is_some_and(|f| f.contains(&i)),
                    "{name}: citizen {i} not in its home"
                );
            }
            if let Some(sp) = town.citizens[i].spouse {
                assert_eq!(town.citizens[sp].spouse, Some(i), "{name}: spouse of {i}");
            }
            for &d in &s.dependents {
                assert_eq!(
                    town.citizens[d].parent,
                    Some(i),
                    "{name}: dependent {d} of {i}"
                );
            }
            if let Some(w) = s.workplace {
                if let Some(wp) = c.workforce.workplaces.get(&w) {
                    assert!(
                        wp.workers.contains(&i),
                        "{name}: citizen {i} not among its workplace's workers"
                    );
                }
            }
        }
        for (&e, wp) in &c.workforce.workplaces {
            for &w in &wp.workers {
                assert_eq!(
                    town.agents.states[w].workplace,
                    Some(e),
                    "{name}: worker {w} of {e}"
                );
            }
        }
        let housed = town
            .agents
            .states
            .iter()
            .filter(|s| s.home.is_some())
            .count();
        let working = town
            .agents
            .states
            .iter()
            .filter(|s| s.workplace.is_some())
            .count();
        eprintln!(
            "{name}: {n} citizens, {housed} housed, {working} with a workplace, {} homes",
            c.homes.len()
        );
    }
}
