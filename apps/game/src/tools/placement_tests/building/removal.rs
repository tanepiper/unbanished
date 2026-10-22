//! Removing buildings: clearing, tearing down and calling a demolition off.

use super::*;

/// A finished house nobody has moved into, holding food, gets a ClearStorage work and
/// laborers carry the food to the town's storage.
#[test]
fn an_empty_house_is_cleared() {
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
        .find_clear_spot(&town, &terrain, town.start)
        .expect("no spot");
    tool.hover(&town, &terrain, Some(spot), 0);
    let id = tool.place(&archive, &mut town, 0).unwrap();
    let builder = town.construction.professions.builder;
    town.construction
        .commands
        .push(unbanished_game::work::professions::Command::SetCount {
            profession: builder,
            count: 2,
        });
    // Food the house can hold.
    let food = (0..town.materials.len())
        .find(|&m| unbanished_game::work::stock::stock_slot(town.materials[m].flags) == Some(5))
        .expect("no food material");
    let mut warnings = unbanished_game::events::StockWarnings::default();
    let dt = 1.0 / 30.0;
    let (mut stocked, mut registered, mut cleared) = (false, false, false);
    for _ in 0..30 * 60 * 15 {
        town.apply_commands(&terrain, &mut rng);
        town.iterate(&terrain, &mut rng, dt, 50.0);
        let stock = std::array::from_fn(|i| (warnings.count[i], warnings.limits[i]));
        town.update_citizens(&terrain, &mut rng, dt, 50.0, stock);
        warnings.recompute(&town.registered_storages(), &town.materials);
        town.construction
            .jobs
            .step(unbanished_game::scheduler::ticks(dt));
        let site = &town.construction.sites[&id];
        let storage = site.storage;
        if !stocked && site.complete && town.construction.homes.contains_key(&id) {
            stocked = true;
            // Nobody may live there while the test watches: the house leaves the
            // residences and anyone who moved in leaves it.
            town.construction.workforce.residences.retain(|&r| r != id);
            let home = town.construction.homes.get_mut(&id).unwrap();
            for c in home.families.iter_mut().flat_map(std::mem::take) {
                town.agents.states[c].home = None;
                town.agents.states[c].home_slot = -1;
            }
            town.storages[storage].add(food, 5);
        }
        if stocked && !registered {
            let w = town.construction.sites[&id].work_id;
            registered = matches!(
                town.construction.jobs.works.get(&w).map(|w| &w.kind),
                Some(unbanished_game::work::jobs::WorkKind::ClearStorage(_))
            );
        }
        if stocked && town.storages[storage].items.iter().all(|i| i.count == 0) {
            cleared = true;
            break;
        }
    }
    assert!(stocked, "the house was not finished empty");
    assert!(registered, "no ClearStorage work");
    assert!(cleared, "the food stayed in the house");
}

/// A finished, lived-in wood house is marked with Remove Buildings: its family moves out,
/// its goods are carried away, builders take it down and it leaves part of its materials
/// in piles on its footprint.
#[test]
fn a_house_is_torn_down() {
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
        .find_clear_spot(&town, &terrain, town.start)
        .expect("no spot");
    tool.hover(&town, &terrain, Some(spot), 0);
    let id = tool.place(&archive, &mut town, 0).unwrap();
    let builder = town.construction.professions.builder;
    town.construction
        .commands
        .push(unbanished_game::work::professions::Command::SetCount {
            profession: builder,
            count: 2,
        });
    let mut removal = crate::tools::removal_tool::RemovalTool::start(
        &archive,
        &mut town,
        r"Template\ClearBuildings.rsc:resource",
    )
    .expect("no Remove Buildings tool");
    let n = town.cells.size as i32;
    let (x, y) = town.placed[id].cell;
    let (w, h) = town.placed[id].footprint;
    let mut warnings = unbanished_game::events::StockWarnings::default();
    let dt = 1.0 / 30.0;
    let (mut marked, mut residents) = (false, Vec::new());
    let (mut states, mut gone) = (Vec::new(), false);
    let mut deconstructed = false;
    for _ in 0..30 * 60 * 30 {
        town.apply_commands(&terrain, &mut rng);
        town.iterate(&terrain, &mut rng, dt, 50.0);
        let stock = std::array::from_fn(|i| (warnings.count[i], warnings.limits[i]));
        town.update_citizens(&terrain, &mut rng, dt, 50.0, stock);
        warnings.recompute(&town.registered_storages(), &town.materials);
        town.construction
            .jobs
            .step(unbanished_game::scheduler::ticks(dt));
        let Some(site) = town.construction.sites.get(&id) else {
            gone = true;
            break;
        };
        let state = site.machine.state;
        if states.last() != Some(&state) {
            states.push(state);
        }
        let w = site.work_id;
        if let Some(unbanished_game::work::jobs::WorkKind::Build(b)) =
            town.construction.jobs.works.get(&w).map(|w| &w.kind)
        {
            deconstructed |= b.destroy && !b.workers.is_empty();
        }
        if !marked
            && town
                .construction
                .homes
                .get(&id)
                .is_some_and(|h| h.occupants() > 0)
        {
            marked = true;
            residents = town.construction.homes[&id]
                .families
                .iter()
                .flatten()
                .copied()
                .collect();
            let cell = (y + h / 2) * n + x + w / 2;
            removal.drag(&archive, &mut town, &terrain, cell, cell);
        }
    }
    assert!(marked, "nobody moved in: {states:?}");
    assert!(gone, "the house still stands: {states:?}");
    assert!(states.contains(&5), "{states:?}");
    assert!(!town.placed[id].alive);
    assert!(!town.construction.workforce.workplaces.contains_key(&id));
    assert!(!town.construction.workforce.residences.contains(&id));
    for c in residents {
        assert_ne!(town.agents.states[c].home, Some(id));
    }
    for cy in y..y + h {
        for cx in x..x + w {
            let i = (cy * n + cx) as usize;
            assert_ne!(town.cells.occupants[i], Some(id));
            assert_ne!(town.cells.objects[i], Some(id));
        }
    }
    let piles: Vec<(usize, i32)> = town
        .construction
        .piles
        .iter()
        .filter(|(&p, _)| {
            let (px, py) = town.placed[p].cell;
            (x..x + w).contains(&px) && (y..y + h).contains(&py)
        })
        .map(|(_, p)| (p.material, p.count))
        .collect();
    let heights: Vec<f32> = (y..y + h)
        .flat_map(|cy| (x..x + w).map(move |cx| (cx, cy)))
        .map(|(cx, cy)| {
            unbanished_game::town::cell_ground(town.cells.size, &terrain, cy * n + cx)[2]
        })
        .collect();
    // Half the 16 logs and 8 stone, where the cells are flat enough.
    assert!(deconstructed, "no builder took the house down");
    assert!(!piles.is_empty(), "no refund on {heights:?}");
    assert!(piles.iter().map(|p| p.1).sum::<i32>() <= 12, "{piles:?}");
}

/// A house marked with Remove Buildings while being built, then Remove Cancel: it goes
/// back to being built and is finished.
#[test]
fn a_demolition_is_called_off() {
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
        .find_clear_spot(&town, &terrain, town.start)
        .expect("no spot");
    tool.hover(&town, &terrain, Some(spot), 0);
    let id = tool.place(&archive, &mut town, 0).unwrap();
    let builder = town.construction.professions.builder;
    town.construction
        .commands
        .push(unbanished_game::work::professions::Command::SetCount {
            profession: builder,
            count: 2,
        });
    let mut remove = crate::tools::removal_tool::RemovalTool::start(
        &archive,
        &mut town,
        r"Template\ClearBuildings.rsc:resource",
    )
    .unwrap();
    let mut cancel = crate::tools::removal_tool::RemovalTool::start(
        &archive,
        &mut town,
        r"Template\ClearCancel.rsc:resource",
    )
    .unwrap();
    let n = town.cells.size as i32;
    let (x, y) = town.placed[id].cell;
    let (w, h) = town.placed[id].footprint;
    let cell = (y + h / 2) * n + x + w / 2;
    let mut warnings = unbanished_game::events::StockWarnings::default();
    let dt = 1.0 / 30.0;
    let (mut marked, mut called_off) = (false, false);
    let mut states = Vec::new();
    for _ in 0..30 * 60 * 20 {
        town.apply_commands(&terrain, &mut rng);
        town.iterate(&terrain, &mut rng, dt, 50.0);
        let stock = std::array::from_fn(|i| (warnings.count[i], warnings.limits[i]));
        town.update_citizens(&terrain, &mut rng, dt, 50.0, stock);
        warnings.recompute(&town.registered_storages(), &town.materials);
        town.construction
            .jobs
            .step(unbanished_game::scheduler::ticks(dt));
        let site = &town.construction.sites[&id];
        let state = site.machine.state;
        if states.last() != Some(&state) {
            states.push(state);
        }
        if !marked && state == 8 && 0 < site.progress {
            marked = true;
            remove.drag(&archive, &mut town, &terrain, cell, cell);
        } else if marked && !called_off && state == 5 {
            called_off = true;
            cancel.drag(&archive, &mut town, &terrain, cell, cell);
        }
        if called_off && state == 2 {
            break;
        }
    }
    assert!(called_off, "{states:?}");
    assert_eq!(
        states.last(),
        Some(&2),
        "not finished after the cancel: {states:?}"
    );
    let site = &town.construction.sites[&id];
    assert!(site.complete);
}
