use super::tests_support::*;
use unbanished_formats::savegame::Value;
use unbanished_game::behaviour::animation::shared;

#[test]
fn vanilla_saves_restore_animation() {
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
        "Goldfoothill.sav",
    ] {
        let Ok(bytes) = std::fs::read(root().join("save-game").join(name)) else {
            continue;
        };
        let saved = crate::saved::SavedGame::parse(&bytes).unwrap();
        let world = crate::town::map::Map::from_save(&archive, &saved).unwrap();
        let grid = world.heights.grid();
        let mut town = crate::town::StartingTown::from_save(&archive, &saved).unwrap();

        // Each citizen's newest layer is its model's first saved state (the town's citizen
        // list is in the restored citizens' order).
        let Some(Value::List(citizens)) = saved.state.town.at("town.citizens") else {
            panic!("{name}: no citizens");
        };
        for (c, entry) in citizens.iter().enumerate() {
            let component = saved.object(entry.ptr("entity").unwrap()).unwrap();
            let entity = saved
                .object(component.fields.ptr("base.entity").unwrap())
                .unwrap();
            let Some(model) = saved.component(entity, "ModelComponent") else {
                continue;
            };
            let Some(Value::List(states)) = model.fields.at("model.animations") else {
                continue;
            };
            let layers = &town.agents.characters[c].animation.layers;
            assert_eq!(layers.len(), states.len(), "{name}: citizen {c}'s layers");
            assert_eq!(
                layers[0].playback.clip as i32,
                states[0].i32("clip").unwrap(),
                "{name}: citizen {c}'s clip"
            );
            assert_eq!(
                layers[0].instance,
                states[0].i32("instance").filter(|&i| i >= 0),
                "{name}: citizen {c}'s instance"
            );
        }

        // The shared instances keep their slots and users.
        let slots = shared::snapshot();
        let Some(Value::List(entries)) = saved.state.animations.at("animations.entries") else {
            panic!("{name}: no animation entries");
        };
        assert_eq!(slots.len(), entries.len(), "{name}: slots");
        for (slot, e) in slots.iter().zip(entries) {
            let users = e.i32("users").unwrap();
            match slot {
                Some((_, clip, u, _)) => {
                    assert_eq!(
                        (*clip as i32, *u),
                        (e.i32("clip").unwrap(), users),
                        "{name}"
                    );
                }
                None => assert!(e.i32("clip").unwrap() < 0, "{name}: a used slot left free"),
            }
        }

        // Ten seconds on, the citizens who were walking (a started walk on top of the stack, past
        // its first cell) have moved.
        let walking: Vec<usize> = (0..town.citizens.len())
            .filter(|&c| {
                let (entries, _) = town.agents.characters[c].stack.saved();
                matches!(entries.last(), Some((unbanished_game::behaviour::character::Action::MoveTo(m), true)) if m.saved_way().0 >= 0)
            })
            .collect();
        let before: Vec<[f32; 3]> = walking
            .iter()
            .map(|&c| town.agents.characters[c].position)
            .collect();
        let mut rng = unbanished_game::random::Random::new(83);
        let dt = 1.0 / 30.0;
        for _ in 0..30 * 10 {
            town.apply_commands(&grid, &mut rng);
            town.iterate(&grid, &mut rng, dt, 50.0);
            let warnings = unbanished_game::events::StockWarnings::default();
            let stock = std::array::from_fn(|i| (warnings.count[i], warnings.limits[i]));
            town.update_citizens(&grid, &mut rng, dt, 50.0, stock);
        }
        let moved = walking
            .iter()
            .zip(&before)
            .filter(|(&c, p)| town.agents.characters[c].position != **p)
            .count();
        eprintln!(
            "{name}: {moved} of {} walking citizens moved",
            walking.len()
        );
        assert_eq!(
            moved,
            walking.len(),
            "{name}: a walking citizen stood still"
        );
    }
}
