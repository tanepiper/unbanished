//! Upgrading a wood house to stone.

use super::*;

/// UpgradeUI: a built wood house is torn down and a stone house site takes its place.
#[test]
fn a_wood_house_is_upgraded_to_stone() {
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
    town.apply_commands(&terrain, &mut rng);
    // A start house (a built site).
    let house = town
        .construction
        .sites
        .keys()
        .copied()
        .find(|&e| {
            town.templates[town.placed[e].template]
                .reference
                .contains("WoodHouse")
        })
        .expect("no start house");
    let stone = town
        .load_template(&archive, r"Template\StoneHouse.rsc:resource")
        .unwrap();
    town.site_template(&archive, stone);
    town.construction
        .commands
        .push(unbanished_game::work::professions::Command::Upgrade {
            entity: house,
            template: stone,
        });
    let builder = town.construction.professions.builder;
    town.construction
        .commands
        .push(unbanished_game::work::professions::Command::SetCount {
            profession: builder,
            count: 4,
        });
    let cell = town.placed[house].cell;
    let mut warnings = unbanished_game::events::StockWarnings::default();
    let dt = 1.0 / 30.0;
    let mut replaced = None;
    for _ in 0..30 * 60 * 10 {
        town.apply_commands(&terrain, &mut rng);
        town.iterate(&terrain, &mut rng, dt, 50.0);
        let stock = std::array::from_fn(|i| (warnings.count[i], warnings.limits[i]));
        town.update_citizens(&terrain, &mut rng, dt, 50.0, stock);
        warnings.recompute(&town.registered_storages(), &town.materials);
        town.construction
            .jobs
            .step(unbanished_game::scheduler::ticks(dt));
        replaced = town
            .construction
            .sites
            .keys()
            .copied()
            .find(|&e| town.placed[e].template == stone && town.placed[e].cell == cell);
        if replaced.is_some() {
            break;
        }
    }
    assert!(
        replaced.is_some(),
        "no stone house took the wood house's place"
    );
    assert!(!town.construction.sites.contains_key(&house));
}
