use crate::town::load::tests_support::{archive, root};
use unbanished_formats::savegame::{Fields, GameState, Value};

/// The differences between two game states: objects' fields (type, path, ours, theirs),
/// map cells and schedules.
pub fn diff(a: &GameState, b: &GameState) -> Vec<String> {
    fn walk(prefix: &str, a: &Fields, b: &Fields, out: &mut Vec<String>) {
        for ((n, va), (_, vb)) in a.0.iter().zip(&b.0) {
            let path = format!("{prefix}.{n}");
            match (va, vb) {
                (Value::Block(_, fa), Value::Block(_, fb)) => walk(&path, fa, fb, out),
                (Value::List(la), Value::List(lb)) => {
                    if la.len() != lb.len() {
                        out.push(format!("{path}: {} items, saved {}", la.len(), lb.len()));
                    }
                    for (k, (fa, fb)) in la.iter().zip(lb).enumerate() {
                        walk(&format!("{path}[{k}]"), fa, fb, out);
                    }
                }
                _ if va != vb => out.push(format!("{path}: ours {va:?} saved {vb:?}")),
                _ => {}
            }
        }
    }
    let mut out = Vec::new();
    if a.entity_time != b.entity_time {
        out.push(format!(
            "entity clock: ours {} saved {}",
            a.entity_time, b.entity_time
        ));
    }
    walk("town", &a.town, &b.town, &mut out);
    walk("managers", &a.managers, &b.managers, &mut out);
    for (id, oa) in &a.objects {
        match b.objects.get(id) {
            Some(ob) => walk(
                &format!("{} {id}", oa.type_name),
                &oa.fields,
                &ob.fields,
                &mut out,
            ),
            None => out.push(format!("{} {id}: not in the save", oa.type_name)),
        }
    }
    for (i, (ca, cb)) in a.map.cells.iter().zip(&b.map.cells).enumerate() {
        if ca != cb {
            out.push(format!("map cell {i}: ours {ca:?} saved {cb:?}"));
        }
    }
    for (k, (sa, sb)) in a.schedules.iter().zip(&b.schedules).enumerate() {
        if sa != sb {
            out.push(format!("schedule {k}: ours {sa:?} saved {sb:?}"));
        }
    }
    out
}

/// A save's state as the original holds it after loading: one more floor under each active
/// FloorComponent (its load slot adds them again, `FUN_140124a10`).
fn floors_added(saved: &crate::saved::SavedGame) -> GameState {
    let mut expected = saved.state.clone();
    let n = expected.map.size;
    for (_, e) in saved.entities() {
        let active = saved
            .component(e, "FloorComponent")
            .is_some_and(|f| matches!(f.fields.at("floor.active"), Some(Value::Bool(true))));
        let Some(map) = saved.component(e, "MapComponent").filter(|_| active) else {
            continue;
        };
        let m = |k: &str| map.fields.i32(&format!("map.{k}")).unwrap_or(0);
        for y in m("y")..m("y") + m("h") {
            for x in m("x")..m("x") + m("w") {
                expected.map.cells[(y * n + x) as usize].bytes[5] += 1;
            }
        }
    }
    expected
}

/// Loading a vanilla save and writing it straight back gives the same bytes: every field
/// the load reads is written back by the same mapping.
#[test]
fn vanilla_saves_export_unchanged() {
    let Some(archive) = archive() else {
        eprintln!("game data missing; skipping");
        return;
    };
    for name in [
        "Save2.sav",
        "Save3.sav",
        "Save4.sav",
        "Save5.sav",
        "Save83.sav",
        "Goldfoothill.sav",
    ] {
        let Ok(bytes) = std::fs::read(root().join("save-game").join(name)) else {
            continue;
        };
        let saved = crate::saved::SavedGame::parse(&bytes).unwrap();
        let town = crate::town::StartingTown::from_save(&archive, &saved).unwrap();
        let mut g = saved.state.clone();
        town.export_into(&archive, &mut g);
        let ours = g.encode().unwrap();
        // What the original writes right after a load: the saved state with one more floor
        // under each active FloorComponent (its load slot adds them again, `FUN_140124a10`).
        let expected = floors_added(&saved);
        let original = expected.encode().unwrap();
        if ours != original {
            for line in diff(&g, &expected).iter().take(12) {
                eprintln!("{name}: {line}");
            }
        }
        if let Some(at) = ours.iter().zip(&original).position(|(a, b)| a != b) {
            panic!(
                "{name}: written state differs at {at:#x} (lengths {} / {})",
                ours.len(),
                original.len()
            );
        }
        assert_eq!(ours.len(), original.len(), "{name}");
        eprintln!("{name}: exported unchanged");
    }
}

/// A town run for a minute after loading, saved, and loaded again writes the same state:
/// everything the simulation changed is written in a form the load reads back. Save83 (116
/// citizens) and Goldfoothill (565 citizens, fishing docks, markets, trade).
#[test]
fn a_simulated_town_reloads_as_saved() {
    let Some(archive) = archive() else {
        eprintln!("game data missing; skipping");
        return;
    };
    for name in ["Save83.sav", "Goldfoothill.sav"] {
        let Ok(bytes) = std::fs::read(root().join("save-game").join(name)) else {
            continue;
        };
        reloads_as_saved(&archive, name, &bytes);
    }
}

fn reloads_as_saved(archive: &unbanished_formats::pkg::Archive, name: &str, bytes: &[u8]) {
    let saved = crate::saved::SavedGame::parse(bytes).unwrap();
    let world = crate::town::map::Map::from_save(archive, &saved).unwrap();
    let grid = world.heights.grid();
    let mut town = crate::town::StartingTown::from_save(archive, &saved).unwrap();
    let mut rng = unbanished_game::random::Random::new(83);
    let dt = 1.0 / 30.0;
    for _ in 0..30 * 60 {
        town.apply_commands(&grid, &mut rng);
        town.iterate(&grid, &mut rng, dt, 50.0);
        let warnings = unbanished_game::events::StockWarnings::default();
        let stock = std::array::from_fn(|i| (warnings.count[i], warnings.limits[i]));
        town.update_citizens(&grid, &mut rng, dt, 50.0, stock);
        town.construction
            .jobs
            .step(unbanished_game::scheduler::ticks(dt));
    }
    let mut first = saved.state.clone();
    town.export_into(archive, &mut first);
    // The minute changed the town: positions, needs, stacks, works.
    let changed = diff(&first, &floors_added(&saved));
    eprintln!(
        "{name}: {} fields changed by simulating; {} objects ({} saved)",
        changed.len(),
        first.objects.len(),
        saved.state.objects.len()
    );
    assert!(changed.len() > 100, "{name}: the simulation changed little");
    let written = first.encode().unwrap();
    let reloaded =
        crate::saved::SavedGame::new(GameState::decode(&written).unwrap(), saved.metadata.clone());
    let again = crate::town::StartingTown::from_save(archive, &reloaded).unwrap();
    assert_eq!(
        again.citizens.len(),
        town.citizens.iter().filter(|c| c.alive).count(),
        "{name}"
    );
    assert_eq!(
        again.wildlife.animals.len(),
        town.wildlife.animals.len(),
        "{name}"
    );
    let mut second = reloaded.state.clone();
    again.export_into(archive, &mut second);
    let lines = diff(&second, &floors_added(&reloaded));
    for line in lines.iter().take(20) {
        eprintln!("{name}: {line}");
    }
    assert!(
        lines.is_empty(),
        "{name}: {} differences after reloading",
        lines.len()
    );
}

/// A new game (seed 1234, Medium, run for ten seconds) written into an empty state loads
/// back and writes the same state again: every entity is made by the writers and reads
/// back as it was.
#[test]
fn a_new_game_saves_and_reloads() {
    let Some(archive) = archive() else {
        eprintln!("game data missing; skipping");
        return;
    };
    let params = crate::town::world_params::WorldParams {
        seed: 1234,
        start_condition_ref: r"Game\StartCondition\Medium.rsc:resource".into(),
        ..Default::default()
    };
    let world = crate::town::map::Map::generate(&archive, &params).unwrap();
    let grid = world.heights.grid();
    let mut rng = unbanished_game::random::Random::new(params.seed);
    let mut town = crate::town::StartingTown::create(
        &archive,
        &params.start_condition_ref,
        world.heights.size / 2,
        &grid,
        &mut rng,
    )
    .unwrap();
    let dt = 1.0 / 30.0;
    for _ in 0..30 * 10 {
        town.apply_commands(&grid, &mut rng);
        town.iterate(&grid, &mut rng, dt, 50.0);
        let warnings = unbanished_game::events::StockWarnings::default();
        let stock = std::array::from_fn(|i| (warnings.count[i], warnings.limits[i]));
        town.update_citizens(&grid, &mut rng, dt, 50.0, stock);
        town.construction
            .jobs
            .step(unbanished_game::scheduler::ticks(dt));
    }
    let size = world.heights.size as i32;
    let map = unbanished_formats::menu_scene::MapComponent {
        version: 6,
        dims: [size, size / 2, 32, 33, size + 1, 33 * 33],
        heights: world.heights.values.clone(),
        terrain_material: Some(world.profile.materials[0].clone()),
        surface: world.surface.clone(),
        water_vertices: world.water.vertices.clone(),
        water_indices: world.water.indices.clone(),
        river_material: Some(world.profile.materials[1].clone()),
        lake_material: Some(world.profile.materials[2].clone()),
        river_ranges: world.water.rivers.clone(),
        lake_ranges: world.water.lakes.clone(),
        river_paths: world.river_paths.clone(),
        decals: Vec::new(),
        extra_decals: Vec::new(),
        ints: Vec::new(),
        decal_materials: Vec::new(),
    };
    let mut first = super::new_game::state(super::new_game::NewGame {
        params: &params,
        flags: 0,
        map,
        effects: 2,
        history: 0,
        stock: 0,
        capacity: 0,
    });
    town.export_into(&archive, &mut first);
    let written = first.encode().unwrap();
    let reloaded = crate::saved::SavedGame::new(
        GameState::decode(&written).unwrap(),
        unbanished_formats::save::SaveMetadata {
            name: String::new(),
            width: 0,
            height: 0,
            image: Vec::new(),
            value: 2,
            mods: Vec::new(),
            wide: false,
        },
    );
    let again = crate::town::StartingTown::from_save(&archive, &reloaded).unwrap();
    let alive = |t: &crate::town::StartingTown| t.placed.iter().filter(|p| p.alive).count();
    assert_eq!(alive(&again), alive(&town));
    assert_eq!(again.citizens.len(), town.citizens.len());
    assert_eq!(again.wildlife.animals.len(), town.wildlife.animals.len());
    let mut second = reloaded.state.clone();
    again.export_into(&archive, &mut second);
    let lines = diff(&second, &floors_added(&reloaded));
    for line in lines.iter().take(20) {
        eprintln!("{line}");
    }
    assert!(
        lines.is_empty(),
        "{} differences after reloading",
        lines.len()
    );
}
