//! A separate towns game: a town for each player on one map (docs/SEPARATE-TOWNS.md).

use super::generated_players;

/// Each player's structures stand together around their own start, far from the other's, and
/// its households are theirs.
#[test]
fn each_player_gets_a_town_on_the_same_map() {
    let Some((_game, gameplay)) = generated_players(2) else {
        return;
    };
    let town = gameplay.sim.town.as_ref().unwrap();
    assert_eq!(gameplay.sim.players(), 2);
    let centroid = |owner: u8| {
        let cells: Vec<(i32, i32)> = town
            .placed
            .iter()
            .filter(|p| p.owner == owner && town.templates[p.template].model.is_some())
            .map(|p| p.cell)
            .collect();
        assert!(!cells.is_empty(), "player {owner} has buildings");
        let n = cells.len() as f32;
        let x: f32 = cells.iter().map(|c| c.0 as f32).sum::<f32>() / n;
        let y: f32 = cells.iter().map(|c| c.1 as f32).sum::<f32>() / n;
        (x, y)
    };
    let (a, b) = (centroid(0), centroid(1));
    let away = ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt();
    assert!(away > 20.0, "the towns are {away} cells apart");
    for owner in [0, 1] {
        assert!(
            town.citizens.iter().any(|c| c.owner == owner),
            "player {owner} has citizens"
        );
        assert!(
            !town.registered_storages_of(owner).is_empty(),
            "player {owner} has storages"
        );
    }
}

/// Each player's storages got their own starting goods.
#[test]
fn each_town_starts_with_its_own_goods() {
    let Some((_game, gameplay)) = generated_players(2) else {
        return;
    };
    let town = gameplay.sim.town.as_ref().unwrap();
    let goods = |owner: u8| -> i32 {
        town.registered_storages_of(owner)
            .iter()
            .map(|s| s.items.iter().map(|i| i.count).sum::<i32>())
            .sum()
    };
    assert!(goods(0) > 0, "the first town's storages hold goods");
    assert!(goods(1) > 0, "the second town's storages hold goods");
}
