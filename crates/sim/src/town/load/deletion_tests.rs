use super::tests_support::*;

/// After two minutes of Save83, every scheduled entity is alive: destroyed sites, picked-up piles
/// and cleared resources have left their buckets (`FUN_1400c8db0`).
#[test]
fn no_deleted_entity_stays_in_a_bucket() {
    let Some(archive) = archive() else {
        eprintln!("game data missing; skipping");
        return;
    };
    let bytes = std::fs::read(root().join("save-game").join("Save83.sav")).unwrap();
    let saved = crate::saved::SavedGame::parse(&bytes).unwrap();
    let world = crate::town::map::Map::from_save(&archive, &saved).unwrap();
    let grid = world.heights.grid();
    let mut town = crate::town::StartingTown::from_save(&archive, &saved).unwrap();
    let mut rng = unbanished_game::random::Random::new(83);
    let dt = 1.0 / 30.0;
    for _ in 0..30 * 120 {
        town.apply_commands(&grid, &mut rng);
        town.iterate(&grid, &mut rng, dt, 50.0);
        let warnings = unbanished_game::events::StockWarnings::default();
        let stock = std::array::from_fn(|i| (warnings.count[i], warnings.limits[i]));
        town.update_citizens(&grid, &mut rng, dt, 50.0, stock);
    }
    for b in 0..5 {
        let list = town.scheduler.bucket(b);
        let dead = list
            .iter()
            .filter(|&&h| h < town.placed.len() && !town.placed[h].alive)
            .count();
        assert_eq!(
            dead,
            0,
            "bucket {b}: {dead} deleted entities of {}",
            list.len()
        );
    }
}
