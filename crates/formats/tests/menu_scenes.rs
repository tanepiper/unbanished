//! All ten main-menu scenes parse completely (skipped without game-data/).

use std::path::PathBuf;
use unbanished_formats::menu_scene::menu_scene;
use unbanished_formats::pkg::{Archive, Package};

#[test]
fn menu_scenes_parse() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../game-data/original/WinData");
    let mut archive = Archive::default();
    for name in ["data1.pkg", "patch1.pkg", "patch0.pkg", "data0.pkg"] {
        let Ok(bytes) = std::fs::read(dir.join(name)) else {
            eprintln!("game data missing; skipping");
            return;
        };
        archive.add(Package::parse(bytes).unwrap());
    }
    for i in 0..10 {
        let reference = format!("Game/MenuScene/Scene{i}.rsc:resource");
        let s = menu_scene(&archive.read_ref(&reference).unwrap())
            .unwrap_or_else(|e| panic!("{reference}: {e}"));
        let m = &s.map;
        let w = m.dims[4] as usize;
        assert_eq!(m.heights.len(), w * w, "{reference}: heights");
        eprintln!(
            "scene {i} '{}': size {} | {} water verts, {} rivers, {} lakes, {} decals (+{}), \
             {} placements of {} meshes, {} particles | sky {:?} | colours {:08x?} floats {:?} \
             | camera pos {:?} target {:?} {:?} | user {:?}",
            s.name,
            m.dims[0],
            m.water_vertices.len(),
            m.river_ranges.len(),
            m.lake_ranges.len(),
            m.decals.len(),
            m.extra_decals.len(),
            s.placements.len(),
            s.meshes.len(),
            s.particles.len(),
            s.sky,
            s.colors,
            s.floats,
            s.camera.position,
            s.camera.target,
            s.camera.floats,
            s.vector,
        );
    }
}
