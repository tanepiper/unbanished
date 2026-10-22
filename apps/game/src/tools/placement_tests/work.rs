//! Workplaces: workshops, the cutter yard, wells, markets, and the professions' commute.

use super::*;

/// The blacksmith, tailor and tavern share the cutter yard's ConsumeProduce code: with
/// every recipe's inputs in the town's storage, two workers fetch them and make the current
/// recipe's goods, which the building's tracker counts.
#[test]
fn workshops_make_their_goods() {
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
    for template in [
        r"Template\ToolMaker.rsc:resource",
        r"Template\Tailor.rsc:resource",
        r"Template\Tavern.rsc:resource",
    ] {
        let mut rng = Random::new(1234);
        let mut town = StartingTown::create(
            &archive,
            "Game/StartCondition/Medium.rsc:resource",
            world.terrain.size,
            &terrain,
            &mut rng,
        )
        .unwrap();
        let mut tool = PlacementTool::start(&archive, &mut town, template, &mut rng).unwrap();
        let spot = tool.find_clear_spot(&town, &terrain, town.start);
        tool.hover(&town, &terrain, spot, 0);
        let id = tool.place(&archive, &mut town, 0).unwrap();
        let builder = town.construction.professions.builder;
        town.construction
            .commands
            .push(unbanished_game::work::professions::Command::SetCount {
                profession: builder,
                count: 3,
            });
        let site_template = town.construction.sites[&id].template;
        let recipes = town.construction.templates[site_template]
            .produce
            .clone()
            .expect("no ConsumeProduceDescription");
        let barn = town.storage_sites[0].storage;
        for r in &recipes.recipes {
            for &(m, need) in &r.inputs {
                town.storages[barn].add(m, 2 * need);
            }
        }
        let outputs: Vec<usize> = recipes.recipes.iter().map(|r| r.output_material).collect();
        let mut warnings = unbanished_game::events::StockWarnings::default();
        let dt = 1.0 / 30.0;
        let mut made = 0;
        let mut built = false;
        for _ in 0..30 * 60 * 15 {
            town.apply_commands(&terrain, &mut rng);
            town.iterate(&terrain, &mut rng, dt, 50.0);
            let stock = std::array::from_fn(|i| (warnings.count[i], warnings.limits[i]));
            town.update_citizens(&terrain, &mut rng, dt, 50.0, stock);
            warnings.recompute(&town.registered_storages(), &town.materials);
            town.construction
                .jobs
                .step(unbanished_game::scheduler::ticks(dt));
            if !built && town.construction.sites[&id].complete {
                built = true;
                let profession = town.construction.workforce.workplaces[&id]
                    .description_profession
                    .expect("no profession");
                town.construction.commands.push(
                    unbanished_game::work::professions::Command::SetCount {
                        profession,
                        count: 2,
                    },
                );
            }
            made = town.construction.trackers.get(&id).map_or(0, |t| {
                t.this_year
                    .iter()
                    .filter(|e| outputs.contains(&e.0))
                    .map(|e| e.1)
                    .sum()
            });
            if 0 < made {
                break;
            }
        }
        assert!(built, "{template} was not built");
        assert!(0 < made, "{template} made nothing");
    }
}

/// The commute optimisation (StockManager every 300 s): with a cutter yard worked and the
/// families housed, a run deals the jobs again: every profession keeps its count and every
/// workplace its workers, and the job ends idle with its clock wrapped.
#[test]
fn the_commute_run_keeps_the_professions() {
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
    let mut ids = Vec::new();
    for template in [
        r"Template\CutterYard.rsc:resource",
        r"Template\WoodHouse.rsc:resource",
        r"Template\WoodHouse.rsc:resource",
    ] {
        let mut tool = PlacementTool::start(&archive, &mut town, template, &mut rng).unwrap();
        let spot = tool.find_clear_spot(&town, &terrain, town.start);
        tool.hover(&town, &terrain, spot, 0);
        ids.push(tool.place(&archive, &mut town, 0).unwrap());
    }
    let p = &town.construction.professions;
    let builder = p.builder;
    let cutter = p
        .list
        .iter()
        .position(|p| p.reference.to_ascii_lowercase().ends_with(":cutter"))
        .unwrap();
    town.construction
        .commands
        .push(unbanished_game::work::professions::Command::SetCount {
            profession: builder,
            count: 4,
        });
    let mut warnings = unbanished_game::events::StockWarnings::default();
    let dt = 1.0 / 30.0;
    let mut staffed = false;
    for _ in 0..30 * 60 * 15 {
        town.apply_commands(&terrain, &mut rng);
        town.iterate(&terrain, &mut rng, dt, 50.0);
        let stock = std::array::from_fn(|i| (warnings.count[i], warnings.limits[i]));
        town.update_citizens(&terrain, &mut rng, dt, 50.0, stock);
        warnings.recompute(&town.registered_storages(), &town.materials);
        town.construction
            .jobs
            .step(unbanished_game::scheduler::ticks(dt));
        if !staffed && town.construction.sites[&ids[0]].complete {
            staffed = true;
            town.construction.commands.push(
                unbanished_game::work::professions::Command::SetCount {
                    profession: cutter,
                    count: 2,
                },
            );
        }
        if ids.iter().all(|e| town.construction.sites[e].complete)
            && town
                .construction
                .workforce
                .workplaces
                .get(&ids[0])
                .is_some_and(|w| !w.workers.is_empty())
        {
            break;
        }
    }
    assert!(ids.iter().all(|e| town.construction.sites[e].complete));
    let counts = |town: &StartingTown| {
        let mut m = std::collections::BTreeMap::new();
        for (c, s) in town.citizens.iter().zip(&town.agents.states) {
            if c.alive && c.age >= town.agents.rules().thresholds.adult_age && s.school.is_none() {
                *m.entry(s.profession).or_insert(0) += 1;
            }
        }
        m
    };
    let workers = |town: &StartingTown| -> Vec<(usize, usize)> {
        town.construction
            .workforce
            .workplaces
            .iter()
            .map(|(&e, w)| (e, w.workers.len()))
            .collect()
    };
    let (before, staff) = (counts(&town), workers(&town));
    assert!(!town.construction.workforce.workplaces[&ids[0]]
        .workers
        .is_empty());
    // 300 s, then the pairs seven an update.
    let mut ran = false;
    for _ in 0..30 * 330 {
        town.update_commute(&terrain, &mut rng, dt);
        let job = &town.construction.commute;
        ran |= 0 <= job.index;
        if ran && job.index < 0 {
            break;
        }
    }
    let job = &town.construction.commute;
    assert!(ran && job.index < 0 && job.pairs.is_empty() && !job.ranks.is_empty());
    assert!(job.timer <= unbanished_game::scheduler::ticks(300.0));
    assert_eq!(counts(&town), before);
    assert_eq!(workers(&town), staff);
    for (c, s) in town.agents.states.iter().enumerate() {
        if let Some(w) = s.workplace {
            assert!(town.construction.workforce.workplaces[&w]
                .workers
                .contains(&c));
        }
    }
}

/// A wood cutter near the start is built, two woodcutters take the ConsumeProduce work:
/// they fetch logs into the yard, work cycles and leave firewood piles that laborers haul to
/// storage.
#[test]
fn cutter_yard_makes_firewood() {
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
        r"Template\CutterYard.rsc:resource",
        &mut rng,
    )
    .unwrap();
    let spot = tool.find_clear_spot(&town, &terrain, town.start);
    tool.hover(&town, &terrain, spot, 0);
    let id = tool.place(&archive, &mut town, 0).unwrap();
    let p = &town.construction.professions;
    let builder = p.builder;
    let cutter = p
        .list
        .iter()
        .position(|p| p.reference.to_ascii_lowercase().ends_with(":cutter"))
        .expect("no cutter profession");
    town.construction
        .commands
        .push(unbanished_game::work::professions::Command::SetCount {
            profession: builder,
            count: 2,
        });
    let firewood = town
        .materials
        .iter()
        .position(|m| m.reference.contains("Firewood"));
    let mut warnings = unbanished_game::events::StockWarnings::default();
    // The Medium start holds the default firewood limit (200) already: raise it, as the
    // yard's limit control would.
    warnings.limits[3] = 1000;
    let dt = 1.0 / 30.0;
    let (mut built_at, mut first_pile, mut stored_at) = (None, None, None);
    let mut logs_before = None;
    let mut seen = std::collections::BTreeSet::new();
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
            let stocked = stock_table(&town.registered_storages(), &town.materials);
            logs_before = Some(stocked[0]);
            town.construction.commands.push(
                unbanished_game::work::professions::Command::SetCount {
                    profession: cutter,
                    count: 2,
                },
            );
        }
        let piles: Vec<usize> = town
            .construction
            .piles
            .iter()
            .filter(|(_, p)| Some(p.material) == firewood)
            .map(|(&e, _)| e)
            .collect();
        if first_pile.is_none() && !piles.is_empty() {
            first_pile = Some(step);
        }
        seen.extend(piles.iter().copied());
        if seen.iter().any(|e| !piles.contains(e)) {
            stored_at = Some(step);
            break;
        }
    }
    let site = &town.construction.sites[&id];
    eprintln!(
        "built {built_at:?}, first firewood pile {first_pile:?}, stored {stored_at:?}, \
         state {} produced {:?}, yard {:?}",
        site.machine.state,
        site.produce.as_ref().map(|p| p.produced),
        town.storages[site.storage].items
    );
    assert!(built_at.is_some(), "the yard was not built");
    assert!(first_pile.is_some(), "no firewood was made");
    assert!(stored_at.is_some(), "no firewood pile was hauled away");
    let after = stock_table(&town.registered_storages(), &town.materials);
    assert!(after[0] < logs_before.unwrap(), "no logs were used");
    // The yard's TrackerComponent counts the firewood it made this year.
    let made = town.construction.trackers[&id]
        .this_year
        .iter()
        .find(|e| Some(e.0) == firewood)
        .map_or(0, |e| e.1);
    assert!(0 < made, "the tracker counted no firewood");
    // The work button (ControlUI → `FUN_140130a70`): idle, then producing again.
    for want in [0xe, 0xd] {
        town.construction
            .commands
            .push(unbanished_game::work::professions::Command::Control {
                entity: id,
                kind: unbanished_game::work::control::kind::CONSUME_PRODUCE,
                index: 0,
            });
        town.apply_commands(&terrain, &mut rng);
        assert_eq!(town.construction.sites[&id].machine.state, want);
    }
}

/// A well near the start: once built, its HappinessComponent registers with the town and
/// marks layer 0 within its radius; the citizens' views see it as a loitering spot.
#[test]
fn a_well_registers_its_happiness() {
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
    let mut tool =
        PlacementTool::start(&archive, &mut town, r"Template\Well.rsc:resource", &mut rng).unwrap();
    let spot = tool.find_clear_spot(&town, &terrain, town.start);
    tool.hover(&town, &terrain, spot, 0);
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
    let mut registered_at = None;
    for step in 0..30 * 60 * 10 {
        town.apply_commands(&terrain, &mut rng);
        town.iterate(&terrain, &mut rng, dt, 50.0);
        let stock = std::array::from_fn(|i| (warnings.count[i], warnings.limits[i]));
        town.update_citizens(&terrain, &mut rng, dt, 50.0, stock);
        warnings.recompute(&town.registered_storages(), &town.materials);
        town.construction
            .jobs
            .step(unbanished_game::scheduler::ticks(dt));
        if town.construction.happiness.contains(&id) {
            registered_at = Some(step);
            break;
        }
    }
    eprintln!("registered at {registered_at:?}");
    assert!(registered_at.is_some(), "the well never registered");
    let st = town.construction.sites[&id].template;
    let radius = town.construction.templates[st].radius.unwrap();
    let p = &town.placed[id];
    let n = town.cells.size as i32;
    let centre = (p.cell.1 + (p.footprint.1 >> 1)) * n + p.cell.0 + (p.footprint.0 >> 1);
    assert_eq!(town.cells.layers[centre as usize][0], 1);
    assert_eq!(town.cells.layer_totals[0], 1);
    let far = centre + radius + 1;
    assert_eq!(town.cells.layers[far as usize][0], 0);
    let views = unbanished_game::behaviour::agents::entity_views(
        &town.placed,
        &town.templates,
        &town.construction,
    );
    assert_eq!(views[id].happiness, Some(2));
}

/// A market is built and its vendors stock it from the town's storages by category; it
/// joins the town's markets with its radius.
#[test]
fn vendors_stock_a_market() {
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
        "Game/StartCondition/Easy.rsc:resource",
        world.terrain.size,
        &terrain,
        &mut rng,
    )
    .unwrap();
    let mut tool = PlacementTool::start(
        &archive,
        &mut town,
        r"Template\Market.rsc:resource",
        &mut rng,
    )
    .unwrap();
    // Not west of the start: the river runs there.
    let (sx, sy) = town.start;
    let spot = (0..8)
        .flat_map(|k| {
            [
                (sx + 10 * k, sy),
                (sx + 10 * k, sy + 20),
                (sx + 10 * k, sy - 20),
            ]
        })
        .filter_map(|near| tool.find_spot(&town, &terrain, near, true))
        .find(|s| sx <= s.0)
        .expect("no spot");
    // Its trees are taken away first (the test's shortcut for clearing).
    tool.hover(&town, &terrain, Some(spot), 0);
    let rect = tool.rect.unwrap();
    let n = town.cells.size as i32;
    for y in rect.y0..=rect.y1 {
        for x in rect.x0..=rect.x1 {
            if let Some(o) = town.cells.objects[(y * n + x) as usize] {
                if !town.construction.destroy.contains(&o) {
                    town.construction.destroy.push(o);
                }
            }
        }
    }
    town.apply_commands(&terrain, &mut rng);
    tool.hover(&town, &terrain, Some(spot), 0);
    let id = tool.place(&archive, &mut town, 0).expect("not placed");
    let builder = town.construction.professions.builder;
    let vendor = town
        .construction
        .professions
        .list
        .iter()
        .position(|p| p.reference.to_ascii_lowercase().ends_with(":vendor"))
        .expect("no vendor profession");
    for (profession, count) in [(builder, 4), (vendor, 2)] {
        town.construction
            .commands
            .push(unbanished_game::work::professions::Command::SetCount { profession, count });
    }
    let mut warnings = unbanished_game::events::StockWarnings::default();
    let dt = 1.0 / 30.0;
    let (mut built_at, mut stocked_at) = (None, None);
    for step in 0..30 * 60 * 20 {
        town.apply_commands(&terrain, &mut rng);
        town.iterate(&terrain, &mut rng, dt, 50.0);
        let stock = std::array::from_fn(|i| (warnings.count[i], warnings.limits[i]));
        town.update_citizens(&terrain, &mut rng, dt, 50.0, stock);
        warnings.recompute(&town.registered_storages(), &town.materials);
        town.construction
            .jobs
            .step(unbanished_game::scheduler::ticks(dt));
        let site = &town.construction.sites[&id];
        if built_at.is_none() && site.complete {
            built_at = Some(step);
        }
        let storage = site.storage;
        if built_at.is_some() && town.storages[storage].items.iter().any(|i| i.count > 0) {
            stocked_at = Some(step);
            break;
        }
    }
    let market = town.storage_sites.iter().find(|s| s.entity == id);
    eprintln!(
        "built {built_at:?}, stocked {stocked_at:?}, market {:?}, items {:?}",
        market.map(|m| m.radius),
        town.storages[town.construction.sites[&id].storage].items
    );
    assert!(built_at.is_some(), "the market was not built");
    assert!(
        market.is_some_and(|m| m.radius.is_some()),
        "not a town market"
    );
    assert!(stocked_at.is_some(), "no goods reached the market");
}
