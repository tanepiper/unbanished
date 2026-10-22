//! Building: clearing the site, building a house and a storage yard.

use super::*;

/// A wood house placed on clear ground near the start is supplied by laborers and built
/// by two builders: the town's logs and stone drop by 16 and 8, the final mesh shows and
/// the map is stamped.
#[test]
fn wood_house_gets_built() {
    let dir =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../game-data/original/WinData");
    let mut archive = Archive::default();
    for name in ["data1.pkg", "patch1.pkg", "patch0.pkg", "data0.pkg"] {
        let Ok(bytes) = std::fs::read(dir.join(name)) else {
            eprintln!("game data missing; skipping construction integration");
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
        r"Template\WoodHouse.rsc:resource",
        &mut rng,
    )
    .unwrap();
    let n = town.cells.size as i32;
    let spot = tool.find_clear_spot(&town, &terrain, town.start);
    let spot = spot.expect("no clear spot for a house");
    tool.hover(&town, &terrain, Some(spot), 0);
    let before = stock_table(&town.registered_storages(), &town.materials);
    let id = tool.place(&archive, &mut town, 0).unwrap();
    let builder = town.construction.professions.builder;
    town.construction
        .commands
        .push(unbanished_game::work::professions::Command::SetCount {
            profession: builder,
            count: 2,
        });
    let mut warnings = unbanished_game::events::StockWarnings::default();
    let dt = 1.0 / 30.0;
    let mut done_at = None;
    for step in 0..30 * 60 * 8 {
        town.apply_commands(&terrain, &mut rng);
        town.iterate(&terrain, &mut rng, dt, 50.0);
        let stock = std::array::from_fn(|i| (warnings.count[i], warnings.limits[i]));
        town.update_citizens(&terrain, &mut rng, dt, 50.0, stock);
        warnings.recompute(&town.registered_storages(), &town.materials);
        town.construction
            .jobs
            .step(unbanished_game::scheduler::ticks(dt));
        if step == 1 {
            let (counts, capacity) = town.profession_counts();
            assert_eq!(counts[builder], 2);
            let points = town.templates[town.placed[id].template].build_points.len();
            assert!(points > 0);
            assert_eq!(capacity[builder], points as i32);
        }
        if town.construction.sites[&id].complete {
            done_at = Some(step);
            break;
        }
    }
    let site = &town.construction.sites[&id];
    eprintln!(
        "state {} progress {} done at {:?}",
        site.machine.state, site.progress, done_at
    );
    assert!(done_at.is_some(), "the house was not built");
    let after = stock_table(&town.registered_storages(), &town.materials);
    assert_eq!(before[0] - after[0], 16, "logs");
    assert_eq!(before[1] - after[1], 8, "stone");
    let p = &town.placed[id];
    assert_eq!(p.mesh, 2);
    let stamped = (p.cell.1..p.cell.1 + p.footprint.1)
        .flat_map(|y| (p.cell.0..p.cell.0 + p.footprint.0).map(move |x| (y * n + x) as usize))
        .filter(|&i| town.cells.objects[i] == Some(id))
        .count();
    assert!(stamped > 0, "footprint not stamped");
    assert!(town.construction.jobs.is_empty());
    // Builders keep their profession without a workplace once the site is done.
    assert_eq!(town.profession_counts().0[builder], 2);
    assert!(town.agents.states.iter().all(|s| s.workplace.is_none()));
    assert_reservations_held(&town);
}

/// A wood house placed over natural resources: laborers fell them (the site clears), the
/// piles they leave are hauled to storage, then the house is supplied and built.
#[test]
fn wood_house_on_resources_gets_cleared_and_built() {
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
        r"Template\WoodHouse.rsc:resource",
        &mut rng,
    )
    .unwrap();
    let spot = tool
        .find_spot(&town, &terrain, town.start, true)
        .expect("no spot with resources");
    tool.hover(&town, &terrain, Some(spot), 0);
    let rect = tool.rect.unwrap();
    let n = town.cells.size as i32;
    let resources: Vec<usize> = (rect.y0..=rect.y1)
        .flat_map(|y| (rect.x0..=rect.x1).map(move |x| (y * n + x) as usize))
        .filter_map(|i| town.cells.objects[i])
        .collect();
    assert!(!resources.is_empty());
    let id = tool.place(&archive, &mut town, 0).unwrap();
    let builder = town.construction.professions.builder;
    town.construction
        .commands
        .push(unbanished_game::work::professions::Command::SetCount {
            profession: builder,
            count: 2,
        });
    let mut warnings = unbanished_game::events::StockWarnings::default();
    let dt = 1.0 / 30.0;
    let (mut cleared_at, mut done_at, mut most_piles) = (None, None, 0);
    for step in 0..30 * 60 * 12 {
        town.apply_commands(&terrain, &mut rng);
        town.iterate(&terrain, &mut rng, dt, 50.0);
        let stock = std::array::from_fn(|i| (warnings.count[i], warnings.limits[i]));
        town.update_citizens(&terrain, &mut rng, dt, 50.0, stock);
        warnings.recompute(&town.registered_storages(), &town.materials);
        town.construction
            .jobs
            .step(unbanished_game::scheduler::ticks(dt));
        most_piles = most_piles.max(town.construction.piles.len());
        if cleared_at.is_none() && resources.iter().all(|&r| !town.placed[r].alive) {
            cleared_at = Some(step);
        }
        if town.construction.sites[&id].complete {
            done_at = Some(step);
            break;
        }
    }
    eprintln!(
        "resources {} cleared at {:?}, most piles {}, built at {:?}, piles left {}",
        resources.len(),
        cleared_at,
        most_piles,
        done_at,
        town.construction.piles.len()
    );
    assert!(cleared_at.is_some(), "the footprint was not cleared");
    assert!(most_piles > 0, "no piles were left");
    assert!(done_at.is_some(), "the house was not built");
    assert!(town.construction.claimed_resources.is_empty());
}

/// A site whose clear-area description has the instant flag (`+0x2d`, set by mods) empties
/// its footprint when clearing starts (`FUN_14011d230`): the natural resources leave their
/// cells at once and are removed, and no clearing work is left.
#[test]
fn an_instant_clear_empties_the_footprint() {
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
        r"Template\WoodHouse.rsc:resource",
        &mut rng,
    )
    .unwrap();
    let spot = tool
        .find_spot(&town, &terrain, town.start, true)
        .expect("no spot with resources");
    tool.hover(&town, &terrain, Some(spot), 0);
    let rect = tool.rect.unwrap();
    let n = town.cells.size as i32;
    let cells: Vec<usize> = (rect.y0..=rect.y1)
        .flat_map(|y| (rect.x0..=rect.x1).map(move |x| (y * n + x) as usize))
        .collect();
    let resources: Vec<usize> = cells
        .iter()
        .filter_map(|&i| town.cells.objects[i])
        .collect();
    assert!(!resources.is_empty());
    let id = tool.place(&archive, &mut town, 0).unwrap();
    let template = town.construction.sites[&id].template;
    town.construction.templates[template]
        .clear
        .as_mut()
        .expect("the house clears its area")
        .instant = true;
    let dt = 1.0 / 30.0;
    for _ in 0..30 {
        town.apply_commands(&terrain, &mut rng);
        town.iterate(&terrain, &mut rng, dt, 50.0);
        town.construction
            .jobs
            .step(unbanished_game::scheduler::ticks(dt));
    }
    assert!(
        resources.iter().all(|&r| !town.placed[r].alive),
        "the resources are still standing"
    );
    assert!(cells
        .iter()
        .all(|&i| town.cells.objects[i].is_none_or(|o| o == id)));
    assert!(town.construction.claimed_resources.is_empty());
    assert!(
        town.construction.sites[&id].machine.state != 0,
        "still clearing"
    );
}

/// A storage yard placed over trees: laborers fell them, the logs left on the yard go
/// straight into its storage, and once clear the yard joins the town's storages.
#[test]
fn a_storage_yard_absorbs_its_logs_and_joins_the_town() {
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
    let n = town.cells.size as i32;
    // A 5 × 5 patch near the start with a few trees and no buildings.
    let trees = |town: &StartingTown, x: i32, y: i32| {
        let mut count = 0;
        for cy in y..y + 5 {
            for cx in x..x + 5 {
                if n <= cx || n <= cy {
                    return None;
                }
                let i = (cy * n + cx) as usize;
                if town.cells.occupants[i].is_some() {
                    return None;
                }
                match town.cells.objects[i] {
                    None if town.cells.types[i] == unbanished_game::terrain::CellType::GROUND => {}
                    Some(o)
                        if town.templates[town.placed[o].template]
                            .reference
                            .contains("Tree") =>
                    {
                        count += 1
                    }
                    _ => return None,
                }
            }
        }
        Some(count)
    };
    let (sx, sy) = town.start;
    let rect = (6..80)
        .flat_map(|r| (-r..=r).flat_map(move |d| [(d, -r), (d, r), (-r, d), (r, d)]))
        .map(|(dx, dy)| (sx + dx, sy + dy))
        .find(|&(x, y)| x > 0 && y > 0 && trees(&town, x, y).is_some_and(|t| 3 <= t))
        .expect("no patch of trees");
    let before = town.storage_sites.len();
    let id = town
        .place_area(
            &archive,
            r"Template\StorageYard.rsc:resource",
            (rect.0, rect.1, 5, 5),
            &terrain,
        )
        .expect("the yard was not placed");
    let storage = town.construction.sites[&id].storage;
    let mut warnings = unbanished_game::events::StockWarnings::default();
    let dt = 1.0 / 30.0;
    let (mut absorbed_at, mut joined_at) = (None, None);
    for step in 0..30 * 60 * 15 {
        town.apply_commands(&terrain, &mut rng);
        town.iterate(&terrain, &mut rng, dt, 60.0);
        let stock = std::array::from_fn(|i| (warnings.count[i], warnings.limits[i]));
        town.update_citizens(&terrain, &mut rng, dt, 60.0, stock);
        warnings.recompute(&town.registered_storages(), &town.materials);
        town.construction
            .jobs
            .step(unbanished_game::scheduler::ticks(dt));
        if absorbed_at.is_none() && !town.storages[storage].items.is_empty() {
            absorbed_at = Some(step);
        }
        if joined_at.is_none() && town.storage_sites.iter().any(|s| s.entity == id) {
            joined_at = Some(step);
        }
        if absorbed_at.is_some() && joined_at.is_some() {
            break;
        }
    }
    eprintln!(
        "absorbed {absorbed_at:?}, joined {joined_at:?}, state {}, items {:?}",
        town.construction.sites[&id].machine.state, town.storages[storage].items
    );
    assert!(absorbed_at.is_some(), "no log went into the yard");
    assert!(
        joined_at.is_some(),
        "the yard never joined the town's storages"
    );
    assert_eq!(town.storage_sites.len(), before + 1);
}
