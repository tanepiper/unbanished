//! A separate towns game: a town for each player on one map (docs/SEPARATE-TOWNS.md).

use super::generated_players;
use crate::loops::GameLoop;

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

/// The owners of a two-town game survive a save and load through the sidecar beside the save.
#[test]
fn a_saved_two_town_game_keeps_its_owners() {
    let Some((mut game, gameplay)) = generated_players(2) else {
        return;
    };
    game.with_ctx(|_, ctx| {
        let bytes = gameplay
            .save_file(ctx.archive, ctx.ui, ctx.mods, Some("Two towns"), None)
            .expect("a save");
        let live = gameplay.sim.town.as_ref().unwrap();
        let (placed, citizens): (Vec<u8>, Vec<u8>) = (
            live.placed.iter().map(|p| p.owner).collect(),
            live.citizens.iter().map(|c| c.owner).collect(),
        );
        assert!(placed.contains(&1), "the second town has buildings");
        // A save read without its sidecar is a game whose owners are all 0.
        let plain = crate::saves::saved::SavedGame::parse(&bytes).unwrap();
        assert!(plain.owners.is_none());
        let saved = plain.with_owners(&gameplay.sim.owners().encode());
        let town = unbanished_sim::town::StartingTown::from_save(ctx.archive, &saved).unwrap();
        assert_eq!(
            town.placed.iter().map(|p| p.owner).collect::<Vec<_>>(),
            placed
        );
        assert_eq!(
            town.citizens.iter().map(|c| c.owner).collect::<Vec<_>>(),
            citizens
        );
    });
}

/// A separate towns game's digest answers to its owners; a single-player one leaves them out.
#[test]
fn the_digest_covers_who_owns_what() {
    let Some((mut game, gameplay)) = generated_players(2) else {
        return;
    };
    game.with_ctx(|_, ctx| {
        let bytes = gameplay
            .save_file(ctx.archive, ctx.ui, ctx.mods, None, None)
            .expect("a save");
        let load = |ctx: &mut crate::loops::Ctx| {
            let saved = crate::saves::saved::SavedGame::parse(&bytes).unwrap();
            let mut g = crate::gameplay::GameplayLoop::from_save(saved);
            g.load(ctx);
            g.startup(ctx);
            g
        };
        // Two towns: changing an owner changes the digest.
        let mut two = load(ctx);
        two.sim.town.as_mut().unwrap().placed[0].owner = 1;
        two.sim.set_players(2);
        let before = two.digest(ctx, 0).unwrap().crc;
        two.sim.town.as_mut().unwrap().placed[0].owner = 0;
        assert_ne!(before, two.digest(ctx, 0).unwrap().crc);
        // One player: the owners ride in no digest.
        let mut one = load(ctx);
        let before = one.digest(ctx, 0).unwrap().crc;
        one.sim.town.as_mut().unwrap().placed[0].owner = 1;
        assert_eq!(before, one.digest(ctx, 0).unwrap().crc);
    });
}
