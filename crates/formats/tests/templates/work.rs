//! The workplaces' descriptions: produce, herbalists, foresters, fishing, wildlife, pastures, markets and quarries.

use super::*;

/// Every shipped ConsumeProduceDescription parses completely (docs/SHIPPED-PRODUCTION-RECIPES.md).
#[test]
fn consume_produce_descriptions_parse() {
    use unbanished_formats::{crc::type_id, crs, game::consume_produce_description};
    let Some(archive) = archive() else { return };
    let mut found = Vec::new();
    for name in archive.names() {
        if !name.ends_with(".crs") {
            continue;
        }
        let bytes = archive.read(name).unwrap();
        if crs::header(&bytes).unwrap().type_id == type_id("ConsumeProduceDescription") {
            let d = consume_produce_description(&bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
            found.push((name.to_string(), d));
        }
    }
    assert_eq!(found.len(), 6);
    let cutter = &found
        .iter()
        .find(|(n, _)| n.contains("CutterYard"))
        .unwrap()
        .1;
    assert_eq!(cutter.recipes.len(), 1);
    assert_eq!(cutter.recipes[0].inputs.len(), 1);
    assert_eq!(cutter.recipes[0].inputs[0].1, 1);
    assert_eq!(cutter.recipes[0].group, 3);
    assert_eq!(
        (cutter.cycles, cutter.duration, cutter.lifetime),
        (1, 4.0, -1)
    );
    let tavern = &found.iter().find(|(n, _)| n.contains("Tavern")).unwrap().1;
    assert_eq!(tavern.recipes.len(), 7);
    let mine = &found
        .iter()
        .find(|(n, _)| n.contains("IronMine"))
        .unwrap()
        .1;
    assert_eq!((mine.cycles, mine.work_type, mine.lifetime), (60, 1, 2000));
}

#[test]
fn herbalist_and_auto_pickup_descriptions_parse() {
    use unbanished_formats::{
        crc::type_id,
        crs,
        game::{auto_pickup_description, herbalist_description},
    };
    let Some(archive) = archive() else { return };
    let mut counts = (0, 0);
    for name in archive.names() {
        if !name.ends_with(".crs") {
            continue;
        }
        let bytes = archive.read(name).unwrap();
        let h = crs::header(&bytes).unwrap();
        if h.type_id == type_id("HerbalistDescription") {
            let d = herbalist_description(&bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert!(!d.resources.is_empty(), "{name}");
            counts.0 += 1;
        } else if h.type_id == type_id("AutoPickupDescription") {
            auto_pickup_description(&bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
            counts.1 += 1;
        }
    }
    assert!(counts.0 >= 2 && counts.1 >= 2, "{counts:?}");
    let gatherer = herbalist_description(
        &archive
            .read_ref("Template/GathererShelter.rsc:herbalist")
            .unwrap(),
    )
    .unwrap();
    assert_eq!(gatherer.resources.len(), 4);
    assert_eq!(gatherer.group, 5);
    let herbalist = herbalist_description(
        &archive
            .read_ref("Template/Herbalist.rsc:herbalist")
            .unwrap(),
    )
    .unwrap();
    assert_eq!(herbalist.group, 6);
    assert!(auto_pickup_description(
        &archive
            .read_ref("Template/GathererShelter.rsc:autopickup")
            .unwrap()
    )
    .unwrap());
}

#[test]
fn forester_description_parses() {
    use unbanished_formats::game::forester_description;
    let Some(archive) = archive() else { return };
    let d = forester_description(
        &archive
            .read_ref("Template/ForesterLodge.rsc:forester")
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        d.tree.as_deref(),
        Some(r"Template\NaturalResourceTree.rsc:resource")
    );
    assert_eq!(d.group, 0);
}

#[test]
fn fishing_description_parses() {
    use unbanished_formats::game::fishing_description;
    let Some(archive) = archive() else { return };
    let d = fishing_description(
        &archive
            .read_ref("Template/FishermansDock.rsc:fishing")
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        d.output.as_deref(),
        Some(r"Template\RawMaterialFish.rsc:resource")
    );
    assert_eq!((d.mask, d.group), (0x180, 5));
}

#[test]
fn wildlife_descriptions_parse() {
    use unbanished_formats::game::{
        animal_description, herd_description, hunt_description, population_description,
    };
    let Some(archive) = archive() else { return };
    let read = |r: &str| archive.read_ref(r).unwrap();
    let p = population_description(&read("Template/PopulationDeer.rsc:population")).unwrap();
    assert_eq!(p.name, "Deer Population");
    assert_eq!(
        (p.cell, p.herd_block, p.habitat, p.forbidden),
        (8, 64, 0x17, 0x68)
    );
    assert_eq!(
        (p.no_occupants, p.capacity, p.regrowth, p.refresh),
        (true, 2, 1, true)
    );
    assert_eq!(
        p.herd.as_deref(),
        Some(r"Template\PopulationDeer.rsc:deerHerd")
    );
    let h = herd_description(&read("Template/PopulationDeer.rsc:herd")).unwrap();
    assert_eq!((h.max, h.min, h.area), (10, 5, 8));
    assert_eq!((h.rest, h.rest_range, h.regrowth), (1.0, 0.75, 12.0));
    let a = animal_description(&read("Template/WildAnimalDeer.rsc:animal")).unwrap();
    assert_eq!(a.materials.len(), 2);
    assert_eq!((a.area, a.life, a.life_range, a.adult), (8, 3.0, 0.5, 1.0));
    let hunt = hunt_description(&read("Template/HunterLodge.rsc:hunt")).unwrap();
    assert_eq!((hunt.time, hunt.group), (2.0, 5));
}

#[test]
fn pasture_descriptions_parse() {
    use unbanished_formats::game::{
        animal_description, create_fenced_description, create_placed_description,
        livestock_description, pasture_description,
    };
    let Some(archive) = archive() else { return };
    let read = |r: &str| archive.read_ref(r).unwrap();
    assert_eq!(
        pasture_description(&read("Template/Pasture.rsc:pasture")).unwrap(),
        5
    );
    let f = create_fenced_description(&read("Template/Pasture.rsc:createfenced")).unwrap();
    assert_eq!(f.mask, 0x14);
    assert_eq!(
        f.parts[0].as_deref(),
        Some(r"Template\AnimalShelter.rsc:resource")
    );
    assert_eq!(
        f.parts[5].as_deref(),
        Some(r"Template\WoodFenceGate.rsc:resource")
    );
    assert_eq!((f.max, f.min, f.centered), ((20, 20), (7, 7), false));
    assert_eq!(f.clear.as_deref(), Some(r"Template\Clear.rsc:resource"));
    let shelter = create_placed_description(&read("Template/AnimalShelter.rsc:createplaced"));
    let shelter = shelter.unwrap();
    assert!(shelter.width > 0 && shelter.height > 0);
    let cow = livestock_description(&read("Template/LivestockCow.rsc:livestock")).unwrap();
    assert_eq!((cow.product, cow.interval, cow.group), (None, 1.0, 5));
    let sheep = livestock_description(&read("Template/LivestockSheep.rsc:livestock")).unwrap();
    assert_eq!(
        sheep.product.as_deref(),
        Some(r"Template\RawMaterialWool.rsc:resource")
    );
    assert_eq!(sheep.interval, 3.0);
    let a = animal_description(&read("Template/LivestockCow.rsc:animal")).unwrap();
    assert_eq!(
        (a.area, a.space, a.life, a.adult, a.breed),
        (5, 20, 6.0, 0.8, 0.6)
    );
}

#[test]
fn market_description_parses() {
    use unbanished_formats::game::market_description;
    let Some(archive) = archive() else { return };
    let d = market_description(&archive.read_ref("Template/Market.rsc:market").unwrap()).unwrap();
    assert!((d.threshold - 0.99).abs() < 1e-6);
    assert_eq!(d.quotas.len(), 11);
    assert_eq!(d.quotas[0].0, 1);
    assert!((d.quotas[0].1 - 0.168).abs() < 1e-3);
    assert_eq!(d.quotas[10].0, 0x10000);
}

#[test]
fn quarry_description_parses() {
    let Some(archive) = archive() else { return };
    let q = unbanished_formats::game::quarry_description(
        &archive.read_ref("Template/Quarry.rsc:quarry").unwrap(),
    )
    .unwrap();
    let mesh = unbanished_formats::picking::picking_mesh(
        &archive
            .read_ref(&q.mesh.clone().unwrap().replace('\\', "/"))
            .unwrap(),
    )
    .unwrap();
    assert_eq!((q.shallow, q.deep), (-1.4, -7.0));
    assert!(100 < mesh.meshes[0].vertices.len());
    let cp = unbanished_formats::game::consume_produce_description(
        &archive
            .read_ref("Template/Quarry.rsc:consumeproduce")
            .unwrap(),
    )
    .unwrap();
    assert_eq!(cp.lifetime, 2000);
}
