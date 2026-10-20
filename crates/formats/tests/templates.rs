//! Every shipped entity template and its model / natural-resource components parse completely
//! (skipped without game-data/).

use std::path::PathBuf;
use unbanished_formats::game::{
    component_description, model_description, natural_resource_description,
};
use unbanished_formats::pkg::{Archive, Package};

fn archive() -> Option<Archive> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../game-data/original/WinData");
    let mut archive = Archive::default();
    for name in ["data1.pkg", "patch1.pkg", "patch0.pkg", "data0.pkg"] {
        let Ok(bytes) = std::fs::read(dir.join(name)) else {
            eprintln!("game data missing; skipping");
            return None;
        };
        archive.add(Package::parse(bytes).unwrap());
    }
    Some(archive)
}

#[test]
fn storage_search_descriptions_parse() {
    use unbanished_formats::{
        crc::type_id,
        crs,
        game::{radius_description, storage_location_description},
    };
    let Some(archive) = archive() else { return };
    let mut counts = (0, 0);
    for name in archive.names() {
        if !name.ends_with(".crs") {
            continue;
        }
        let bytes = archive.read(name).unwrap();
        let h = crs::header(&bytes).unwrap();
        if h.type_id == type_id("StorageLocationDescription") {
            storage_location_description(&bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
            counts.0 += 1;
        } else if h.type_id == type_id("RadiusDescription") {
            let (radius, _) = radius_description(&bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert!(radius > 0, "{name}");
            counts.1 += 1;
        }
    }
    assert_eq!(counts, (4, 14));
    assert_eq!(
        storage_location_description(
            &archive
                .read_ref("Template/StorageCart.rsc:storagelocation")
                .unwrap()
        )
        .unwrap(),
        0
    );
}

fn entries<'a>(archive: &'a Archive, suffix: &str) -> Vec<&'a str> {
    archive
        .names()
        .into_iter()
        .filter(|n| n.replace('\\', "/").starts_with("Template/") && n.ends_with(suffix))
        .collect()
}

#[test]
fn templates_parse() {
    let Some(archive) = archive() else { return };
    let mut n = 0;
    for name in entries(&archive, "_resource.crs") {
        let data = archive.read(name).unwrap();
        // Template files also hold other `_resource` classes (raw materials are templates too).
        match component_description(&data) {
            Ok(t) => {
                assert!(t.bucket <= 6, "{name}");
                n += 1;
            }
            Err(e) if e.to_string().contains("expected ComponentDescription") => {}
            Err(e) => panic!("{name}: {e}"),
        }
    }
    // 155 shipped templates, 148 of them under Template/.
    assert_eq!(n, 148);
}

#[test]
fn models_parse() {
    let Some(archive) = archive() else { return };
    let names = entries(&archive, "_model.crs");
    assert!(names.len() > 100);
    for name in names {
        model_description(&archive.read(name).unwrap()).unwrap_or_else(|e| panic!("{name}: {e}"));
    }
    let tree = model_description(
        &archive
            .read_ref("Template/NaturalResourceTree.rsc:model")
            .unwrap(),
    )
    .unwrap();
    assert_eq!(tree.variants.len(), 3);
    assert_eq!(tree.variants[0].meshes.len(), 4);
}

#[test]
fn natural_resources_parse() {
    let Some(archive) = archive() else { return };
    let names = entries(&archive, "_naturalresource.crs");
    assert!(names.len() >= 24);
    for name in names {
        natural_resource_description(&archive.read(name).unwrap())
            .unwrap_or_else(|e| panic!("{name}: {e}"));
    }
    let tree = natural_resource_description(
        &archive
            .read_ref("Template/NaturalResourceTree.rsc:naturalresource")
            .unwrap(),
    )
    .unwrap();
    assert_eq!(tree.noise_scale, 30.0);
    assert_eq!(tree.random_variant_one_in, 5);
}

#[test]
fn growth_spawn_map_parse() {
    use unbanished_formats::game::{growth_description, map_description, spawn_description};
    let Some(archive) = archive() else { return };
    let growth = entries(&archive, "_growth.crs");
    assert_eq!(growth.len(), 22);
    for name in growth {
        growth_description(&archive.read(name).unwrap()).unwrap_or_else(|e| panic!("{name}: {e}"));
    }
    let maps = entries(&archive, "_map.crs");
    assert!(maps.len() >= 100);
    for name in maps {
        map_description(&archive.read(name).unwrap()).unwrap_or_else(|e| panic!("{name}: {e}"));
    }
    let spawn = spawn_description(
        &archive
            .read_ref("Template/NaturalResourceTree.rsc:spawn")
            .unwrap(),
    )
    .unwrap();
    assert!(!spawn.is_empty());
    let tree = map_description(
        &archive
            .read_ref("Template/NaturalResourceTree.rsc:map")
            .unwrap(),
    )
    .unwrap();
    assert_eq!(tree.cell_type, 4);
}

#[test]
fn create_placed_parse() {
    use unbanished_formats::game::create_placed_description;
    let Some(archive) = archive() else { return };
    let names = entries(&archive, "_createplaced.crs");
    assert!(names.len() > 40, "{}", names.len());
    for name in names {
        create_placed_description(&archive.read(name).unwrap())
            .unwrap_or_else(|e| panic!("{name}: {e}"));
    }
    let barn = create_placed_description(
        &archive
            .read_ref("Template/StorageBarn.rsc:createplaced")
            .unwrap(),
    )
    .unwrap();
    println!("{barn:?}");
    assert!(barn.width > 0 && barn.height > 0);
}

#[test]
fn raw_materials_and_storage_parse() {
    use unbanished_formats::game::{raw_material_description, storage_description};
    let Some(archive) = archive() else { return };
    for name in entries(&archive, "_rawmaterial.crs") {
        let r = raw_material_description(&archive.read(name).unwrap())
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        println!(
            "{name}: size {} flags {:#x} {:?}",
            r.size,
            r.flags,
            (r.value_40, r.value_44, r.value_4c, r.value_50, r.value_54)
        );
    }
    for name in entries(&archive, "_storage.crs") {
        let s = storage_description(&archive.read(name).unwrap())
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        println!("{name}: {s:?}");
    }
}

#[test]
fn citizen_description_parses() {
    use unbanished_formats::game::citizen_description;
    let Some(archive) = archive() else { return };
    let c =
        citizen_description(&archive.read_ref("Template/Citizen.rsc:citizen").unwrap()).unwrap();
    println!(
        "adult {} start {:?} lifespan {:?} edu {} health {} happy {}",
        c.adult_age(),
        c.start_age(),
        c.lifespan(),
        c.i(0x118),
        c.f(0x70),
        c.f(0xa4)
    );
    assert!(c.adult_age() > 0.0);
}

#[test]
fn build_descriptions_parse() {
    use unbanished_formats::game::build_description;
    let Some(archive) = archive() else { return };
    let names = entries(&archive, "_build.crs");
    assert!(names.len() >= 27, "{}", names.len());
    for name in names {
        build_description(&archive.read(name).unwrap()).unwrap_or_else(|e| panic!("{name}: {e}"));
    }
    let house =
        build_description(&archive.read_ref("Template/WoodHouse.rsc:build").unwrap()).unwrap();
    assert_eq!(house.materials.len(), 2);
    assert_eq!(house.materials[0].0, 16);
}

#[test]
fn professions_parse() {
    use unbanished_formats::game::{external_list, profession};
    let Some(archive) = archive() else { return };
    let list = external_list(
        &archive
            .read_ref("Game/Profession/Profession.rsc:professionList")
            .unwrap(),
    )
    .unwrap();
    assert!(list.len() > 15, "{}", list.len());
    for r in &list {
        let path = r.replace('\\', "/");
        profession(&archive.read_ref(&path).unwrap()).unwrap_or_else(|e| panic!("{r}: {e}"));
    }
    println!("{list:?}");
}

#[test]
fn character_descriptions_parse() {
    let Some(archive) = archive() else { return };
    let mut n = 0;
    for name in entries(&archive, "_character.crs") {
        let data = archive.read(name).unwrap();
        let c = unbanished_formats::game::character_description(&data)
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        if name.contains("Citizen_") {
            eprintln!("{name}: {c:?}");
        }
        n += 1;
    }
    assert!(n >= 6, "{n}");
}

#[test]
fn interact_descriptions_resolve_points() {
    let Some(archive) = archive() else { return };
    let mut n = 0;
    for name in entries(&archive, "_interact.crs") {
        let d = unbanished_formats::game::interact_description(&archive.read(name).unwrap())
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        // A few (the Cemetery) have no points.
        let Some(r) = d.points else { continue };
        let file = r.split(':').next().unwrap().replace('\\', "/");
        let list = unbanished_formats::game::point_list(
            &archive.read_ref(&format!("{file}:resource")).unwrap(),
        )
        .unwrap_or_else(|e| panic!("{name} {r}: {e}"));
        let uses = unbanished_formats::game::interaction_points(&list, 0).len();
        if name.contains("StorageBarn") || name.contains("StorageCart") {
            eprintln!("{name}: {r} use {uses} {:?}", list.points);
        }
        n += 1;
    }
    assert!(n >= 25, "{n}");
}

#[test]
fn status_icon_descriptions_parse() {
    let Some(archive) = archive() else { return };
    let mut n = 0;
    for name in entries(&archive, "_statusicon.crs") {
        let d = unbanished_formats::game::status_icon_description(&archive.read(name).unwrap())
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(d.sheet.is_some() && d.size > 0.0, "{name}");
        n += 1;
    }
    assert_eq!(n, 31);
}

/// The removal tools' descriptions and every CreateDragDescription parse completely.
#[test]
fn removal_descriptions_parse() {
    use unbanished_formats::{
        crc::type_id,
        crs,
        game::{create_drag_description, create_removal_description},
    };
    let Some(archive) = archive() else { return };
    let mut drags = 0;
    for name in archive.names() {
        if !name.ends_with(".crs") {
            continue;
        }
        let bytes = archive.read(name).unwrap();
        if crs::header(&bytes).unwrap().type_id == type_id("CreateDragDescription") {
            create_drag_description(&bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
            drags += 1;
        }
    }
    assert!(drags > 0);
    let read = |t: &str| {
        create_removal_description(
            &archive
                .read_ref(&format!("Template/{t}.rsc:createremoval"))
                .unwrap(),
        )
        .unwrap()
    };
    let tree = read("ClearTree");
    assert!(tree.resources && !tree.roads && !tree.cancel);
    assert_eq!(tree.filters, [r"Template\NaturalResourceTree.rsc:resource"]);
    assert_eq!(
        tree.template.as_deref(),
        Some(r"Template\Clear.rsc:resource")
    );
    assert!(read("ClearResources").filters.is_empty());
    assert!(read("ClearRoad").roads);
    assert!(read("ClearBuildings").buildings);
    assert!(read("ClearCancel").cancel);
    assert!(read("ClearPriority").priority);
    let clear =
        create_drag_description(&archive.read_ref("Template/Clear.rsc:createdrag").unwrap())
            .unwrap();
    assert_eq!(clear.mask, 0x14);
    assert!(!clear.override_occupant);
}

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
fn happiness_descriptions_parse() {
    use unbanished_formats::game::happiness_description;
    let Some(archive) = archive() else { return };
    let read = |n: &str| {
        happiness_description(
            &archive
                .read_ref(&format!("Template/{n}.rsc:happiness"))
                .unwrap(),
        )
        .unwrap()
    };
    let tavern = read("Tavern");
    assert_eq!(
        (tavern.kind, tavern.needs_workers, tavern.needs_stock),
        (2, true, true)
    );
    assert_eq!((tavern.mask, tavern.pad), (0x20000, 3));
    assert_eq!(read("Well").kind, 0);
    assert_eq!(read("Quarry").kind, 5);
    assert_eq!(read("DoctorHouse").pad, 5);
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
fn disease_descriptions_parse() {
    use unbanished_formats::game::disease_description;
    let Some(archive) = archive() else { return };
    let read = |r: &str| disease_description(&archive.read_ref(r).unwrap()).unwrap();
    let field = read("Template/CropField.rsc:disease");
    assert_eq!(
        (field.months, field.odds, field.odds_same),
        ((3, 7), 200.0, 150.0)
    );
    assert_eq!((field.check, field.duration, field.radius), (0.25, 8.0, 20));
    let pasture = read("Template/Pasture.rsc:disease");
    assert_eq!(
        (pasture.months, pasture.kill, pasture.radius),
        ((0, 12), 0.2, 40)
    );
    read("Template/Orchard.rsc:disease");
}

#[test]
fn cemetery_descriptions_parse() {
    use unbanished_formats::game::{cemetery_description, tombstone_description};
    let Some(archive) = archive() else { return };
    let read = |r: &str| archive.read_ref(r).unwrap();
    assert_eq!(
        cemetery_description(&read("Template/Cemetery.rsc:cemetery"))
            .unwrap()
            .as_deref(),
        Some(r"Template\Tombstone.rsc:resource")
    );
    assert_eq!(
        tombstone_description(&read("Template/Tombstone.rsc:tombstone")).unwrap(),
        (60.0, 24.0)
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
fn decal_descriptions_parse() {
    use unbanished_formats::game::decal_description;
    let Some(archive) = archive() else { return };
    let names = entries(&archive, "_decal.crs");
    assert_eq!(names.len(), 33);
    for name in &names {
        let d = decal_description(&archive.read(name).unwrap())
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(!d.materials.is_empty(), "{name}");
        // A picture has a rectangle per model variant; a tiled decal has none.
        assert_eq!(d.tiled, d.rects.is_empty(), "{name}");
    }
    let house =
        decal_description(&archive.read_ref("Template/WoodHouse.rsc:decal").unwrap()).unwrap();
    assert_eq!(
        (house.alpha, house.scale, house.rects.len()),
        (0.5, 2048.0, 6)
    );
    assert_eq!(house.rects[1], [256.0, 0.0, 512.0, 320.0]);
}

#[test]
fn particles_parse() {
    use unbanished_formats::{crc::type_id, crs, game};
    let Some(archive) = archive() else { return };
    let mut counts = (0, 0);
    for name in archive.names() {
        if !name.ends_with(".crs") {
            continue;
        }
        let bytes = archive.read(name).unwrap();
        let h = crs::header(&bytes).unwrap();
        if h.type_id == type_id("ParticleSystem") {
            game::particle_system(&bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
            counts.0 += 1;
        } else if h.type_id == type_id("ParticleDescription") {
            game::particle_description(&bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
            counts.1 += 1;
        }
    }
    assert_eq!(counts, (6, 26));
    let smoke = game::particle_system(
        &archive
            .read_ref("ParticleSystems/ChimneySmoke/ChimneySmoke.rsc:resource")
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        (smoke.rate, smoke.max_particles, smoke.life),
        (5.0, 32, [4.5, 1.0])
    );
    assert_eq!(smoke.colors, [0xc0c0_c0c0, 0xc0ff_ffff]);
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

/// Profession `+0x4d` (no disease while at work) is the doctor's alone.
#[test]
fn only_the_doctor_is_immune_at_work() {
    use unbanished_formats::game::{external_list, profession};
    let Some(archive) = archive() else { return };
    let list = external_list(
        &archive
            .read_ref("Game/Profession/Profession.rsc:professionList")
            .unwrap(),
    )
    .unwrap();
    let immune: Vec<String> = list
        .iter()
        .filter(|r| {
            let d = archive.read_ref(&r.replace('\\', "/")).unwrap();
            profession(&d).unwrap().flag_4d
        })
        .cloned()
        .collect();
    assert_eq!(immune.len(), 1, "{immune:?}");
    assert!(immune[0].to_lowercase().contains("doctor"), "{immune:?}");
}

#[test]
fn tornado_particles_parse() {
    use unbanished_formats::game;
    let Some(archive) = archive() else { return };
    let t = game::tornado_particle(
        &archive
            .read_ref("ParticleSystems/Tornado/Tornado.rsc:resource")
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        (t.max_particles, t.radius, t.rise, t.wobble, t.spiral_turn),
        (750, 20.0, 75.0, 26.0, 400.0)
    );
    assert_eq!(
        (t.rate, t.life, t.size, t.size_scale),
        (20.0, [30.0, 2.0], [2.5, 15.0], 30.0)
    );
    assert_eq!(t.colors, [0xffc0_c0c0, 0x8090_9090]);
    assert_eq!(
        (t.fade_in, t.fade_out, t.wobble_turn),
        (50_000, 1_000_000, 30.0)
    );
    let slot =
        &game::particle_description(&archive.read_ref("Template/Tornado.rsc:particle").unwrap())
            .unwrap()
            .slots[0];
    assert_eq!(
        (slot.fade, slot.prewarm, slot.flags[1]),
        (10_000_000, 32_000_000, true)
    );
}
