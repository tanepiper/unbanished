//! Fields, orchards and pastures: planting, tending, harvesting, eggs and blight.

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

/// An orchard near the start given the town's known tree: farmers plant every
/// slot and tend; with the trees grown and their fruit ripe in autumn they pick it and
/// the fruit is stored.
#[test]
fn arborists_plant_tend_and_pick_an_orchard() {
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
    let tree = town
        .construction
        .known
        .iter()
        .copied()
        .find(|&t| town.templates[t].fruit.is_some())
        .expect("no known orchard tree");
    let n = town.cells.size as i32;
    let empty = |town: &StartingTown, x: i32, y: i32| {
        (y..y + 7).all(|cy| {
            (x..x + 7).all(|cx| {
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
            r"Template\Orchard.rsc:resource",
            (rect.0, rect.1, 7, 7),
            &terrain,
        )
        .expect("the orchard was not placed");
    town.construction
        .commands
        .push(unbanished_game::work::professions::Command::SetCrop {
            entity: id,
            crop: Some(tree),
        });
    let arborist = town
        .construction
        .professions
        .list
        .iter()
        .position(|p| p.reference.to_ascii_lowercase().ends_with(":farmer"))
        .expect("no farmer profession");
    town.construction
        .commands
        .push(unbanished_game::work::professions::Command::SetCount {
            profession: arborist,
            count: 2,
        });
    let fruit = town.templates[tree]
        .fruit
        .and_then(|f| f.pile)
        .and_then(|p| town.templates[p].pile_material)
        .expect("no fruit material");
    let mut warnings = unbanished_game::events::StockWarnings::default();
    warnings.limits[5] = 100_000;
    let dt = 1.0 / 30.0;
    let mut calendar = unbanished_game::calendar::Calendar::default();
    let (mut tending_at, mut picking_at, mut stored_at) = (None, None, None);
    let mut seen = std::collections::BTreeSet::new();
    for step in 0..30 * 60 * 30 {
        town.construction.calendar = calendar.seconds();
        town.construction.year = calendar.year;
        town.apply_commands(&terrain, &mut rng);
        town.iterate(&terrain, &mut rng, dt, 60.0);
        let stock = std::array::from_fn(|i| (warnings.count[i], warnings.limits[i]));
        town.update_citizens(&terrain, &mut rng, dt, 60.0, stock);
        warnings.recompute(&town.registered_storages(), &town.materials);
        town.construction
            .jobs
            .step(unbanished_game::scheduler::ticks(dt));
        calendar.advance(unbanished_game::scheduler::ticks(dt));
        let state = town.construction.sites[&id].machine.state;
        if tending_at.is_none() && state == 0xa {
            tending_at = Some(step);
            // Fast-forward: the trees are grown, their fruit ripe, and it is autumn.
            let trees: Vec<usize> = town.construction.sites[&id]
                .orchard
                .as_ref()
                .unwrap()
                .trees
                .iter()
                .filter_map(|t| t.entity)
                .collect();
            for t in trees {
                town.placed[t].growth = 1.0;
                town.construction.fruits.entry(t).or_default().ripeness = 1.0;
            }
            calendar.elapsed = unbanished_game::scheduler::ticks(8.0 * 300.0);
        }
        if picking_at.is_none() && state == 0xb {
            picking_at = Some(step);
        }
        let piles: Vec<usize> = town
            .construction
            .piles
            .iter()
            .filter(|(_, p)| p.material == fruit)
            .map(|(&e, _)| e)
            .collect();
        seen.extend(piles.iter().copied());
        if stored_at.is_none() && seen.iter().any(|e| !piles.contains(e)) {
            stored_at = Some(step);
            break;
        }
    }
    let o = town.construction.sites[&id].orchard.clone().unwrap();
    eprintln!(
        "tending {tending_at:?}, picking {picking_at:?}, stored {stored_at:?}, {} trees, \
         states {:?}",
        o.trees.len(),
        o.trees.iter().map(|t| t.state).collect::<Vec<_>>()
    );
    assert!(tending_at.is_some(), "the orchard was never planted");
    assert!(picking_at.is_some(), "the fruit was never picked");
    assert!(stored_at.is_some(), "no fruit was stored");
    assert_reservations_held(&town);
}

/// An easy start's livestock wait for a pasture; a fenced pasture is built with its shelter
/// and fences, takes them once its kind is chosen, its herdsmen tend it (a visit lets an
/// animal breed) and young are born; with a lower limit the herdsmen cull the oldest
/// grown animal, which leaves its raw materials.
#[test]
fn herdsmen_keep_a_pasture() {
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
    let waiting = town.wildlife.unassigned.clone();
    assert_eq!(waiting.len(), 8, "the starting livestock");
    let kind = town.wildlife.animals[&waiting[0]].template;
    assert!(town.construction.known.contains(&kind));
    let n = town.cells.size as i32;
    let size = 10;
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
    let rect = (8..60)
        .flat_map(|r| (-r..=r).flat_map(move |d| [(d, -r), (d, r), (-r, d), (r, d)]))
        .map(|(dx, dy)| (sx + dx, sy + dy))
        .find(|&(x, y)| x > 0 && y > 0 && empty(&town, x, y))
        .expect("no empty ground");
    let id = town
        .place_fenced(
            &archive,
            r"Template\Pasture.rsc:resource",
            rect,
            (rect.0 + size - 1, rect.1 + size - 1),
            0,
            &terrain,
        )
        .expect("the pasture was not placed");
    let parts = town.construction.sites[&id].parts.clone();
    let named = |t: &str| {
        parts
            .iter()
            .filter(|&&p| {
                town.templates[town.placed[p].template]
                    .reference
                    .contains(t)
            })
            .count()
    };
    eprintln!(
        "parts: {} shelter {} corners {} gates {} long {}",
        parts.len(),
        named("AnimalShelter"),
        named("WoodFenceCorner"),
        named("WoodFenceGate"),
        named("WoodFenceStraightLong")
    );
    assert_eq!(named("AnimalShelter"), 1);
    assert_eq!(named("WoodFenceCorner"), 3, "the shelter takes one corner");
    assert_eq!(named("WoodFenceGate"), 4, "a gate on each side");
    let builder = town.construction.professions.builder;
    let herdsman = town
        .construction
        .professions
        .list
        .iter()
        .position(|p| p.reference.to_ascii_lowercase().ends_with(":herdsman"))
        .expect("no herdsman profession");
    // Herdsmen hired before the pasture can take them find it once it does
    // (`FUN_140115490`).
    for (profession, count) in [(builder, 3), (herdsman, 2)] {
        town.construction
            .commands
            .push(unbanished_game::work::professions::Command::SetCount { profession, count });
    }
    let mut warnings = unbanished_game::events::StockWarnings {
        limits: [100_000; 21],
        ..Default::default()
    };
    let dt = 1.0 / 30.0;
    let (mut built_at, mut penned_at, mut bred_at, mut born_at, mut culled_at) =
        (None, None, None, None, None);
    let mut chose = false;
    let initial: std::collections::BTreeSet<usize> =
        town.wildlife.animals.keys().copied().collect();
    let beef = town.wildlife.animal_kinds[&kind]
        .materials
        .first()
        .and_then(|&m| town.templates[m].pile_material);
    let mut culled = 0;
    let mut herd = usize::MAX;
    for step in 0..30 * 60 * 40 {
        town.apply_commands(&terrain, &mut rng);
        town.iterate(&terrain, &mut rng, dt, 60.0);
        let stock = std::array::from_fn(|i| (warnings.count[i], warnings.limits[i]));
        town.update_citizens(&terrain, &mut rng, dt, 60.0, stock);
        warnings.recompute(&town.registered_storages(), &town.materials);
        town.construction
            .jobs
            .step(unbanished_game::scheduler::ticks(dt));
        let state = town.construction.sites[&id].machine.state;
        if built_at.is_none() && state == 2 {
            built_at = Some(step);
        }
        if built_at.is_some() && !chose {
            chose = true;
            town.construction
                .commands
                .push(unbanished_game::work::professions::Command::SetCrop {
                    entity: id,
                    crop: Some(kind),
                });
        }
        let pasture = &town.wildlife.pastures[&id];
        let cap = town.wildlife.capacity(id);
        if penned_at.is_none() && 0 < cap && pasture.livestock.len() as i32 == cap.min(8) {
            penned_at = Some(step);
        }
        if penned_at.is_some() && bred_at.is_none() {
            let breeder = pasture.livestock.iter().copied().find(|a| {
                let x = &town.wildlife.animals[a];
                x.flags & unbanished_game::nature::wildlife::BREEDER != 0
            });
            if let Some(a) = breeder {
                bred_at = Some(step);
                // Fast-forward: a grown male breeder with young almost due.
                let males: Vec<usize> = pasture
                    .livestock
                    .iter()
                    .copied()
                    .filter(|a| town.wildlife.animals[a].male)
                    .collect();
                let m = males.first().copied().unwrap_or(a);
                let x = town.wildlife.animals.get_mut(&m).unwrap();
                x.male = true;
                x.flags |= unbanished_game::nature::wildlife::BREEDER;
                x.age = 2.0;
                x.breed = 1;
            }
        }
        let young = town
            .wildlife
            .animals
            .iter()
            .any(|(a, x)| x.livestock.is_some() && !initial.contains(a));
        if bred_at.is_some() && born_at.is_none() && young {
            born_at = Some(step);
            // The herd is to shrink to two: the herdsmen cull.
            herd = town.wildlife.pastures[&id].livestock.len();
            for a in town.wildlife.pastures[&id].livestock.clone() {
                town.wildlife.animals.get_mut(&a).unwrap().age = 3.0;
            }
            town.construction.commands.push(
                unbanished_game::work::professions::Command::SetLimit {
                    entity: id,
                    limit: 2,
                },
            );
        }
        if born_at.is_some() {
            // The herdsmen carry the meat away themselves (the pasture's AutoPickup), so
            // the first cull is the check.
            let left = town.wildlife.pastures[&id].livestock.len();
            if left < herd && culled_at.is_none() {
                culled_at = Some(step);
            }
            culled = town
                .construction
                .piles
                .values()
                .filter(|p| Some(p.material) == beef)
                .count();
            if culled_at.is_some() && 0 < culled {
                break;
            }
        }
    }
    let p = &town.wildlife.pastures[&id];
    eprintln!(
        "built {built_at:?}, penned {penned_at:?}, bred {bred_at:?}, born {born_at:?}, \
         culled {culled_at:?}; {} animals, limit {}, {} meat piles",
        p.livestock.len(),
        p.limit,
        culled
    );
    assert!(built_at.is_some(), "the pasture was not built");
    assert!(
        penned_at.is_some(),
        "the livestock never joined the pasture"
    );
    assert!(bred_at.is_some(), "no herdsman's visit let an animal breed");
    assert!(born_at.is_some(), "no young were born");
    assert!(culled_at.is_some(), "the herd was not culled");
    assert!(0 < culled || beef.is_none(), "a culled animal left nothing");
    for &a in &town.wildlife.pastures[&id].livestock {
        let x = &town.wildlife.animals[&a];
        assert_eq!(x.flags & unbanished_game::nature::wildlife::WILD, 0);
        assert_eq!(x.area, town.wildlife.pastures[&id].area);
    }
}

/// Chickens in a pasture lay: every pile's worth gathered becomes an egg pile on a free
/// shelter point, which is then hauled to storage.
#[test]
fn a_pasture_gathers_eggs() {
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
            seed: 99,
            ..Default::default()
        },
    )
    .unwrap();
    let terrain = HeightGrid {
        width: world.terrain.width(),
        heights: &world.terrain.heights,
    };
    let mut rng = Random::new(99);
    let mut town = StartingTown::create(
        &archive,
        "Game/StartCondition/Medium.rsc:resource",
        world.terrain.size,
        &terrain,
        &mut rng,
    )
    .unwrap();
    let chicken = town
        .wildlife
        .livestock_kinds
        .iter()
        .find(|(_, k)| k.product.is_some() && k.interval == 1.0)
        .map(|(&t, _)| t)
        .expect("no chickens");
    let egg = town.wildlife.livestock_kinds[&chicken]
        .product
        .and_then(|p| town.templates[p].pile_material)
        .expect("no egg material");
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
    let rect = (8..60)
        .flat_map(|r| (-r..=r).flat_map(move |d| [(d, -r), (d, r), (-r, d), (r, d)]))
        .map(|(dx, dy)| (sx + dx, sy + dy))
        .find(|&(x, y)| x > 0 && y > 0 && empty(&town, x, y))
        .expect("no empty ground");
    let id = town
        .place_fenced(
            &archive,
            r"Template\Pasture.rsc:resource",
            rect,
            (rect.0 + size - 1, rect.1 + size - 1),
            2,
            &terrain,
        )
        .expect("the pasture was not placed");
    // Selecting the pasture lights up its fence and shelter too (`FUN_140101eb0`).
    let mut highlight = crate::gfx::highlight::Highlight::default();
    let parts = town.construction.sites[&id].parts.clone();
    assert!(parts.len() > 1);
    let one = highlight.geometry(
        &archive,
        &town,
        &terrain,
        Some(crate::tools::selection::Entity::Placed(parts[0])),
    );
    let all = highlight.geometry(
        &archive,
        &town,
        &terrain,
        Some(crate::tools::selection::Entity::Placed(id)),
    );
    assert!(!one.is_empty() && one.edge_count() < all.edge_count());
    let builder = town.construction.professions.builder;
    town.construction
        .commands
        .push(unbanished_game::work::professions::Command::SetCount {
            profession: builder,
            count: 3,
        });
    let mut warnings = unbanished_game::events::StockWarnings {
        limits: [100_000; 21],
        ..Default::default()
    };
    let dt = 1.0 / 30.0;
    let (mut built_at, mut laid_at, mut stored_at) = (None, None, None);
    let mut seen = std::collections::BTreeSet::new();
    for step in 0..30 * 60 * 20 {
        town.apply_commands(&terrain, &mut rng);
        town.iterate(&terrain, &mut rng, dt, 60.0);
        let stock = std::array::from_fn(|i| (warnings.count[i], warnings.limits[i]));
        town.update_citizens(&terrain, &mut rng, dt, 60.0, stock);
        warnings.recompute(&town.registered_storages(), &town.materials);
        town.construction
            .jobs
            .step(unbanished_game::scheduler::ticks(dt));
        let state = town.construction.sites[&id].machine.state;
        if built_at.is_none() && state == 2 {
            built_at = Some(step);
            town.construction
                .commands
                .push(unbanished_game::work::professions::Command::SetCrop {
                    entity: id,
                    crop: Some(chicken),
                });
            let herdsman = town
                .construction
                .professions
                .list
                .iter()
                .position(|p| p.reference.to_ascii_lowercase().ends_with(":herdsman"))
                .expect("no herdsman profession");
            town.construction.commands.push(
                unbanished_game::work::professions::Command::SetCount {
                    profession: herdsman,
                    count: 1,
                },
            );
        }
        let pasture = &town.wildlife.pastures[&id];
        if pasture.kind == Some(chicken) && pasture.livestock.is_empty() {
            // Four hens, bought in (`FUN_14012db40` without a parent).
            town.with_wild(&terrain, &mut rng, |w| {
                for _ in 0..4 {
                    w.give_birth(id, None, true);
                }
            });
        }
        if built_at.is_some() && laid_at.is_none() {
            // Fast-forward: the hens have laid three piles' worth.
            if town.wildlife.pastures[&id].kind == Some(chicken)
                && town.wildlife.pastures[&id].produced == 0
            {
                let amount = town.materials[egg].pile_amount;
                town.wildlife.pastures.get_mut(&id).unwrap().produced = amount * 3 + 1;
            }
        }
        let piles: Vec<usize> = town
            .construction
            .piles
            .iter()
            .filter(|(_, p)| p.material == egg)
            .map(|(&e, _)| e)
            .collect();
        if laid_at.is_none() && !piles.is_empty() {
            laid_at = Some(step);
        }
        seen.extend(piles.iter().copied());
        if stored_at.is_none() && seen.iter().any(|e| !piles.contains(e)) {
            stored_at = Some(step);
            break;
        }
    }
    let p = &town.wildlife.pastures[&id];
    eprintln!(
        "built {built_at:?}, laid {laid_at:?}, stored {stored_at:?}; {} hens, {} piles seen",
        p.livestock.len(),
        seen.len()
    );
    assert!(built_at.is_some(), "the pasture was not built");
    assert!(4 <= p.livestock.len());
    assert!(laid_at.is_some(), "no eggs were laid");
    assert!(stored_at.is_some(), "no eggs were stored");
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
