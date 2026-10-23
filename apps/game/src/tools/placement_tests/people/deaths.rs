//! Deaths: accidents and burials in the cemetery.

use super::*;

/// A fenced cemetery is built and lays out its graves; a citizen who dies of old age is
/// buried there (a tombstone on a grave) and the spouse grieves less (bit `0x40`).
#[test]
fn the_dead_are_buried_in_the_cemetery() {
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
    // Seed 7: the cemetery is built in under three minutes. Seed 1234 builds it unusually late
    // (19 minutes before meals at home, 50 after), so the burial fell outside the window.
    let mut rng = Random::new(7);
    let mut town = StartingTown::create(
        &archive,
        "Game/StartCondition/Easy.rsc:resource",
        world.terrain.size,
        &terrain,
        &mut rng,
    )
    .unwrap();
    let n = town.cells.size as i32;
    let size = 8;
    let empty = |town: &StartingTown, x: i32, y: i32| {
        (y..y + size).all(|cy| {
            (x..x + size).all(|cx| {
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
    let rect = (8..80)
        .flat_map(|r| (-r..=r).flat_map(move |d| [(d, -r), (d, r), (-r, d), (r, d)]))
        .map(|(dx, dy)| (sx + dx, sy + dy))
        .find(|&(x, y)| x > 0 && y > 0 && empty(&town, x, y))
        .expect("no empty ground");
    let id = town
        .place_fenced(
            &archive,
            r"Template\Cemetery.rsc:resource",
            rect,
            (rect.0 + size - 1, rect.1 + size - 1),
            0,
            &terrain,
        )
        .expect("the cemetery was not placed");
    let builder = town.construction.professions.builder;
    town.construction
        .commands
        .push(unbanished_game::work::professions::Command::SetCount {
            profession: builder,
            count: 4,
        });
    let mut warnings = unbanished_game::events::StockWarnings {
        limits: [100_000; 21],
        ..Default::default()
    };
    let dt = 1.0 / 30.0;
    let (mut built_at, mut buried_at) = (None, None);
    let mut widow = None;
    for step in 0..30 * 60 * 30 {
        town.apply_commands(&terrain, &mut rng);
        town.iterate(&terrain, &mut rng, dt, 60.0);
        let stock = std::array::from_fn(|i| (warnings.count[i], warnings.limits[i]));
        town.update_citizens(&terrain, &mut rng, dt, 60.0, stock);
        warnings.recompute(&town.registered_storages(), &town.materials);
        town.construction
            .jobs
            .step(unbanished_game::scheduler::ticks(dt));
        if built_at.is_none() && town.construction.cemeteries.contains(&id) {
            built_at = Some(step);
            // Old age comes for a married citizen.
            let c = (0..town.citizens.len())
                .find(|&c| town.citizens[c].alive && town.citizens[c].spouse.is_some())
                .expect("no married citizen");
            widow = town.citizens[c].spouse;
            let age = town.citizens[c].age;
            town.citizens[c].lifespan = age;
        }
        if built_at.is_some() && !town.construction.tombstones.is_empty() {
            buried_at = Some(step);
            break;
        }
    }
    let graves = town.construction.sites[&id].graves.clone();
    eprintln!(
        "built {built_at:?}, buried {buried_at:?}, {} graves, tombstones {:?}",
        graves.len(),
        town.construction.tombstones
    );
    assert!(built_at.is_some(), "the cemetery was not built");
    assert!(!graves.is_empty(), "no graves laid out");
    assert!(buried_at.is_some(), "nobody was buried");
    let (&stone, _) = town.construction.tombstones.iter().next().unwrap();
    let p = &town.placed[stone];
    let cell = p.cell.1 * n + p.cell.0;
    assert!(graves.contains(&cell), "the tombstone is not on a grave");
    assert_eq!(town.cells.objects[cell as usize], Some(stone));
    let w = widow.unwrap();
    assert_ne!(
        town.agents.states[w].flags & 0x40,
        0,
        "the spouse knows of the burial"
    );
}

/// With accidents made near certain for builders, a builder's work step fails with an
/// injury and the builder dies of it (code 7, the profession's death message).
#[test]
fn a_builder_dies_in_an_accident() {
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
    let builder = town.construction.professions.builder;
    let death = town.construction.professions.list[builder].death.clone();
    assert!(!death.is_empty(), "the builder has no death message");
    assert!(0.0 < town.construction.professions.list[builder].accident.1);
    town.construction.professions.list[builder].accident = (1e-6, 1.0);
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
    tool.place(&archive, &mut town, 0).unwrap();
    town.construction
        .commands
        .push(unbanished_game::work::professions::Command::SetCount {
            profession: builder,
            count: 2,
        });
    let mut warnings = unbanished_game::events::StockWarnings::default();
    let dt = 1.0 / 30.0;
    let mut events = Vec::new();
    for _ in 0..30 * 60 * 8 {
        town.apply_commands(&terrain, &mut rng);
        town.iterate(&terrain, &mut rng, dt, 50.0);
        let stock = std::array::from_fn(|i| (warnings.count[i], warnings.limits[i]));
        town.update_citizens(&terrain, &mut rng, dt, 50.0, stock);
        warnings.recompute(&town.registered_storages(), &town.materials);
        town.construction
            .jobs
            .step(unbanished_game::scheduler::ticks(dt));
        events.extend(std::mem::take(&mut town.agents.events));
        if events
            .iter()
            .any(|e| e.kind == unbanished_game::people::life::reason::ACCIDENT)
        {
            break;
        }
    }
    let e = events
        .iter()
        .find(|e| e.kind == unbanished_game::people::life::reason::ACCIDENT)
        .expect("no accident");
    assert_eq!(
        e.extra.as_ref().map(|(_, k)| k.as_str()),
        Some(death.as_str())
    );
    assert_eq!(
        town.agents.states[e.citizen].accident,
        Some(builder),
        "the accident's profession"
    );
}
