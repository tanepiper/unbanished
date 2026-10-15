//! Every shipped entity template and its model / natural-resource components parse completely
//! (skipped without game-data/).

use banished_formats::game::{
    component_description, model_description, natural_resource_description,
};
use banished_formats::pkg::{Archive, Package};
use std::path::PathBuf;

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
    use banished_formats::game::{growth_description, map_description, spawn_description};
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
    use banished_formats::game::create_placed_description;
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
    use banished_formats::game::{raw_material_description, storage_description};
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
    use banished_formats::game::citizen_description;
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
    use banished_formats::game::build_description;
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
    use banished_formats::game::{external_list, profession};
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
        let c = banished_formats::game::character_description(&data)
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
        let d = banished_formats::game::interact_description(&archive.read(name).unwrap())
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        // A few (the Cemetery) have no points.
        let Some(r) = d.points else { continue };
        let file = r.split(':').next().unwrap().replace('\\', "/");
        let list = banished_formats::game::point_list(
            &archive.read_ref(&format!("{file}:resource")).unwrap(),
        )
        .unwrap_or_else(|e| panic!("{name} {r}: {e}"));
        let uses = banished_formats::game::interaction_points(&list, 0).len();
        if name.contains("StorageBarn") || name.contains("StorageCart") {
            eprintln!("{name}: {r} use {uses} {:?}", list.points);
        }
        n += 1;
    }
    assert!(n >= 25, "{n}");
}
