//! Roads, bridges, tunnels, and dragged areas and pastures.

use super::*;

/// A dirt road dragged near the start is built by builders cell by cell, then upgraded to
/// stone (they fetch a stone per cell), and Remove Roads lays part of it back to ground.
#[test]
fn roads_are_built_upgraded_and_removed() {
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
    // A row of eight free ground cells near the start.
    let free = |town: &StartingTown, c: i32| {
        let i = c as usize;
        town.cells.types[i].0 == 2
            && town.cells.occupants[i].is_none()
            && town.cells.objects[i].is_none()
    };
    let (sx, sy) = town.start;
    let mut row = None;
    'find: for r in 4..30 {
        for dy in -r..=r {
            for dx in -r..=r {
                let (x, y) = (sx + dx, sy + dy);
                if (0..8).all(|k| free(&town, y * n + x + k)) {
                    row = Some((x, y));
                    break 'find;
                }
            }
        }
    }
    let (x, y) = row.expect("no free row");
    let (a, b) = (y * n + x, y * n + x + 7);
    let builder = town.construction.professions.builder;
    town.construction
        .commands
        .push(unbanished_game::work::professions::Command::SetCount {
            profession: builder,
            count: 4,
        });
    let mut warnings = unbanished_game::events::StockWarnings::default();
    let dt = 1.0 / 30.0;
    let mut run =
        |town: &mut StartingTown, rng: &mut Random, done: &dyn Fn(&StartingTown) -> bool| {
            for _ in 0..30 * 60 * 10 {
                town.apply_commands(&terrain, rng);
                town.iterate(&terrain, rng, dt, 50.0);
                let stock = std::array::from_fn(|i| (warnings.count[i], warnings.limits[i]));
                town.update_citizens(&terrain, rng, dt, 50.0, stock);
                warnings.recompute(&town.registered_storages(), &town.materials);
                town.construction
                    .jobs
                    .step(unbanished_game::scheduler::ticks(dt));
                if done(town) {
                    return true;
                }
            }
            false
        };
    let cells: Vec<i32> = (a..=b).collect();
    let all = |town: &StartingTown, ty: u8| {
        cells.iter().all(|&c| town.cells.types[c as usize].0 == ty)
            && town.construction.pending_roads.is_empty()
    };
    let mut dirt = crate::tools::removal_tool::RemovalTool::start(
        &archive,
        &mut town,
        r"Template\DirtRoad.rsc:resource",
    )
    .expect("no road tool");
    assert_eq!(
        dirt.drag(&archive, &mut town, &terrain, a, b),
        crate::tools::removal_tool::DragSound::Thunk
    );
    assert!(
        run(&mut town, &mut rng, &|t| all(t, 1)),
        "the dirt road was not built"
    );
    let mut stone = crate::tools::removal_tool::RemovalTool::start(
        &archive,
        &mut town,
        r"Template\StoneRoad.rsc:resource",
    )
    .unwrap();
    stone.drag(&archive, &mut town, &terrain, a, b);
    assert!(
        run(&mut town, &mut rng, &|t| all(t, 0)),
        "the stone road was not built"
    );
    let mut clear = crate::tools::removal_tool::RemovalTool::start(
        &archive,
        &mut town,
        r"Template\ClearRoad.rsc:resource",
    )
    .unwrap();
    clear.drag(&archive, &mut town, &terrain, a, a + 2);
    let cleared = |t: &StartingTown| {
        (a..=a + 2)
            .all(|c| t.cells.types[c as usize].0 == 2 && t.cells.road_kind[c as usize] == 0xff)
    };
    assert!(
        run(&mut town, &mut rng, &cleared),
        "the road was not removed"
    );
    assert_eq!(town.cells.types[(a + 3) as usize].0, 0);
}

/// A wooden bridge dragged across the river nearest the start: its parts span the water,
/// builders put it up, its cells become bridge (type 9) with a deck above the water, and
/// pulling it down lifts both again.
#[test]
fn a_bridge_spans_the_river() {
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
    let mut tool = crate::tools::removal_tool::RemovalTool::start(
        &archive,
        &mut town,
        r"Template\WoodBridge.rsc:resource",
    )
    .expect("no bridge tool");
    let spec = town
        .bridge_spec(&archive, r"Template\WoodBridge.rsc:resource")
        .unwrap();
    assert_eq!(spec.parts.iter().flatten().count(), 5);
    // The nearest crossing to the start, either way, at most 12 cells.
    let n = town.cells.size as i32;
    let (sx, sy) = town.start;
    let mut found = None;
    'find: for r in 1..n {
        for dy in -r..=r {
            for dx in -r..=r {
                if dx.abs() != r && dy.abs() != r {
                    continue;
                }
                let (x, y) = (sx + dx, sy + dy);
                if !(0..n).contains(&x) || !(0..n).contains(&y) {
                    continue;
                }
                for (ux, uy) in [(1, 0), (0, 1)] {
                    for l in 3..14 {
                        let (ex, ey) = (x + ux * l, y + uy * l);
                        if n <= ex || n <= ey {
                            break;
                        }
                        let (a, b) = (y * n + x, ey * n + ex);
                        let (ok, span) = town.bridge_check(&spec, a, b);
                        if ok && 3 <= span {
                            found = Some((a, b, span));
                            break 'find;
                        }
                    }
                }
            }
        }
    }
    let (a, b, span) = found.expect("no crossing");
    let before = town.placed.len();
    assert_eq!(
        tool.drag(&archive, &mut town, &terrain, a, b),
        crate::tools::removal_tool::DragSound::Thunk
    );
    let id = (before..town.placed.len())
        .find(|&e| town.construction.sites.contains_key(&e))
        .expect("no bridge site");
    let parts = town.construction.sites[&id].parts.clone();
    let cells: i32 = parts
        .iter()
        .map(|&p| town.placed[p].footprint.0 * town.placed[p].footprint.1)
        .sum();
    assert!(
        cells == span || (span == 3 && cells == 4),
        "{cells} cells for {span}"
    );
    let deck: Vec<usize> = parts
        .iter()
        .flat_map(|&p| {
            let q = &town.placed[p];
            let (x, y) = q.cell;
            (0..q.footprint.1)
                .flat_map(move |j| (0..q.footprint.0).map(move |i| ((y + j) * n + x + i) as usize))
        })
        .collect();
    assert!(deck.iter().all(|&i| 0 < town.cells.floors.counts[i]));
    let builder = town.construction.professions.builder;
    town.construction
        .commands
        .push(unbanished_game::work::professions::Command::SetCount {
            profession: builder,
            count: 4,
        });
    let mut warnings = unbanished_game::events::StockWarnings::default();
    let dt = 1.0 / 30.0;
    let mut supplied = false;
    let mut states = Vec::new();
    let mut built = false;
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
        if state == 1 && !supplied {
            supplied = true;
            // MultiObject `+0x58`: a bridge costs its cells' worth.
            let (st, storage, stages) = (site.template, site.storage, site.stages);
            assert_eq!(Some(stages), site.multi_stages);
            assert!(1 < stages);
            for &(k, m) in &town.construction.templates[st].materials.clone() {
                town.storages[storage].add(m, k * stages);
            }
        }
        if state == 2 {
            built = true;
            break;
        }
    }
    assert!(built, "the bridge was not built: {states:?}");
    assert!(
        deck.iter().all(|&i| town.cells.types[i].0 == 9),
        "not bridge cells"
    );
    // The deck stands above the river bed.
    let half = (n >> 1) as f32 - 0.5;
    let i = deck[deck.len() / 2] as i32;
    let (x, y) = ((i % n) as f32 - half, (i / n) as f32 - half);
    let mut normal = [0.0; 3];
    let bed = terrain.ground(x, y, &mut normal);
    let top = town.cells.floors.ground(town.cells.size, &terrain, x, y);
    assert!(bed < top, "deck {top} not above the bed {bed}");
    // Torn down, the water comes back.
    town.construction
        .commands
        .push(unbanished_game::work::professions::Command::Demolish { entity: id });
    let mut gone = false;
    for _ in 0..30 * 60 * 10 {
        town.apply_commands(&terrain, &mut rng);
        town.iterate(&terrain, &mut rng, dt, 50.0);
        let stock = std::array::from_fn(|i| (warnings.count[i], warnings.limits[i]));
        town.update_citizens(&terrain, &mut rng, dt, 50.0, stock);
        warnings.recompute(&town.registered_storages(), &town.materials);
        town.construction
            .jobs
            .step(unbanished_game::scheduler::ticks(dt));
        if !town.construction.sites.contains_key(&id) {
            gone = true;
            break;
        }
    }
    assert!(gone, "the bridge was not torn down");
    assert!(deck.iter().all(|&i| town.cells.floors.counts[i] == 0));
    assert!(deck.iter().all(|&i| town.cells.types[i].blocks_movement()));
    eprintln!("bridge of {span} over {:?}, states {states:?}", parts.len());
}

/// A tunnel dragged through the hill nearest the start: placed by the bridge tool, built,
/// it digs and fills the terrain (TerraformComponent), its cells become tunnel (type 11)
/// and the rough ground over it high; pulled down, the fill goes again.
#[test]
fn a_tunnel_goes_through_a_hill() {
    let dir =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../game-data/original/WinData");
    let mut archive = Archive::default();
    for name in ["data1.pkg", "patch1.pkg", "patch0.pkg", "data0.pkg"] {
        let Ok(bytes) = std::fs::read(dir.join(name)) else {
            return;
        };
        archive.add(Package::parse(bytes).unwrap());
    }
    let reference = r"Template\Tunnel.rsc:resource";
    // The first map (by seed) with a hill to tunnel the town can reach both sides of.
    let mut chosen = None;
    for seed in 1..40 {
        let world = crate::view::world::World::generate(
            &archive,
            &crate::view::world::WorldParams {
                seed,
                ..Default::default()
            },
        )
        .unwrap();
        let heights = world.terrain.heights.clone();
        let width = world.terrain.width();
        let mut rng = Random::new(seed);
        let terrain = HeightGrid {
            width,
            heights: &heights,
        };
        let Some(mut town) = StartingTown::create(
            &archive,
            "Game/StartCondition/Medium.rsc:resource",
            world.terrain.size,
            &terrain,
            &mut rng,
        ) else {
            continue;
        };
        let spec = town.bridge_spec(&archive, reference).unwrap();
        let n = town.cells.size as i32;
        let (sx, sy) = town.start;
        let regions = unbanished_game::pathfind::regions(town.cells.size, &town.cells.types);
        let home = regions[(sy * n + sx) as usize];
        let mut found = None;
        'find: for r in 1i32..48 {
            for dy in -r..=r {
                for dx in -r..=r {
                    if dx.abs() != r && dy.abs() != r {
                        continue;
                    }
                    let (x, y) = (sx + dx, sy + dy);
                    if !(1..n - 1).contains(&x) || !(1..n - 1).contains(&y) {
                        continue;
                    }
                    for (ux, uy) in [(1, 0), (0, 1)] {
                        for l in 3..14 {
                            let (ex, ey) = (x + ux * l, y + uy * l);
                            if n - 1 <= ex || n - 1 <= ey {
                                break;
                            }
                            let (a, b) = (y * n + x, ey * n + ex);
                            let (ok, span) = town.bridge_check(&spec, a, b);
                            if ok
                                && 3 <= span
                                && regions[a as usize] == home
                                && regions[b as usize] == home
                            {
                                found = Some((a, b, span));
                                break 'find;
                            }
                        }
                    }
                }
            }
        }
        if let Some(f) = found {
            chosen = Some((heights, width, rng, town, spec, f, seed));
            break;
        }
    }
    let (mut heights, width, mut rng, mut town, spec, (a, b, span), seed) =
        chosen.expect("no hill to tunnel");
    let n = town.cells.size as i32;
    assert!(town.templates[spec.parts[0].unwrap()].terraform.is_some());
    let mut tool = crate::tools::removal_tool::RemovalTool::start(&archive, &mut town, reference)
        .expect("no tunnel tool");
    let before = town.placed.len();
    {
        let terrain = HeightGrid {
            width,
            heights: &heights,
        };
        assert_eq!(
            tool.drag(&archive, &mut town, &terrain, a, b),
            crate::tools::removal_tool::DragSound::Thunk
        );
    }
    let id = (before..town.placed.len())
        .find(|&e| town.construction.sites.contains_key(&e))
        .expect("no tunnel site");
    let parts = town.construction.sites[&id].parts.clone();
    let builder = town.construction.professions.builder;
    town.construction
        .commands
        .push(unbanished_game::work::professions::Command::SetCount {
            profession: builder,
            count: 4,
        });
    let mut warnings = unbanished_game::events::StockWarnings::default();
    let dt = 1.0 / 30.0;
    let original = heights.clone();
    let mut step = |town: &mut StartingTown, heights: &mut Vec<i16>, rng: &mut Random| {
        {
            let terrain = HeightGrid {
                width,
                heights: heights.as_slice(),
            };
            town.apply_commands(&terrain, rng);
            town.iterate(&terrain, rng, dt, 50.0);
            let stock = std::array::from_fn(|i| (warnings.count[i], warnings.limits[i]));
            town.update_citizens(&terrain, rng, dt, 50.0, stock);
        }
        warnings.recompute(&town.registered_storages(), &town.materials);
        town.construction
            .jobs
            .step(unbanished_game::scheduler::ticks(dt));
        town.apply_terraforms(heights, width);
    };
    // The tunnel clears its cells at once (`+0x2d`), so its supply starts on the first
    // tick: laborers bring all of its stone from the town's stock.
    let (mut built, mut states) = (false, Vec::new());
    for _ in 0..30 * 60 * 20 {
        step(&mut town, &mut heights, &mut rng);
        let site = &town.construction.sites[&id];
        let state = site.machine.state;
        if states.last() != Some(&state) {
            states.push(state);
        }
        if state == 2 {
            built = true;
            break;
        }
    }
    assert!(built, "the tunnel was not built: {states:?}");
    step(&mut town, &mut heights, &mut rng);
    let changed = heights
        .iter()
        .zip(&original)
        .filter(|(a, b)| a != b)
        .count();
    let raised: usize = parts
        .iter()
        .filter_map(|p| town.construction.terraforms.get(p))
        .map(|t| t.raised.len())
        .sum();
    let inside = parts.iter().all(|&p| {
        let q = &town.placed[p];
        let i = (q.cell.1 * n + q.cell.0) as usize;
        town.cells.types[i].0 == 11
    });
    assert!(inside, "the tunnel cells are not tunnel");
    assert!(changed > 0, "the terrain was not reshaped");
    eprintln!(
        "seed {seed}: tunnel of {span}: {} parts, {changed} heights changed, {raised} raised, \
         states {states:?}",
        parts.len()
    );
    // Torn down, the ceiling's fill goes again.
    town.construction
        .commands
        .push(unbanished_game::work::professions::Command::Demolish { entity: id });
    let mut gone = false;
    for _ in 0..30 * 60 * 10 {
        step(&mut town, &mut heights, &mut rng);
        if !town.construction.sites.contains_key(&id) {
            gone = true;
            break;
        }
    }
    step(&mut town, &mut heights, &mut rng);
    assert!(gone, "the tunnel was not torn down");
    assert!(town.construction.terraforms.is_empty());
}

/// The drag tools for areas: a field, an orchard and a storage yard (DragPlacementTool)
/// and a pasture (FencedTool) dragged over empty ground; too small and too large drags
/// are refused.
#[test]
fn areas_and_pastures_are_dragged() {
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
    let empty = |town: &StartingTown, x: i32, y: i32, w: i32, h: i32| {
        (y..y + h).all(|cy| {
            (x..x + w).all(|cx| {
                let i = (cy * n + cx) as usize;
                cx < n
                    && cy < n
                    && town.cells.types[i] == unbanished_game::terrain::CellType::GROUND
                    && town.cells.objects[i].is_none()
                    && town.cells.occupants[i].is_none()
                    && town.cells.flags[i] == 0
            })
        })
    };
    let (sx, sy) = town.start;
    let spot = |town: &StartingTown, w: i32, h: i32| {
        (8..80)
            .flat_map(|r| (-r..=r).flat_map(move |d| [(d, -r), (d, r), (-r, d), (r, d)]))
            .map(|(dx, dy)| (sx + dx, sy + dy))
            .find(|&(x, y)| x > 0 && y > 0 && empty(town, x, y, w, h))
            .expect("no empty ground")
    };
    for (reference, w, h) in [
        (r"Template\CropField.rsc:resource", 6, 5),
        (r"Template\Orchard.rsc:resource", 5, 5),
        (r"Template\StorageYard.rsc:resource", 4, 6),
        (r"Template\Pasture.rsc:resource", 8, 8),
    ] {
        let mut tool =
            crate::tools::removal_tool::RemovalTool::start(&archive, &mut town, reference)
                .unwrap_or_else(|| panic!("no drag tool for {reference}"));
        let (x, y) = spot(&town, w, h);
        let a = y * n + x;
        // One cell is too small for every one of them.
        assert_eq!(
            tool.drag(&archive, &mut town, &terrain, a, a),
            crate::tools::removal_tool::DragSound::Error,
            "{reference}"
        );
        let before = town.placed.len();
        let b = (y + h - 1) * n + x + w - 1;
        assert_eq!(
            tool.drag(&archive, &mut town, &terrain, a, b),
            crate::tools::removal_tool::DragSound::Thunk,
            "{reference}"
        );
        let id = (before..town.placed.len())
            .find(|&e| town.construction.sites.contains_key(&e))
            .unwrap_or_else(|| panic!("no site for {reference}"));
        let p = &town.placed[id];
        assert_eq!((p.cell, p.footprint), ((x, y), (w, h)), "{reference}");
        if !reference.contains("Pasture") {
            // Turned 90° unless wider than long.
            assert_eq!(p.yaw, if h < w { 0.0 } else { 90.0 }, "{reference}");
        }
        assert!(tool
            .cells(&mut town, &terrain, a)
            .iter()
            .all(|&(_, _, ok)| !ok));
    }
}
