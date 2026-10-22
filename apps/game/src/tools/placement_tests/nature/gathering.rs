//! Gatherers, foresters and fishermen.

use super::*;

/// A gatherer's shelter near the start gets two gatherers: they harvest grown wild food
/// within the radius, and the piles they leave are picked up (by them first, through the
/// AutoPickup) and stored.
#[test]
fn gatherers_harvest_wild_food() {
    let dir =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../game-data/original/WinData");
    let mut archive = Archive::default();
    for name in ["data1.pkg", "patch1.pkg", "patch0.pkg", "data0.pkg"] {
        let Ok(bytes) = std::fs::read(dir.join(name)) else {
            return;
        };
        archive.add(Package::parse(bytes).unwrap());
    }
    let world = crate::view::world::World::generate(
        &archive,
        &crate::view::world::WorldParams {
            seed: 1234,
            ..Default::default()
        },
    )
    .unwrap();
    let terrain = HeightGrid {
        width: world.terrain.width(),
        heights: &world.terrain.heights,
    };
    let mut rng = Random::new(1234);
    let mut town = StartingTown::create(
        &archive,
        "Game/StartCondition/Medium.rsc:resource",
        world.terrain.size,
        &terrain,
        &mut rng,
    )
    .unwrap();
    let mut tool = PlacementTool::start(
        &archive,
        &mut town,
        r"Template\GathererShelter.rsc:resource",
        &mut rng,
    )
    .unwrap();
    let spot = tool.find_clear_spot(&town, &terrain, town.start);
    tool.hover(&town, &terrain, spot, 0);
    let id = tool.place(&archive, &mut town, 0).unwrap();
    let st = town.construction.sites[&id].template;
    let kinds = town.construction.templates[st]
        .herbalist
        .clone()
        .expect("no HerbalistDescription")
        .resources;
    let p = &town.construction.professions;
    let builder = p.builder;
    let gatherer = p
        .list
        .iter()
        .position(|p| p.reference.to_ascii_lowercase().ends_with(":gatherer"))
        .expect("no gatherer profession");
    town.construction
        .commands
        .push(unbanished_game::work::professions::Command::SetCount {
            profession: builder,
            count: 2,
        });
    let mut warnings = unbanished_game::events::StockWarnings::default();
    warnings.limits[5] = 100_000;
    let wild = |town: &StartingTown| {
        town.placed
            .iter()
            .filter(|p| p.alive && !p.removal_requested && kinds.contains(&p.template))
            .count()
    };
    let wild_before = wild(&town);
    // The wild plants at the start: gathering takes some of them (new ones keep growing, so the
    // count alone can rise over a long run).
    let first: Vec<usize> = (0..town.placed.len())
        .filter(|&e| {
            let p = &town.placed[e];
            p.alive && !p.removal_requested && kinds.contains(&p.template)
        })
        .collect();
    let dt = 1.0 / 30.0;
    let (mut built_at, mut harvested_at, mut stored_at) = (None, None, None);
    let mut seen = std::collections::BTreeSet::new();
    let mut auto = 0;
    for step in 0..30 * 60 * 12 {
        town.apply_commands(&terrain, &mut rng);
        town.iterate(&terrain, &mut rng, dt, 50.0);
        let stock = std::array::from_fn(|i| (warnings.count[i], warnings.limits[i]));
        town.update_citizens(&terrain, &mut rng, dt, 50.0, stock);
        warnings.recompute(&town.registered_storages(), &town.materials);
        town.construction
            .jobs
            .step(unbanished_game::scheduler::ticks(dt));
        if built_at.is_none() && town.construction.sites[&id].complete {
            built_at = Some(step);
            town.construction.commands.push(
                unbanished_game::work::professions::Command::SetCount {
                    profession: gatherer,
                    count: 2,
                },
            );
        }
        let site = &town.construction.sites[&id];
        if let Some(a) = &site.auto_pickup {
            auto = auto.max(a.works.len());
        }
        let piles: Vec<usize> = town
            .construction
            .piles
            .iter()
            .filter(|(_, p)| p.auto_pickup == Some(id))
            .map(|(&e, _)| e)
            .collect();
        if harvested_at.is_none() && !piles.is_empty() {
            harvested_at = Some(step);
        }
        seen.extend(piles.iter().copied());
        if seen.iter().any(|e| !piles.contains(e)) {
            stored_at = Some(step);
            break;
        }
    }
    eprintln!(
        "built {built_at:?}, harvested {harvested_at:?}, stored {stored_at:?}, wild {} -> {}, \
         auto-pickup list up to {auto}",
        wild_before,
        wild(&town)
    );
    assert!(built_at.is_some(), "the shelter was not built");
    assert!(harvested_at.is_some(), "nothing was gathered");
    assert!(auto > 0, "the AutoPickup recorded no pile");
    assert!(stored_at.is_some(), "no gathered pile was picked up");
    assert!(
        first
            .iter()
            .any(|&e| !town.placed[e].alive || town.placed[e].removal_requested),
        "no wild plant was gathered"
    );
    assert_reservations_held(&town);
}

/// A forester's lodge near the start gets two foresters: they plant saplings on free
/// ground in the lodge's region and cut grown trees, whose logs are picked up.
#[test]
fn foresters_plant_and_cut() {
    let dir =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../game-data/original/WinData");
    let mut archive = Archive::default();
    for name in ["data1.pkg", "patch1.pkg", "patch0.pkg", "data0.pkg"] {
        let Ok(bytes) = std::fs::read(dir.join(name)) else {
            return;
        };
        archive.add(Package::parse(bytes).unwrap());
    }
    let world = crate::view::world::World::generate(
        &archive,
        &crate::view::world::WorldParams {
            seed: 1234,
            ..Default::default()
        },
    )
    .unwrap();
    let terrain = HeightGrid {
        width: world.terrain.width(),
        heights: &world.terrain.heights,
    };
    let mut rng = Random::new(1234);
    let mut town = StartingTown::create(
        &archive,
        "Game/StartCondition/Medium.rsc:resource",
        world.terrain.size,
        &terrain,
        &mut rng,
    )
    .unwrap();
    let mut tool = PlacementTool::start(
        &archive,
        &mut town,
        r"Template\ForesterLodge.rsc:resource",
        &mut rng,
    )
    .unwrap();
    let spot = tool.find_clear_spot(&town, &terrain, town.start);
    tool.hover(&town, &terrain, spot, 0);
    let id = tool.place(&archive, &mut town, 0).unwrap();
    let st = town.construction.sites[&id].template;
    let tree = town.construction.templates[st]
        .forester
        .clone()
        .expect("no ForesterDescription")
        .tree;
    let p = &town.construction.professions;
    let builder = p.builder;
    let forester = p
        .list
        .iter()
        .position(|p| p.reference.to_ascii_lowercase().ends_with(":forester"))
        .expect("no forester profession");
    town.construction
        .commands
        .push(unbanished_game::work::professions::Command::SetCount {
            profession: builder,
            count: 2,
        });
    let mut warnings = unbanished_game::events::StockWarnings::default();
    warnings.limits[0] = 100_000;
    let saplings = |town: &StartingTown| {
        town.placed
            .iter()
            .filter(|p| p.alive && p.template == tree && p.growth < 0.05)
            .count()
    };
    let dt = 1.0 / 30.0;
    let (mut built_at, mut planted_at, mut cut_at, mut stored_at) = (None, None, None, None);
    let mut base = 0;
    let mut seen = std::collections::BTreeSet::new();
    for step in 0..30 * 60 * 15 {
        town.apply_commands(&terrain, &mut rng);
        town.iterate(&terrain, &mut rng, dt, 50.0);
        let stock = std::array::from_fn(|i| (warnings.count[i], warnings.limits[i]));
        town.update_citizens(&terrain, &mut rng, dt, 50.0, stock);
        warnings.recompute(&town.registered_storages(), &town.materials);
        town.construction
            .jobs
            .step(unbanished_game::scheduler::ticks(dt));
        if built_at.is_none() && town.construction.sites[&id].complete {
            built_at = Some(step);
            base = saplings(&town);
            town.construction.commands.push(
                unbanished_game::work::professions::Command::SetCount {
                    profession: forester,
                    count: 2,
                },
            );
        }
        if built_at.is_some() && planted_at.is_none() && saplings(&town) > base {
            planted_at = Some(step);
        }
        let piles: Vec<usize> = town
            .construction
            .piles
            .iter()
            .filter(|(_, p)| p.auto_pickup == Some(id))
            .map(|(&e, _)| e)
            .collect();
        if cut_at.is_none() && !piles.is_empty() {
            cut_at = Some(step);
        }
        seen.extend(piles.iter().copied());
        if stored_at.is_none() && seen.iter().any(|e| !piles.contains(e)) {
            stored_at = Some(step);
        }
        if planted_at.is_some() && stored_at.is_some() {
            break;
        }
    }
    eprintln!(
        "built {built_at:?}, planted {planted_at:?}, cut {cut_at:?}, stored {stored_at:?}, \
         toggles {:?}",
        town.construction.sites[&id].forester
    );
    assert!(built_at.is_some(), "the lodge was not built");
    assert!(planted_at.is_some(), "no sapling was planted");
    assert!(cut_at.is_some(), "no tree was cut");
    assert!(stored_at.is_some(), "no log pile was picked up");
    assert_reservations_held(&town);
}

/// A fisherman's dock on the stream near the start: once built it counts on map layer 8
/// with its share of water, and two fishermen land fish on its create points.
#[test]
fn fishermen_land_fish() {
    let dir =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../game-data/original/WinData");
    let mut archive = Archive::default();
    for name in ["data1.pkg", "patch1.pkg", "patch0.pkg", "data0.pkg"] {
        let Ok(bytes) = std::fs::read(dir.join(name)) else {
            return;
        };
        archive.add(Package::parse(bytes).unwrap());
    }
    let world = crate::view::world::World::generate(
        &archive,
        &crate::view::world::WorldParams {
            seed: 1234,
            ..Default::default()
        },
    )
    .unwrap();
    let terrain = HeightGrid {
        width: world.terrain.width(),
        heights: &world.terrain.heights,
    };
    let mut rng = Random::new(1234);
    let mut town = StartingTown::create(
        &archive,
        "Game/StartCondition/Medium.rsc:resource",
        world.terrain.size,
        &terrain,
        &mut rng,
    )
    .unwrap();
    let mut tool = PlacementTool::start(
        &archive,
        &mut town,
        r"Template\FishermansDock.rsc:resource",
        &mut rng,
    )
    .unwrap();
    let spot = tool.find_clear_spot(&town, &terrain, town.start);
    assert!(spot.is_some(), "no shore near the start");
    tool.hover(&town, &terrain, spot, 0);
    let id = tool.place(&archive, &mut town, 0).unwrap();
    let p = &town.construction.professions;
    let builder = p.builder;
    let fisherman = p
        .list
        .iter()
        .position(|p| p.reference.to_ascii_lowercase().ends_with(":fisherman"))
        .expect("no fisherman profession");
    town.construction
        .commands
        .push(unbanished_game::work::professions::Command::SetCount {
            profession: builder,
            count: 2,
        });
    let fish = town
        .materials
        .iter()
        .position(|m| m.reference.contains("Fish"))
        .expect("no fish material");
    let mut warnings = unbanished_game::events::StockWarnings::default();
    warnings.limits[5] = 100_000;
    let dt = 1.0 / 30.0;
    let (mut built_at, mut landed_at) = (None, None);
    for step in 0..30 * 60 * 15 {
        town.apply_commands(&terrain, &mut rng);
        town.iterate(&terrain, &mut rng, dt, 50.0);
        let stock = std::array::from_fn(|i| (warnings.count[i], warnings.limits[i]));
        town.update_citizens(&terrain, &mut rng, dt, 50.0, stock);
        warnings.recompute(&town.registered_storages(), &town.materials);
        town.construction
            .jobs
            .step(unbanished_game::scheduler::ticks(dt));
        if built_at.is_none() && town.construction.sites[&id].complete {
            built_at = Some(step);
            town.construction.commands.push(
                unbanished_game::work::professions::Command::SetCount {
                    profession: fisherman,
                    count: 2,
                },
            );
        }
        if town.construction.piles.values().any(|p| p.material == fish) {
            landed_at = Some(step);
            break;
        }
    }
    let f = town.construction.sites[&id].fishing.unwrap();
    eprintln!("built {built_at:?}, landed {landed_at:?}, dock {f:?}");
    assert!(built_at.is_some(), "the dock was not built");
    assert!(f.registered && f.water > 0.0, "{f:?}");
    assert_eq!(
        town.cells.layer_totals[unbanished_game::production::fishing::LAYER],
        1
    );
    assert!(landed_at.is_some(), "no fish was landed");
    assert_reservations_held(&town);
}
