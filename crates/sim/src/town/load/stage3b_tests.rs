use super::tests_support::*;
use unbanished_game::work::jobs::WorkKind;

#[test]
fn vanilla_saves_restore_stacks_and_claims() {
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
        // Saved depths in the citizens' order (`top + 1`).
        let depths: Vec<usize> = match saved.state.town.at("town.citizens") {
            Some(unbanished_formats::savegame::Value::List(cs)) => cs
                .iter()
                .map(|c| {
                    let character = c
                        .ptr("entity")
                        .and_then(|c| saved.object(c))
                        .and_then(|c| c.fields.ptr("base.entity"))
                        .and_then(|e| saved.object(e))
                        .and_then(|e| saved.component(e, "CharacterComponent"));
                    character
                        .and_then(|k| k.fields.i32("character.stack_top"))
                        .map_or(0, |t| (t + 1).max(0) as usize)
                })
                .collect(),
            _ => Vec::new(),
        };
        for (c, d) in depths.iter().enumerate() {
            assert_eq!(
                town.agents.characters[c].stack.depth(),
                *d,
                "{name}: stack of {c}"
            );
        }
        // A meal in progress keeps the storage it eats from (the message's argument).
        let mut meals = 0;
        for ch in &town.agents.characters {
            for (action, _) in ch.stack.saved().0 {
                if let unbanished_game::behaviour::character::Action::SendMessage(m) = action {
                    if m.component == unbanished_game::behaviour::character::Component::Citizen
                        && m.message == 2
                    {
                        meals += 1;
                        let site = m.entity.and_then(|e| town.construction.sites.get(&e));
                        assert!(site.is_some(), "{name}: a meal with no storage");
                    }
                }
            }
        }
        let mut held = 0;
        for w in town.construction.jobs.works.values() {
            let workers: Vec<usize> = match &w.kind {
                WorkKind::Gather(g) => g.workers.iter().copied().collect(),
                WorkKind::Build(b) => b.workers.iter().copied().collect(),
                WorkKind::Herbalist(h)
                | WorkKind::Forester(h)
                | WorkKind::Hunt(h)
                | WorkKind::TendArea(h)
                | WorkKind::Pasture(h)
                | WorkKind::ClearStorage(h)
                | WorkKind::Fire(h) => h.workers.iter().copied().collect(),
                _ => Vec::new(),
            };
            for c in workers {
                held += 1;
                assert_eq!(
                    town.agents.states[c].job, w.id,
                    "{name}: worker {c} of work {}",
                    w.id
                );
            }
        }
        eprintln!(
            "{name}: {} stacks, {held} workers on their works, {meals} meals",
            depths.len()
        );
    }
}
