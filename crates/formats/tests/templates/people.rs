//! Citizens, professions, characters, interaction points, status icons, happiness, disease and cemeteries.

use super::*;

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
