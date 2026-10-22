//! Fire and the tornado.

use super::*;

/// A finished wood house catches fire: it is damaged, citizens around run buckets from the
/// nearest water until it is out, and it is left as rubble; rebuilding the rubble takes it
/// down and puts a new site of the house in its place.
#[test]
fn a_house_burns_and_is_rebuilt() {
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
    assert!(
        !town.construction.water.points.is_empty(),
        "no shore points"
    );
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
    let mut warnings = unbanished_game::events::StockWarnings::default();
    let dt = 1.0 / 30.0;
    let (mut lit, mut fought, mut buckets) = (false, false, 0);
    let (mut rubble, mut rebuilt) = (false, None);
    let mut damaged_look = false;
    let mut states = Vec::new();
    for _ in 0..30 * 60 * 40 {
        town.apply_commands(&terrain, &mut rng);
        town.iterate(&terrain, &mut rng, dt, 50.0);
        let stock = std::array::from_fn(|i| (warnings.count[i], warnings.limits[i]));
        town.update_citizens(&terrain, &mut rng, dt, 50.0, stock);
        warnings.recompute(&town.registered_storages(), &town.materials);
        town.construction
            .jobs
            .step(unbanished_game::scheduler::ticks(dt));
        let Some(site) = town.construction.sites.get_mut(&id) else {
            // Taken down: the new site stands in the old one's rectangle.
            let (x, y) = town.placed[id].cell;
            rebuilt = town
                .construction
                .sites
                .iter()
                .find(|(&e, s)| e != id && town.placed[e].cell == (x, y) && s.machine.state >= 0)
                .map(|(&e, _)| e);
            if rebuilt.is_some() {
                break;
            }
            continue;
        };
        let state = site.machine.state;
        if states.last() != Some(&state) {
            states.push(state);
        }
        if !lit && site.complete && state == 2 {
            // The ignition odds come due at the next check.
            lit = true;
            site.fire.as_mut().unwrap().timer = unbanished_game::scheduler::ticks(1.0e9);
        }
        if let Some(f) = &site.fire {
            buckets = buckets.max(f.buckets);
        }
        let looks_damaged = site.destroy.as_ref().is_some_and(|d| d.damaged_look);
        let w = site.work_id;
        if !damaged_look && looks_damaged {
            // The burning house's model takes the damaged material set (`+0x28`).
            damaged_look = true;
            let mesh = town.templates[town.placed[id].template]
                .model
                .as_ref()
                .and_then(|m| m.variants.get(town.placed[id].variant))
                .and_then(|v| v.meshes.get(town.placed[id].mesh))
                .cloned()
                .flatten()
                .unwrap();
            assert!(crate::view::town::TownView::new(&archive, &town)
                .model_instances(&town)
                .iter()
                .any(|(m, g, i, _)| *m == mesh && g.set == 1 && !i.is_empty()));
        }
        if let Some(unbanished_game::work::jobs::WorkKind::Fire(fw)) =
            town.construction.jobs.works.get(&w).map(|w| &w.kind)
        {
            fought |= !fw.workers.is_empty();
        }
        if lit && !rubble && state == 7 {
            rubble = true;
            assert_eq!(
                town.cells.layer_totals[unbanished_game::disasters::fire::LAYER],
                0
            );
            town.construction.commands.push(
                unbanished_game::work::professions::Command::StateMessage {
                    entity: id,
                    message: 0,
                },
            );
        }
    }
    assert!(lit, "the house was not finished: {states:?}");
    assert!(damaged_look, "the burning house kept its material");
    assert!(states.contains(&6), "it never burned: {states:?}");
    assert!(fought, "nobody fought the fire: {states:?}");
    assert!(
        rubble,
        "it was not left as rubble: {states:?}, {buckets} buckets"
    );
    let new = rebuilt.expect("the rubble was not rebuilt");
    assert_eq!(town.placed[new].template, town.placed[id].template);
    eprintln!("states {states:?}, {buckets} buckets");
}

/// A tornado steered across the starting town wrecks its buildings and trees, then winds
/// down and goes.
#[test]
fn a_tornado_crosses_the_town() {
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
    // A house built first.
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
    let house = tool.place(&archive, &mut town, 0).unwrap();
    let builder = town.construction.professions.builder;
    town.construction
        .commands
        .push(unbanished_game::work::professions::Command::SetCount {
            profession: builder,
            count: 2,
        });
    let mut warnings = unbanished_game::events::StockWarnings::default();
    let dt = 1.0 / 30.0;
    let mut step = |town: &mut StartingTown, rng: &mut Random| {
        town.apply_commands(&terrain, rng);
        town.iterate(&terrain, rng, dt, 50.0);
        let stock = std::array::from_fn(|i| (warnings.count[i], warnings.limits[i]));
        town.update_citizens(&terrain, rng, dt, 50.0, stock);
        warnings.recompute(&town.registered_storages(), &town.materials);
        town.construction
            .jobs
            .step(unbanished_game::scheduler::ticks(dt));
    };
    let mut built = false;
    for _ in 0..30 * 60 * 20 {
        step(&mut town, &mut rng);
        let s = &town.construction.sites[&house];
        if s.complete && s.machine.state == 2 {
            built = true;
            break;
        }
    }
    assert!(built, "the house was not built");
    let reference = world
        .environment
        .template
        .clone()
        .expect("no tornado template");
    let id = town
        .spawn_tornado(&archive, &reference, &terrain, &mut rng)
        .expect("no tornado");
    assert_eq!(
        town.construction.site_events.last(),
        Some(&(0x1e, 3, id)),
        "no tornado event"
    );
    assert_eq!(town.construction.tornadoes[&id].path.len(), 129);
    // Steered straight over the house, from 30 cells west to 30 east.
    let [hx, hy, _] = town.placed[house].position;
    {
        let t = town.construction.tornadoes.get_mut(&id).unwrap();
        t.path = (0..129)
            .map(|i| [hx - 30.0 + 60.0 * i as f32 / 128.0, hy, 0.0])
            .collect();
        t.target = t.path[0];
        t.index = 0;
        t.position = [hx - 30.0, hy, 0.0];
    }
    let entities = town.placed.iter().filter(|p| p.alive).count();
    let citizens = town.citizens.iter().filter(|c| c.alive).count();
    let mut gone = false;
    for _ in 0..30 * 120 {
        step(&mut town, &mut rng);
        if !town.construction.tornadoes.contains_key(&id) {
            gone = true;
            break;
        }
    }
    assert!(gone, "the tornado did not wind down");
    assert!(!town.placed[id].alive);
    let wrecked = town
        .construction
        .sites
        .get(&house)
        .and_then(|s| s.destroy.as_ref())
        .is_some_and(|d| d.damaged);
    let after = town.placed.iter().filter(|p| p.alive).count();
    let living = town.citizens.iter().filter(|c| c.alive).count();
    eprintln!("entities {entities} → {after}, citizens {citizens} → {living}");
    assert!(wrecked, "the house was not wrecked");
    assert!(after < entities, "nothing was flattened");
}
