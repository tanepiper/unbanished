//! Templates, models, nature, storage, construction, removal, decals and particles.

use super::*;

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
