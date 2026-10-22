use super::tests_support::*;
use unbanished_formats::savegame::Value;

#[test]
fn vanilla_saves_restore_construction() {
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
        let c = &town.construction;

        // The stock manager's counts are recomputed from storages (`FUN_14008f3a0`).
        let table =
            unbanished_game::work::stock::stock_table(&town.registered_storages(), &town.materials);
        assert_eq!(table, saved.stock_warnings().count, "{name}: stock");

        let machines: Vec<String> = saved
            .entities()
            // Trader boats are characters, not sites (stage 3c).
            .filter(|(_, e)| {
                saved.component(e, "StateMachineComponent").is_some()
                    && saved.component(e, "BoatComponent").is_none()
            })
            .map(|(_, e)| {
                saved
                    .ext(&e.fields, "entity.template")
                    .unwrap_or("?")
                    .to_string()
            })
            .collect();
        let mut missing = machines.clone();
        for site in c.sites.values() {
            let r = &town.templates[town.placed[site.entity].template].reference;
            if let Some(i) = missing.iter().position(|m| crate::town::normalise(m) == *r) {
                missing.remove(i);
            }
        }
        assert_eq!(
            c.sites.len(),
            machines.len(),
            "{name}: sites; not restored: {missing:?}"
        );
        let mut bad = Vec::new();
        for (&e, site) in &c.sites {
            let m = &site.machine;
            if m.state >= 0 && m.controllers[m.current].find(m.state) != Some(m.index as usize) {
                bad.push(format!(
                    "{} state {} index {} controllers {:?} current {}",
                    town.templates[town.placed[e].template].reference,
                    m.state,
                    m.index,
                    m.controllers,
                    m.current
                ));
            }
            // WorkComponent `+0x28` keeps the id of a work the manager has since removed
            // (a finished build); the port leaves it too and checks it is registered.
            if let Some(w) = c.jobs.works.get(&site.work_id) {
                assert_eq!(
                    w.entity, e,
                    "{name}: site {e} ({}) work {}",
                    town.templates[town.placed[e].template].reference, site.work_id
                );
            }
            assert!(
                !c.placed.contains(&e),
                "{name}: site {e} queued for activation"
            );
        }
        bad.sort();
        bad.dedup();
        assert!(bad.is_empty(), "{name}: machines {bad:#?}");
        for (&e, pile) in &c.piles {
            assert_eq!(
                c.jobs.works.get(&pile.work).map(|w| w.entity),
                Some(e),
                "{name}: pile {e} work"
            );
        }
        // Every work in the save (all kinds are restored since stage 3b).
        let saved_works = match saved.state.managers.at("jobs.works") {
            Some(Value::List(w)) => w.len(),
            _ => 0,
        };
        assert_eq!(c.jobs.works.len(), saved_works, "{name}: works");
        let roads = c
            .jobs
            .works
            .values()
            .filter(|w| matches!(w.kind, unbanished_game::work::jobs::WorkKind::Road(_)))
            .count();
        assert_eq!(c.pending_roads.len(), roads, "{name}: planned roads");
        let listed = ["town.storages", "town.markets"]
            .iter()
            .map(|l| match saved.state.town.at(l) {
                Some(Value::List(v)) => v.len(),
                _ => 0,
            })
            .sum::<usize>();
        assert_eq!(
            town.storage_sites.len(),
            listed,
            "{name}: storage registrations"
        );
        eprintln!(
            "{name}: {} sites, {} piles, {} works ({roads} roads), stock {:?}",
            c.sites.len(),
            c.piles.len(),
            c.jobs.works.len(),
            &table[..8]
        );
    }
}
