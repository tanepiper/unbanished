//! A separate towns game: a town for each player on one map (docs/SEPARATE-TOWNS.md).

use super::generated_players;
use crate::gameplay::lockstep::Message;
use crate::gameplay::network::Link;
use crate::loops::GameLoop;
use crate::tools::selection::Entity;

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
        let (bytes, owners) = gameplay
            .save_file(ctx.archive, ctx.ui, ctx.mods, Some("Two towns"), None)
            .expect("a save");
        let live = gameplay.sim.town.as_ref().unwrap();
        let (placed, citizens): (Vec<u8>, Vec<u8>) = (
            live.placed.iter().map(|p| p.owner).collect(),
            live.citizens.iter().map(|c| c.owner).collect(),
        );
        assert!(placed.contains(&1), "the second town has buildings");
        // The sidecar names only what is not player 0's: load gives the rest player 0.
        assert!(!owners.placed.is_empty() && !owners.citizens.is_empty());
        assert!(owners
            .placed
            .iter()
            .chain(&owners.citizens)
            .all(|&(_, o)| o != 0));
        // A save read without its sidecar is a game whose owners are all 0.
        let plain = crate::saves::saved::SavedGame::parse(&bytes).unwrap();
        assert!(plain.owners.is_none());
        let saved = plain.with_owners(&owners.encode());
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

/// A save after a death keeps each owner on its own entity: the save leaves the dead behind, so
/// a position in the town's list no longer names the same thing in the file.
#[test]
fn a_saved_two_town_game_keeps_its_owners_across_deaths() {
    let Some((mut game, gameplay)) = generated_players(2) else {
        return;
    };
    game.with_ctx(|_, ctx| {
        let mut gameplay = gameplay;
        {
            let town = gameplay.sim.town.as_mut().unwrap();
            let dead = town
                .placed
                .iter()
                .position(|p| town.templates[p.template].reference.contains("Tree"))
                .expect("a tree");
            assert_ne!(dead, town.placed.len() - 1, "a tree before the last entity");
            town.placed[dead].alive = false;
            let citizen = town
                .citizens
                .iter()
                .position(|c| c.alive && c.owner == 0)
                .expect("a citizen");
            town.citizens[citizen].alive = false;
        }
        let (bytes, owners) = gameplay
            .save_file(ctx.archive, ctx.ui, ctx.mods, None, None)
            .expect("a save");
        let live = gameplay.sim.town.as_ref().unwrap();
        let placed: std::collections::BTreeSet<_> = live
            .placed
            .iter()
            .filter(|p| p.alive)
            .map(|p| {
                (
                    live.templates[p.template].reference.clone(),
                    p.cell,
                    p.owner,
                )
            })
            .collect();
        let people: std::collections::BTreeSet<_> = live
            .citizens
            .iter()
            .filter(|c| c.alive)
            .map(|c| (c.name.clone(), c.owner))
            .collect();
        let saved = crate::saves::saved::SavedGame::parse(&bytes)
            .unwrap()
            .with_owners(&owners.encode());
        let town = unbanished_sim::town::StartingTown::from_save(ctx.archive, &saved).unwrap();
        assert_eq!(town.citizens.len(), people.len());
        let owner_of: std::collections::BTreeMap<_, _> = placed
            .iter()
            .map(|(reference, cell, owner)| ((reference.clone(), *cell), *owner))
            .collect();
        let mut seen = std::collections::BTreeSet::new();
        for p in &town.placed {
            let key = (town.templates[p.template].reference.clone(), p.cell);
            match owner_of.get(&key) {
                // The save's first entities are the map's own, which no player owns.
                Some(&owner) => {
                    assert_eq!(p.owner, owner, "{key:?}");
                    seen.insert(key);
                }
                None => assert_eq!(p.owner, 0, "{key:?} belongs to no player"),
            }
        }
        assert_eq!(seen.len(), owner_of.len(), "every alive entity came back");
        for c in &town.citizens {
            assert!(people.contains(&(c.name.clone(), c.owner)), "{}", c.name);
        }
    });
}

/// A separate towns game's digest answers to its owners; a single-player one leaves them out.
#[test]
fn the_digest_covers_who_owns_what() {
    let Some((mut game, gameplay)) = generated_players(2) else {
        return;
    };
    game.with_ctx(|_, ctx| {
        let (bytes, _) = gameplay
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

/// A separate towns game picks only its own player's buildings and citizens, though every
/// player's town is drawn; a shared town's are everyone's.
#[test]
fn only_the_local_players_town_can_be_picked() {
    let Some((_game, mut gameplay)) = generated_players(2) else {
        return;
    };
    gameplay.sim.local_player = 0;
    let town = gameplay.sim.town.as_ref().unwrap();
    let find = |owner: u8, citizens: bool| {
        if citizens {
            town.citizens.iter().position(|c| c.owner == owner)
        } else {
            town.placed.iter().position(|p| p.owner == owner)
        }
        .expect("an entity of that player")
    };
    let (own, other) = (find(0, false), find(1, false));
    assert!(gameplay.may_select(town, Entity::Placed(own)));
    assert!(!gameplay.may_select(town, Entity::Placed(other)));
    let (own, other) = (find(0, true), find(1, true));
    assert!(gameplay.may_select(town, Entity::Citizen(own)));
    assert!(!gameplay.may_select(town, Entity::Citizen(other)));

    // The world's things are every player's: player 1 picks a tree, not player 0's buildings.
    gameplay.sim.local_player = 1;
    let town = gameplay.sim.town.as_ref().unwrap();
    let natural =
        |p: &unbanished_game::nature::seeding::Placed| town.templates[p.template].natural.is_some();
    let tree = town.placed.iter().position(natural).expect("a tree");
    assert!(gameplay.may_select(town, Entity::Placed(tree)));
    let building = town
        .placed
        .iter()
        .position(|p| p.owner == 0 && !natural(p) && town.templates[p.template].storage.is_some())
        .expect("player 0's storage");
    assert!(!gameplay.may_select(town, Entity::Placed(building)));

    // A shared town of two players has one town: either player picks any of it.
    let Some((_game, mut shared)) = generated_players(1) else {
        return;
    };
    shared.sim.set_players(2);
    shared.sim.local_player = 1;
    let town = shared.sim.town.as_ref().unwrap();
    assert!(!shared.sim.separate());
    assert!(shared.may_select(town, Entity::Placed(0)));
}

/// A room whose mode, or number of players, does not match the save it starts from is refused
/// with the reason; the matching one starts.
#[test]
fn a_room_refuses_a_save_of_the_other_mode() {
    let Some((mut game, gameplay)) = generated_players(2) else {
        return;
    };
    struct NoLink;
    impl Link for NoLink {
        fn send(&mut self, _to: Option<u8>, _message: &Message) {}
        fn poll(&mut self) -> Vec<(u8, Message)> {
            Vec::new()
        }
    }
    game.with_ctx(|_, ctx| {
        let (bytes, sidecar) = gameplay
            .save_file(ctx.archive, ctx.ui, ctx.mods, None, None)
            .expect("a save");
        let owners = sidecar.encode();
        let start = |separate: bool, players: u8| Message::Start {
            players,
            delay: 6,
            check: 60,
            absent: Vec::new(),
            bytes: bytes.clone(),
            owners: owners.clone(),
            separate,
            update: 0,
            epoch: 0,
            speed: 0,
            paused: false,
            tools: Vec::new(),
        };
        // A two-town save in a shared town room: refused.
        assert_eq!(
            crate::gameplay::GameplayLoop::from_start(1, start(false, 2), Box::new(NoLink), vec![])
                .err()
                .as_deref(),
            Some("the room's mode and its save disagree")
        );
        // A two-player save in a room of three: refused.
        assert_eq!(
            crate::gameplay::GameplayLoop::from_start(1, start(true, 3), Box::new(NoLink), vec![])
                .err()
                .as_deref(),
            Some("the save does not have a town for every player")
        );
        // The save's own mode and players: the guest starts.
        assert!(crate::gameplay::GameplayLoop::from_start(
            1,
            start(true, 2),
            Box::new(NoLink),
            vec![]
        )
        .is_ok());
    });
}

/// Each town counts its own people, which its town hall's immigration reads.
#[test]
fn each_town_counts_its_own_population() {
    let Some((mut game, mut gameplay)) = generated_players(2) else {
        return;
    };
    game.with_ctx(|_, ctx| {
        for _ in 0..30 {
            gameplay.update(ctx, 1.0 / 30.0);
        }
        let town = gameplay.sim.town.as_ref().unwrap();
        let count = |owner: u8| {
            town.agents
                .registry
                .iter()
                .filter(|&&c| town.citizens[c].owner == owner)
                .count() as i32
        };
        let c = &town.construction;
        assert_eq!(c.populations, vec![count(0), count(1)]);
        assert_eq!(c.population, count(0) + count(1));
        assert!(count(1) > 0, "the second town has people");
        // The town hall's professions are its own town's too.
        let adults = |owner: u8| {
            town.citizens
                .iter()
                .zip(&town.agents.states)
                .filter(|(c, s)| {
                    c.owner == owner && c.alive && c.is_adult(&town.rules) && s.school.is_none()
                })
                .count() as i32
        };
        for owner in 0..2 {
            assert_eq!(
                town.profession_counts(owner).0.iter().sum::<i32>(),
                adults(owner)
            );
        }
    });
}

/// Each town keeps its citizens to itself: no one lives, works, studies, worships or is treated
/// in the other player's buildings, and every couple is one town's.
#[test]
fn each_town_keeps_its_citizens() {
    let Some((mut game, mut gameplay)) = generated_players(2) else {
        return;
    };
    game.with_ctx(|_, ctx| {
        gameplay.paused = false;
        gameplay.speed = 3;
        for _ in 0..150 {
            gameplay.update(ctx, 1.0 / 30.0);
        }
        let town = gameplay.sim.town.as_ref().unwrap();
        let owner_of = |e: usize| town.placed.get(e).map_or(0, |p| p.owner);
        let mut homed = [0; 2];
        for (c, state) in town.agents.states.iter().enumerate() {
            let Some(citizen) = town.citizens.get(c) else {
                continue;
            };
            if !citizen.alive {
                continue;
            }
            let owner = citizen.owner;
            for (what, e) in [
                ("home", state.home),
                ("workplace", state.workplace),
                ("school", state.school),
                ("doctor", state.doctor),
                ("chapel", state.priest),
            ] {
                if let Some(e) = e {
                    assert_eq!(
                        owner_of(e),
                        owner,
                        "citizen {c}'s {what} ({e}) is another town's"
                    );
                }
            }
            if state.home.is_some() {
                homed[usize::from(owner.min(1))] += 1;
            }
            if let Some(s) = citizen.spouse {
                assert_eq!(
                    town.citizens[s].owner, owner,
                    "citizen {c}'s spouse is another town's"
                );
            }
        }
        assert!(
            homed[0] > 0 && homed[1] > 0,
            "both towns' citizens found homes: {homed:?}"
        );
    });
}
