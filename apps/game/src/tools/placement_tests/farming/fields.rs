//! Crop fields: planting, tending and harvesting, and blight.

use super::*;

/// A crop field near the start, given one of the town's known crops in spring: two
/// farmers plant every cell, tend the crop while it grows, harvest it and the crop is
/// stored.
#[test]
fn farmers_plant_tend_and_harvest_a_field() {
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
    let crop = town
        .construction
        .known
        .iter()
        .copied()
        .find(|&t| {
            town.templates[t].growth.is_some() && !town.templates[t].reference.contains("Tree")
        })
        .expect("no known crop");
    // The nearest 6 × 6 square of empty ground to the start (rings outwards).
    let n = town.cells.size as i32;
    let empty = |town: &StartingTown, x: i32, y: i32| {
        (y..y + 6).all(|cy| {
            (x..x + 6).all(|cx| {
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
    let rect = (8..60)
        .flat_map(|r| (-r..=r).flat_map(move |d| [(d, -r), (d, r), (-r, d), (r, d)]))
        .map(|(dx, dy)| (sx + dx, sy + dy))
        .find(|&(x, y)| x > 0 && y > 0 && empty(&town, x, y))
        .expect("no empty ground");
    let id = town
        .place_area(
            &archive,
            r"Template\CropField.rsc:resource",
            (rect.0, rect.1, 6, 6),
            &terrain,
        )
        .expect("the field was not placed");
    town.construction
        .commands
        .push(unbanished_game::work::professions::Command::SetCrop {
            entity: id,
            crop: Some(crop),
        });
    let farmer = town
        .construction
        .professions
        .list
        .iter()
        .position(|p| p.reference.to_ascii_lowercase().ends_with(":farmer"))
        .expect("no farmer profession");
    town.construction
        .commands
        .push(unbanished_game::work::professions::Command::SetCount {
            profession: farmer,
            count: 2,
        });
    let material = town.templates[crop]
        .pile
        .and_then(|p| town.templates[p].pile_material)
        .expect("the crop leaves no raw material");
    let mut warnings = unbanished_game::events::StockWarnings::default();
    warnings.limits[5] = 100_000;
    let dt = 1.0 / 30.0;
    let mut seen = std::collections::BTreeSet::new();
    let (mut planted_at, mut growing_at, mut harvest_at, mut stored_at) = (None, None, None, None);
    let mut calendar = unbanished_game::calendar::Calendar::default();
    for step in 0..30 * 60 * 60 {
        town.construction.calendar = calendar.seconds();
        town.apply_commands(&terrain, &mut rng);
        town.iterate(&terrain, &mut rng, dt, 60.0);
        let stock = std::array::from_fn(|i| (warnings.count[i], warnings.limits[i]));
        town.update_citizens(&terrain, &mut rng, dt, 60.0, stock);
        warnings.recompute(&town.registered_storages(), &town.materials);
        town.construction
            .jobs
            .step(unbanished_game::scheduler::ticks(dt));
        calendar.advance(unbanished_game::scheduler::ticks(dt));
        let site = &town.construction.sites[&id];
        let state = site.machine.state;
        if planted_at.is_none() && state == 9 {
            planted_at = Some(step);
        }
        if growing_at.is_none() && state == 0xa {
            growing_at = Some(step);
        }
        if harvest_at.is_none() && state == 0xb {
            harvest_at = Some(step);
        }
        let piles: Vec<usize> = town
            .construction
            .piles
            .iter()
            .filter(|(_, p)| p.material == material)
            .map(|(&e, _)| e)
            .collect();
        seen.extend(piles.iter().copied());
        if stored_at.is_none() && seen.iter().any(|e| !piles.contains(e)) {
            stored_at = Some(step);
            break;
        }
    }
    let f = town.construction.sites[&id].crop_field.clone().unwrap();
    eprintln!(
        "planting {planted_at:?}, growing {growing_at:?}, harvest {harvest_at:?}, stored \
         {stored_at:?}, {} cells, states {:?}",
        f.cells.len(),
        f.cells.iter().map(|c| c.state).collect::<Vec<_>>()
    );
    assert!(planted_at.is_some(), "the field never planted");
    assert!(growing_at.is_some(), "the crop never grew");
    assert!(harvest_at.is_some(), "the crop was never harvested");
    assert!(stored_at.is_some(), "no crop was stored");
    assert_reservations_held(&town);
}

/// Blight on a field withers its crops and passes to the field next to it, which grows
/// the same crop (and says so in the event log).
#[test]
fn blight_withers_a_field_and_spreads() {
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
    let crop = town
        .construction
        .known
        .iter()
        .copied()
        .find(|&t| {
            town.templates[t].growth.is_some() && !town.templates[t].reference.contains("Tree")
        })
        .expect("no known crop");
    let n = town.cells.size as i32;
    let empty = |town: &StartingTown, x: i32, y: i32| {
        (y..y + 10).all(|cy| {
            (x..x + 20).all(|cx| {
                let i = (cy * n + cx) as usize;
                cx < n
                    && cy < n
                    && town.cells.types[i] == unbanished_game::terrain::CellType::GROUND
                    && town.cells.occupants[i].is_none()
                    && town.cells.objects[i].is_none()
            })
        })
    };
    let (sx, sy) = town.start;
    let rect = (8..150)
        .flat_map(|r| (-r..=r).flat_map(move |d| [(d, -r), (d, r), (-r, d), (r, d)]))
        .map(|(dx, dy)| (sx + dx, sy + dy))
        .find(|&(x, y)| x > 0 && y > 0 && empty(&town, x, y))
        .expect("no empty ground");
    let mut fields = Vec::new();
    // Big enough that a spread draw (±20 cells) falls on the other field often.
    for dx in [0, 10] {
        let id = town
            .place_area(
                &archive,
                r"Template\CropField.rsc:resource",
                (rect.0 + dx, rect.1, 10, 10),
                &terrain,
            )
            .expect("the field was not placed");
        town.construction
            .commands
            .push(unbanished_game::work::professions::Command::SetCrop {
                entity: id,
                crop: Some(crop),
            });
        fields.push(id);
    }
    let farmer = town
        .construction
        .professions
        .list
        .iter()
        .position(|p| p.reference.to_ascii_lowercase().ends_with(":farmer"))
        .expect("no farmer profession");
    town.construction
        .commands
        .push(unbanished_game::work::professions::Command::SetCount {
            profession: farmer,
            count: 8,
        });
    let mut warnings = unbanished_game::events::StockWarnings::default();
    warnings.limits[5] = 100_000;
    let dt = 1.0 / 30.0;
    let mut calendar = unbanished_game::calendar::Calendar::default();
    let crops = |town: &StartingTown, e: usize| {
        let f = town.construction.sites[&e].crop_field.as_ref().unwrap();
        f.cells
            .iter()
            .filter_map(|c| c.entity)
            .filter(|&x| town.placed[x].alive && !town.placed[x].removal_requested)
            .count()
    };
    let (mut sick_at, mut withered, mut spread_at) = (None, false, None);
    let mut before = 0;
    let mut logged = false;
    for step in 0..30 * 60 * 60 {
        town.construction.calendar = calendar.seconds();
        town.apply_commands(&terrain, &mut rng);
        town.iterate(&terrain, &mut rng, dt, 60.0);
        let stock = std::array::from_fn(|i| (warnings.count[i], warnings.limits[i]));
        town.update_citizens(&terrain, &mut rng, dt, 60.0, stock);
        warnings.recompute(&town.registered_storages(), &town.materials);
        town.construction
            .jobs
            .step(unbanished_game::scheduler::ticks(dt));
        calendar.advance(unbanished_game::scheduler::ticks(dt));
        // Both fields have crops coming up (planting goes on).
        let growing = 20 <= crops(&town, fields[0]) && 5 <= crops(&town, fields[1]);
        if sick_at.is_none() && growing {
            sick_at = Some(step);
            before = crops(&town, fields[0]);
            let b = town.construction.sites.get_mut(&fields[0]).unwrap();
            let b = b.disease.as_mut().unwrap();
            b.left = unbanished_game::scheduler::ticks(8.0 * 300.0);
            // Fast-forward: 200 spread intervals at once (a draw lands on the other
            // field about one time in sixteen).
            b.spread = unbanished_game::scheduler::ticks(200.0 * 0.125 * 300.0);
        }
        if sick_at.is_some() && crops(&town, fields[0]) < before {
            withered = true;
        }
        for (kind, _, e) in std::mem::take(&mut town.construction.site_events) {
            logged |= kind == 0xc && e == fields[1];
        }
        let other = town.construction.sites[&fields[1]]
            .disease
            .as_ref()
            .unwrap();
        if sick_at.is_some() && spread_at.is_none() && 0 < other.left {
            spread_at = Some(step);
        }
        if withered && spread_at.is_some() {
            break;
        }
    }
    eprintln!(
        "sick {sick_at:?} with {before} crops, now {}, spread {spread_at:?}",
        crops(&town, fields[0])
    );
    assert!(sick_at.is_some(), "the fields never grew");
    assert!(withered, "no crop withered");
    assert!(spread_at.is_some(), "the blight did not spread");
    assert!(logged, "the event log was not told");
}
