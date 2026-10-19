use super::*;
use banished_formats::pkg::Package;
use banished_game::random::Random;

/// ClearTree over the trees near the start: one marker per tree cell, laborers fell them,
/// every marker removes itself, and ClearCancel takes markers away again.
#[test]
fn drags_report_their_size_count_and_message() {
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
    let (x, y) = town.start;
    let drag = |tool: &mut RemovalTool, a: i32, b: i32| {
        tool.dragging = true;
        tool.start = a;
        tool.end = b;
    };
    let mut clear =
        RemovalTool::start(&archive, &mut town, r"Template\ClearTree.rsc:resource").unwrap();
    assert!(clear.drag_info(&mut town, &terrain).is_none());
    drag(&mut clear, (y - 2) * n + x - 3, (y + 2) * n + x + 3);
    let info = clear.drag_info(&mut town, &terrain).unwrap();
    assert_eq!(
        (info.mode, info.width, info.height, info.count),
        (0, 7, 5, 1)
    );
    // A tutorial's RestrictPlayer cells: only that drag is valid; any other fails.
    clear.restrict = Some([x - 3, y - 2, x + 3, y + 2]);
    assert_eq!(clear.drag_info(&mut town, &terrain).unwrap().message, 0);
    clear.restrict = Some([x - 3, y - 2, x + 3, y + 3]);
    assert_eq!(clear.drag_info(&mut town, &terrain).unwrap().message, 3);
    let before = town.placed.len();
    assert_eq!(clear.apply(&archive, &mut town, &terrain), DragSound::Error);
    assert_eq!(town.placed.len(), before);
    clear.restrict = None;
    // A road from a free cell 6 along: its length, the stone per cell.
    let mut road =
        RemovalTool::start(&archive, &mut town, r"Template\StoneRoad.rsc:resource").unwrap();
    let free = (0..n * n)
        .find(|&i| {
            (0..7).all(|k| {
                let c = (i + k) as usize;
                (i + k) % n >= k
                    && town.cells.types[c].0 == 2
                    && town.cells.occupants[c].is_none()
                    && town.cells.objects[c].is_none()
            })
        })
        .unwrap();
    drag(&mut road, free, free + 6);
    let info = road.drag_info(&mut town, &terrain).unwrap();
    assert_eq!(
        (info.mode, info.width, info.count, info.message),
        (1, 7, 7, 0)
    );
    // No road to the water: no size, message 1.
    let water = (0..n * n)
        .find(|&i| 1u32 << (town.cells.types[i as usize].0 & 0x1f) & ROAD_ENDS != 0)
        .unwrap();
    drag(&mut road, free, water);
    let info = road.drag_info(&mut town, &terrain).unwrap();
    assert_eq!((info.mode, info.message), (2, 1));
    // Hovering a free cell before a drag (`FUN_14010a660`): that cell, green.
    road.dragging = false;
    let cells = road.cells(&mut town, &terrain, free);
    assert_eq!(cells, vec![(free % n, free / n, true)], "hover");
    // A drag along free cells: every cell green.
    drag(&mut road, free, free + 6);
    let cells = road.cells(&mut town, &terrain, free);
    assert_eq!(cells.len(), 7);
    assert!(cells.iter().all(|c| c.2), "{cells:?}");
    // Drawn green with frame 1, not as a removal's red marks.
    let overlay = road.overlay(&mut town, &terrain, free);
    assert!(overlay
        .iter()
        .all(|c| c.argb == 0xff00_ff00 && c.frame == 1));
    // To the water: the way there shown, red where a road cannot go.
    drag(&mut road, free, water);
    let overlay = road.overlay(&mut town, &terrain, free);
    assert!(overlay.iter().any(|c| c.argb == 0xffff_0000));
}

#[test]
fn clear_tree_marks_and_fells() {
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
    let (x, y) = town.start;
    let r = 12;
    let (a, b) = ((y - r) * n + x - r, (y + r) * n + x + r);
    let is_tree = |town: &StartingTown, i: usize| {
        town.cells.objects[i].is_some_and(|o| {
            town.placed[o].alive
                && town.templates[town.placed[o].template]
                    .reference
                    .contains("NaturalResourceTree")
        })
    };
    let cells: Vec<usize> = (y - r..=y + r)
        .flat_map(|cy| (x - r..=x + r).map(move |cx| (cy * n + cx) as usize))
        .collect();
    let trees = cells.iter().filter(|&&i| is_tree(&town, i)).count();
    let mut tool =
        RemovalTool::start(&archive, &mut town, r"Template\ClearTree.rsc:resource").unwrap();
    let before = town.placed.len();
    tool.drag(&archive, &mut town, &terrain, a, b);
    let markers: Vec<usize> = (before..town.placed.len()).collect();
    assert!(!markers.is_empty(), "no trees near the start");
    assert!(markers.len() <= trees);
    // A second drag marks nothing new: every marked cell has an occupant.
    tool.drag(&archive, &mut town, &terrain, a, b);
    assert_eq!(town.placed.len(), before + markers.len());

    // Cancel one marker before anyone works on it.
    let mut cancel =
        RemovalTool::start(&archive, &mut town, r"Template\ClearCancel.rsc:resource").unwrap();
    let first = markers[0];
    let (cx, cy) = town.placed[first].cell;
    cancel.drag(&archive, &mut town, &terrain, cy * n + cx, cy * n + cx);
    assert_eq!(town.construction.destroy, [first]);

    let mut warnings = banished_game::events::StockWarnings::default();
    let dt = 1.0 / 30.0;
    let mut done_at = None;
    for step in 0..30 * 60 * 10 {
        town.apply_commands(&terrain, &mut rng);
        town.iterate(&terrain, &mut rng, dt, 50.0);
        let stock = std::array::from_fn(|i| (warnings.count[i], warnings.limits[i]));
        town.update_citizens(&terrain, &mut rng, dt, 50.0, stock);
        warnings.recompute(&town.registered_storages(), &town.materials);
        town.construction
            .jobs
            .step(banished_game::scheduler::ticks(dt));
        if step == 0 {
            assert!(!town.placed[first].alive, "cancelled marker still alive");
            assert_eq!(town.cells.occupants[(cy * n + cx) as usize], None);
        }
        if markers.iter().all(|&m| !town.placed[m].alive) {
            done_at = Some(step);
            break;
        }
    }
    let left = markers.iter().filter(|&&m| town.placed[m].alive).count();
    eprintln!(
        "{} markers, {left} left, done at {done_at:?}",
        markers.len()
    );
    assert!(done_at.is_some(), "markers left: {left}");
    for &m in &markers[1..] {
        let (mx, my) = town.placed[m].cell;
        let i = (my * n + mx) as usize;
        assert!(!is_tree(&town, i), "tree at {mx},{my} not felled");
        assert_eq!(town.cells.occupants[i], None);
    }
    let (fx, fy) = town.placed[first].cell;
    assert!(
        is_tree(&town, (fy * n + fx) as usize),
        "cancelled tree was felled"
    );
    assert!(town.construction.sites.keys().all(|e| !markers.contains(e)));
}
