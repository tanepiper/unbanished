//! Every shipped entity template and its model / natural-resource components parse completely
//! (skipped without game-data/).

use std::path::PathBuf;
use unbanished_formats::game::{
    component_description, model_description, natural_resource_description,
};
use unbanished_formats::pkg::{Archive, Package};

mod objects;
mod people;
mod work;

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
