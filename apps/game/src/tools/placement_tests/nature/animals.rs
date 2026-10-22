//! Deer herds and hunters.

use super::*;

/// The Medium start's deer population: herds of 5–10 deer where there is habitat; they
/// wander, and the herds keep their numbers.
#[test]
fn deer_herds_start_and_wander() {
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
    let w = &town.wildlife;
    let herds = w.herds.iter().filter(|h| !h.animals.is_empty()).count();
    eprintln!(
        "{} populations, {} herds ({} with deer), {} deer",
        w.populations.len(),
        w.herds.len(),
        herds,
        w.animals.len()
    );
    assert_eq!(w.populations.len(), 1);
    assert!(herds > 0 && w.animals.len() >= 3 * herds);
    // Herds start with `Rand(max − min) + min` deer; those already past their lifespan
    // die at once (`FUN_14010b8f0` with no time at activation).
    for h in &w.herds {
        assert!(h.animals.len() <= 9, "{}", h.animals.len());
    }
    let start: Vec<[f32; 3]> = w.animals.values().map(|a| a.character.position).collect();
    let dt = 1.0 / 30.0;
    let mut warnings = unbanished_game::events::StockWarnings::default();
    for _ in 0..30 * 60 {
        town.apply_commands(&terrain, &mut rng);
        town.iterate(&terrain, &mut rng, dt, 50.0);
        let stock = std::array::from_fn(|i| (warnings.count[i], warnings.limits[i]));
        town.update_citizens(&terrain, &mut rng, dt, 50.0, stock);
        warnings.recompute(&town.registered_storages(), &town.materials);
    }
    let moved = town
        .wildlife
        .animals
        .values()
        .zip(&start)
        .filter(|(a, p)| {
            let q = a.character.position;
            (q[0] - p[0]).hypot(q[1] - p[1]) > 1.0
        })
        .count();
    eprintln!("{moved} of {} deer moved", town.wildlife.animals.len());
    assert!(moved > 0, "no deer moved");
}

/// A hunter's lodge near the start gets two hunters: once the lodge has gathered its
/// hunting time, a hunter who comes near deer takes one and its venison is left there.
#[test]
fn hunters_take_deer() {
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
        r"Template\HunterLodge.rsc:resource",
        &mut rng,
    )
    .unwrap();
    let spot = tool.find_clear_spot(&town, &terrain, town.start);
    tool.hover(&town, &terrain, spot, 0);
    let id = tool.place(&archive, &mut town, 0).unwrap();
    let p = &town.construction.professions;
    let builder = p.builder;
    let hunter = p
        .list
        .iter()
        .position(|p| p.reference.to_ascii_lowercase().ends_with(":hunter"))
        .expect("no hunter profession");
    town.construction
        .commands
        .push(unbanished_game::work::professions::Command::SetCount {
            profession: builder,
            count: 2,
        });
    let venison = town
        .materials
        .iter()
        .position(|m| m.reference.contains("Venison"))
        .expect("no venison material");
    let mut warnings = unbanished_game::events::StockWarnings::default();
    warnings.limits[5] = 100_000;
    let dt = 1.0 / 30.0;
    let (mut built_at, mut ready_at, mut taken_at) = (None, None, None);
    for step in 0..30 * 60 * 60 {
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
                    profession: hunter,
                    count: 2,
                },
            );
        }
        // The hunting time grows while staffed (it drops to 0 when a deer is taken).
        let time = town.construction.sites[&id].hunt_time.unwrap_or(0);
        if ready_at.is_none() && time > 0 {
            ready_at = Some(step);
        }
        if town
            .construction
            .piles
            .values()
            .any(|p| p.material == venison)
        {
            taken_at = Some(step);
            break;
        }
    }
    let lodge = town.placed[id].position;
    let near = town
        .wildlife
        .animals
        .values()
        .map(|a| {
            let p = a.character.position;
            (p[0] - lodge[0]).hypot(p[1] - lodge[1])
        })
        .fold(f32::MAX, f32::min);
    eprintln!(
        "built {built_at:?}, ready {ready_at:?}, venison {taken_at:?}, {} deer, nearest {near}",
        town.wildlife.animals.len()
    );
    assert!(built_at.is_some(), "the lodge was not built");
    assert!(ready_at.is_some(), "no hunting time was gathered");
    assert!(taken_at.is_some(), "no deer was taken");
    assert_reservations_held(&town);
}
