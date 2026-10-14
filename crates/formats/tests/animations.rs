//! AnimationGroup playback metadata across the player's local package corpus.
use banished_formats::pkg::{Archive, Package};
use banished_formats::types::{animation_group, graphics_mesh};
use std::path::PathBuf;

#[test]
fn animation_metadata_and_tree_geometry_parse() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../game-data/original/WinData");
    let mut archive = Archive::default();
    for name in ["data1.pkg", "patch1.pkg", "patch0.pkg", "data0.pkg"] {
        let Ok(data) = std::fs::read(root.join(name)) else {
            eprintln!("game data missing; skipping animation metadata corpus");
            return;
        };
        archive.add(Package::parse(data).unwrap());
    }
    let names: Vec<_> = archive
        .names()
        .into_iter()
        .filter(|name| name.contains("Anims_resource.crs"))
        .map(str::to_owned)
        .collect();
    assert_eq!(names.len(), 15);
    let mut total = 0;
    for name in names {
        let group = animation_group(&archive.read(&name).unwrap()).unwrap();
        let n = group.clips.len();
        total += n;
        assert_eq!(group.blend_matrix.len(), n * n, "{name}");
        for clip in &group.clips {
            assert!(clip.duration_ticks > 0);
            assert!(clip.sample_rate > 0.0);
            assert!(clip.instance_count >= 0);
            if clip.instanced {
                assert!(clip.instance_count > 0);
            }
            assert_eq!(
                clip.frames.len() * clip.bone_count,
                clip.frames.iter().map(Vec::len).sum::<usize>()
            );
        }
        if name.contains("NaturalResource") {
            let clip = &group.clips[2];
            assert_eq!(clip.name, "Remove");
            assert_eq!(clip.duration_ticks, 5_066_667);
            assert_eq!(clip.bone_count, 2);
            assert!(!clip.instanced);
            assert!(!clip.looping);
        }
    }
    assert_eq!(total, 64);
    let meshes: Vec<_> = archive
        .names()
        .into_iter()
        .filter(|name| {
            name.contains("Models\\NaturalResource\\")
                && name.contains("TreeMesh")
                && name.ends_with("_resource.crs")
        })
        .map(str::to_owned)
        .collect();
    assert_eq!(meshes.len(), 17);
    for name in meshes {
        let mesh = graphics_mesh(&archive.read(&name).unwrap()).unwrap();
        assert_eq!(mesh.bones.len(), 2, "{name}");
        assert!(
            mesh.parts
                .iter()
                .flat_map(|p| &p.ranges)
                .filter(|r| r.index_count > 0)
                .all(|r| r.bone == 1),
            "{name}"
        );
    }
    eprintln!("15 animation groups, 64 clips and 17 rigid tree meshes verified");
}
