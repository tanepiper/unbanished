//! Pastures: herding livestock and gathering eggs.

use super::*;

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
