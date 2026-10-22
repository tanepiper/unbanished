use super::placement_tool::*;
use crate::start::StartingTown;
use unbanished_formats::pkg::Archive;
use unbanished_formats::pkg::Package;
use unbanished_game::random::Random;
use unbanished_game::terrain::HeightGrid;
use unbanished_game::work::stock::stock_table;

mod building;
mod disasters;
mod farming;
mod nature;
mod people;
mod transport;
mod work;

/// Every reserved stored unit belongs to an action still queued (a family carrying goods
/// home once the house is finished): construction left none behind.
fn assert_reservations_held(town: &StartingTown) {
    let mut held = std::collections::BTreeMap::new();
    for ch in &town.agents.characters {
        for (storage, material, n) in ch.stack.pending_reservations() {
            *held.entry((storage, material)).or_insert(0) += n;
        }
    }
    for (si, s) in town.storages.iter().enumerate() {
        for e in &s.items {
            let h = held.get(&(si, e.material)).copied().unwrap_or(0);
            assert_eq!(
                e.reserved, h,
                "storage {si} material {} reserved",
                e.material
            );
        }
    }
}

/// A seed-1234 Medium town with its terrain and stream, after `setup`.
fn with_town(test: impl FnOnce(&Archive, &HeightGrid, &mut StartingTown, &mut Random)) {
    let dir =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../game-data/original/WinData");
    let mut archive = Archive::default();
    for name in ["data1.pkg", "patch1.pkg", "patch0.pkg", "data0.pkg"] {
        let Ok(bytes) = std::fs::read(dir.join(name)) else {
            eprintln!("game data missing; skipping life integration");
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
    test(&archive, &terrain, &mut town, &mut rng);
}

/// One simulation iteration as the gameplay loop runs it, at `temperature`.
fn step(town: &mut StartingTown, terrain: &HeightGrid, rng: &mut Random, temperature: f32) {
    let dt = 1.0 / 30.0;
    town.apply_commands(terrain, rng);
    town.iterate(terrain, rng, dt, temperature);
    let warnings = unbanished_game::events::StockWarnings::default();
    let stock = std::array::from_fn(|i| (warnings.count[i], warnings.limits[i]));
    town.update_citizens(terrain, rng, dt, temperature, stock);
    town.construction
        .jobs
        .step(unbanished_game::scheduler::ticks(dt));
}
