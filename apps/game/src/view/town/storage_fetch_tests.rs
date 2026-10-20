use super::*;
use unbanished_formats::pkg::Package;
use unbanished_game::{behaviour::ai::activity, work::stock::stock_table};

/// ShowTool (`FUN_14011c880`): a citizen's tool is drawn on its mesh's `tool` node, carried
/// goods as their pile model; nothing with −1.
#[test]
fn shown_tools_hang_on_the_tool_node() {
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
    let town_view = crate::view::town::TownView::new(&archive, &town);
    assert_eq!(town_view.tool_meshes.len(), 11);
    assert!(town_view.citizen_bones.iter().all(Option::is_some));
    let tools = |town: &StartingTown, name: &str| -> Vec<[f32; 16]> {
        town_view
            .model_instances(town)
            .into_iter()
            .filter(|(m, ..)| m.contains(name))
            .flat_map(|(_, _, i, _)| i.into_iter().map(|i| i.matrix))
            .collect()
    };
    assert!(tools(&town, "ToolAxe").is_empty());
    town.agents.states[0].shown_tool = 0;
    let axes = tools(&town, "ToolAxe");
    assert_eq!(axes.len(), 1);
    let at = town.citizens[0].position;
    let d = (axes[0][12] - at[0]).hypot(axes[0][13] - at[1]);
    assert!(
        d < 1.0 && 0.2 < axes[0][14] - at[2],
        "{:?} vs {at:?}",
        &axes[0][12..15]
    );
    // Carried logs: the log pile's mesh.
    let log = town
        .materials
        .iter()
        .position(|m| m.reference.contains("Wood"))
        .unwrap();
    town.agents.states[0].carried.add(log, 3);
    town.agents.states[0].shown_tool = -2;
    town_view.cache_carry_models(&town, &archive);
    let carried: Vec<String> = town_view
        .model_instances(&town)
        .into_iter()
        .map(|(m, ..)| m)
        .filter(|m| m.contains("RawMaterial") || m.contains("Wood") || m.contains("Log"))
        .collect();
    assert!(!carried.is_empty(), "nothing carried");
}

#[test]
fn new_citizens_equip_from_registered_storage_then_loiter() {
    let dir =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../game-data/original/WinData");
    let mut archive = Archive::default();
    for name in ["data1.pkg", "patch1.pkg", "patch0.pkg", "data0.pkg"] {
        let Ok(bytes) = std::fs::read(dir.join(name)) else {
            eprintln!("game data missing; skipping citizen fetch integration");
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
    assert!(!town.storage_sites.is_empty());
    let initial = stock_table(&town.storages, &town.materials);
    assert_eq!(&initial[..8], &[120, 60, 40, 200, 40, 1800, 0, 30]);
    let yard = town
        .storage_sites
        .iter()
        .find(|s| {
            town.templates[town.placed[s.entity].template]
                .reference
                .contains("StorageYard")
        })
        .unwrap();
    assert_eq!(town.placed[yard.entity].footprint, (4, 6));
    assert_eq!(town.storages[yard.storage].capacity, 6000);
    let positions: Vec<_> = town.citizens.iter().map(|c| c.position).collect();
    let adults = town
        .citizens
        .iter()
        .filter(|c| c.is_adult(&town.rules))
        .count();
    // Each iteration: the bucket pass (it visits the citizens), then the citizens.
    for _ in 0..3600 {
        town.iterate(&terrain, &mut rng, 1.0 / 30.0, 0.0);
        town.update_citizens(&terrain, &mut rng, 1.0 / 30.0, 0.0, [(1000, 0); 21]);
    }
    let final_stock = stock_table(&town.storages, &town.materials);
    assert_eq!(initial[4] - final_stock[4], adults as i32);
    assert_eq!(initial[7] - final_stock[7], town.citizens.len() as i32);
    for (i, (citizen, state)) in town.citizens.iter().zip(&town.agents.states).enumerate() {
        assert!(state.clothing > 0, "{} has no clothing", citizen.name);
        assert_eq!(state.clothing_status, state.clothing);
        if citizen.is_adult(&town.rules) {
            assert!(state.tool_uses > 0, "{} has no tool", citizen.name);
        }
        // Adults loiter; children loiter or play next to a parent (activity 11).
        let current = town.agents.ais[i].current;
        assert!(
            current == activity::IDLE
                || (!citizen.is_adult(&town.rules) && current == activity::PLAY),
            "{} is doing {current}",
            citizen.name
        );
        assert_eq!(state.health_events & 4, 4);
        assert_ne!(citizen.position, positions[i]);
    }
    assert!(town
        .storages
        .iter()
        .all(|s| s.items.iter().all(|e| e.reserved == 0)));
    eprintln!(
        "equipped {} citizens / {adults} adults; tools {} -> {}, clothes {} -> {}",
        town.citizens.len(),
        initial[4],
        final_stock[4],
        initial[7],
        final_stock[7]
    );
    // Exhausted storage must set failure/status fields and cool down both searches.
    for s in &mut town.storages {
        s.items.clear();
    }
    for i in 0..town.citizens.len() {
        let group = town.citizen_anims[usize::from(!town.citizens[i].male)]
            .as_deref()
            .unwrap();
        let ch = &mut town.agents.characters[i];
        let mut stack = std::mem::take(&mut ch.stack);
        stack.clear(ch, group, &mut town.storages);
        ch.stack = stack;
        town.agents.states[i] = unbanished_game::behaviour::agents::CitizenState {
            tool_status: 9,
            clothing_status: 9,
            job: 42,
            ..Default::default()
        };
        town.agents.ais[i] = Default::default();
    }
    town.iterate(&terrain, &mut rng, 1.0 / 30.0, 0.0);
    town.update_citizens(&terrain, &mut rng, 1.0 / 30.0, 0.0, [(1000, 0); 21]);
    for (i, state) in town.agents.states.iter().enumerate() {
        assert_eq!(state.flags & 4, 4);
        assert_eq!(state.clothing_status, 0);
        assert!(town.agents.ais[i].cooldowns[activity::CLOTHING as usize] > 0);
        if town.citizens[i].is_adult(&town.rules) {
            assert_eq!(state.tool_status, 0);
            assert!(town.agents.ais[i].cooldowns[activity::TOOL as usize] > 0);
        }
        assert_eq!(state.job, 42); // The success-only writes must not run.
        assert_eq!(state.health_events & 4, 0);
    }
}
