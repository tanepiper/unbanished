//! Orchards: planting, tending and picking.

use super::*;

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
