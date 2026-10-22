//! Roads, and dragging areas and pastures.

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
