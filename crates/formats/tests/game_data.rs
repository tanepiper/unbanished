//! Decodes every supported resource in a real install. Skipped (passes trivially) when the
//! git-ignored `game-data/original/WinData` folder is absent; run `tools/extract-assets.sh` first.

use banished_formats::crc::type_id;
use banished_formats::pkg::{Archive, Package};
use banished_formats::{crs, types};
use std::path::PathBuf;

fn archive() -> Option<Archive> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../game-data/original/WinData");
    if !dir.join("data0.pkg").exists() {
        eprintln!("game data not found at {}; skipping", dir.display());
        return None;
    }
    let mut archive = Archive::default();
    for name in ["data1.pkg", "patch1.pkg", "patch0.pkg", "data0.pkg"] {
        let bytes = std::fs::read(dir.join(name)).unwrap();
        archive.add(Package::parse(bytes).unwrap_or_else(|e| panic!("{name}: {e}")));
    }
    Some(archive)
}

#[test]
fn decodes_all_supported_resources() {
    let Some(archive) = archive() else { return };
    let mut counts = std::collections::BTreeMap::<&str, usize>::new();
    let mut containers = 0;
    for name in archive.names() {
        if !name.ends_with(".crs") {
            continue;
        }
        let data = archive.read(name).unwrap_or_else(|e| panic!("{name}: {e}"));
        let header = crs::header(&data).unwrap_or_else(|e| panic!("{name}: {e}"));
        containers += 1;
        let class = match header.type_id {
            t if t == type_id("ImageBuffer") => {
                let img = types::image_buffer(&data).unwrap_or_else(|e| panic!("{name}: {e}"));
                for (i, level) in img.levels.iter().enumerate() {
                    let mip = (i % img.mip_count()) as u32;
                    assert_eq!(
                        level.len(),
                        img.format.level_size(img.width >> mip, img.height >> mip),
                        "{name}"
                    );
                }
                "ImageBuffer"
            }
            t if t == type_id("GraphicsMesh") => {
                let mesh = types::graphics_mesh(&data).unwrap_or_else(|e| panic!("{name}: {e}"));
                for part in &mesh.parts {
                    let stride = if part.skinned {
                        types::SKINNED_STRIDE
                    } else {
                        types::STATIC_STRIDE
                    };
                    assert_eq!(part.vertices.len() % stride, 0, "{name}");
                    let verts = part.vertices.len() / stride;
                    for ix in part.indices.chunks_exact(2) {
                        assert!(
                            (u16::from_le_bytes([ix[0], ix[1]]) as usize) < verts,
                            "{name}: index out of range"
                        );
                    }
                }
                "GraphicsMesh"
            }
            t if t == type_id("MaterialInstance") => {
                types::material_instance(&data).unwrap_or_else(|e| panic!("{name}: {e}"));
                "MaterialInstance"
            }
            t if t == type_id("VertexLayout") => {
                types::vertex_layout(&data).unwrap_or_else(|e| panic!("{name}: {e}"));
                "VertexLayout"
            }
            _ => continue,
        };
        *counts.entry(class).or_default() += 1;
    }
    eprintln!("{containers} resources, decoded {counts:?}");
    assert_eq!(containers, 5812);
    assert_eq!(counts["ImageBuffer"], 406);
    assert_eq!(counts["GraphicsMesh"], 271);
    assert_eq!(counts["MaterialInstance"], 209);
    assert_eq!(counts["VertexLayout"], 19);
}
