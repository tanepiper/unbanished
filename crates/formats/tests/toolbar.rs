//! Every shipped toolbar entry parses completely (skipped without game-data/).

use std::path::PathBuf;
use unbanished_formats::game::toolbar_item;
use unbanished_formats::pkg::{Archive, Package};

fn common_archive() -> Option<Archive> {
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
fn toolbar_items_parse() {
    let Some(archive) = common_archive() else {
        return;
    };
    let names: Vec<String> = archive
        .names()
        .into_iter()
        .filter(|n| {
            n.replace('\\', "/").starts_with("Game/Toolbar_") && !n.ends_with("_resource.crs")
        })
        .map(str::to_owned)
        .collect();
    assert!(names.len() > 60);
    for n in names {
        let item = toolbar_item(&archive.read(&n).unwrap()).unwrap_or_else(|e| panic!("{n}: {e}"));
        if let Some(t) = &item.template {
            let r = t.replace("resource", "toolbar").replace('\\', "/");
            let d = unbanished_formats::game::toolbar_description(&archive.read_ref(&r).unwrap())
                .unwrap_or_else(|e| panic!("{r}: {e}"));
            eprintln!(
                "  {r}: sprite '{}' name '{}' tip '{}' {:?}",
                d.sprite, d.name, d.tooltip, d.messages
            );
        }
        eprintln!(
            "{n}: kind {} parent {:?} order {} value {} vis {} en {} id {} sprite '{}' tip '{}' template {:?}",
            item.kind, item.parent, item.order, item.value, item.visible, item.enabled, item.id,
            item.sprite, item.tooltip, item.template
        );
    }
}

#[test]
fn ui_descriptions_parse() {
    let Some(archive) = common_archive() else {
        return;
    };
    let mut n = 0;
    for name in archive.names() {
        let path = name.replace('\\', "/");
        if path.starts_with("Template/") && path.ends_with("_ui.crs") {
            let d = unbanished_formats::game::ui_description(&archive.read(name).unwrap())
                .unwrap_or_else(|e| panic!("{name}: {e}"));
            println!("{name}: {d:?}");
            n += 1;
        }
    }
    assert!(n > 0);
}
