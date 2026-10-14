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
                    for range in &part.ranges {
                        let (indices, vertices, stride) = part.geometry(range);
                        assert_eq!(vertices.len() % stride, 0, "{name}");
                        assert_eq!(indices.len() % 2, 0, "{name}");
                        let verts = vertices.len() / stride;
                        let start = usize::try_from(range.start_index).unwrap() * 2;
                        let end = start + usize::try_from(range.index_count).unwrap() * 2;
                        let base = usize::try_from(range.start_vertex).unwrap();
                        assert!(end <= indices.len(), "{name}: index range out of bounds");
                        assert!(base + range.vertex_count as usize <= verts, "{name}");
                        for ix in indices[start..end].chunks_exact(2) {
                            let index = u16::from_le_bytes([ix[0], ix[1]]) as usize;
                            assert!(index < range.vertex_count as usize, "{name}: local index");
                            assert!(base + index < verts, "{name}: index out of range");
                        }
                    }
                }
                "GraphicsMesh"
            }
            t if t == type_id("MaterialInstance") => {
                types::material_instance(&data).unwrap_or_else(|e| panic!("{name}: {e}"));
                "MaterialInstance"
            }
            t if t == type_id("SoundBuffer") => {
                let snd = types::sound_buffer(&data).unwrap_or_else(|e| panic!("{name}: {e}"));
                assert_eq!(snd.format.tag, 0x161, "{name}: expected xWMA");
                assert_eq!(snd.packet_table.len() % 4, 0, "{name}");
                if snd.streamed {
                    assert!(snd.data.is_empty(), "{name}");
                    assert!(
                        archive.get(&snd.stream_entry).is_some(),
                        "{name}: missing {}",
                        snd.stream_entry
                    );
                } else {
                    assert!(!snd.data.is_empty(), "{name}");
                }
                "SoundBuffer"
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
    assert_eq!(counts["SoundBuffer"], 72);
}

/// Combined rigid and weighted geometry must reproduce every skinned mesh's stored bounds.
#[test]
fn skinned_bind_pose_matches_bounds() {
    use banished_formats::skeleton::{bind_palette, local_bind, world_transforms};
    use banished_formats::types::{POSITION_SCALE, SKINNED_STRIDE};
    let Some(archive) = archive() else { return };
    let mut checked = 0;
    for name in archive.names() {
        if !name.starts_with("Models\\") || !name.ends_with(".crs") {
            continue;
        }
        let data = archive.read(name).unwrap();
        if crs::header(&data).unwrap().type_id != type_id("GraphicsMesh") {
            continue;
        }
        let mesh = types::graphics_mesh(&data).unwrap();
        if !mesh.parts.iter().any(|p| !p.skinned_vertices.is_empty()) {
            continue;
        }
        let pal = bind_palette(&mesh.bones);
        let locals: Vec<_> = mesh.bones.iter().map(local_bind).collect();
        let world = world_transforms(&mesh.bones, &locals);
        let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
        // The viewer uses the first detail level. Its triangle lists address both streams.
        for part in mesh.parts.iter().take(1) {
            for range in &part.ranges {
                let (indices, vertices, stride) = part.geometry(range);
                let start = range.start_index as usize * 2;
                let end = start + range.index_count as usize * 2;
                for ix in indices[start..end].chunks_exact(2) {
                    let i =
                        range.start_vertex as usize + u16::from_le_bytes([ix[0], ix[1]]) as usize;
                    let v = &vertices[i * stride..(i + 1) * stride];
                    let pos = [0, 2, 4]
                        .map(|o| i16::from_le_bytes([v[o], v[o + 1]]) as f32 * POSITION_SCALE);
                    let acc = if stride == SKINNED_STRIDE {
                        let mut acc = [0.0f32; 3];
                        for k in 0..4 {
                            let w = v[24 + k] as f32 / 255.0;
                            if w > 0.0 {
                                let p = pal[v[28 + k] as usize].apply(pos);
                                (0..3).for_each(|i| acc[i] += p[i] * w);
                            }
                        }
                        acc
                    } else {
                        world[range.bone as usize].apply(pos)
                    };
                    (0..3).for_each(|i| {
                        lo[i] = lo[i].min(acc[i]);
                        hi[i] = hi[i].max(acc[i]);
                    });
                }
            }
        }
        for i in 0..3 {
            assert!(
                (lo[i] - mesh.bounds_min[i]).abs() < 0.01,
                "{name}: min {lo:?} vs {:?}",
                mesh.bounds_min
            );
            assert!(
                (hi[i] - mesh.bounds_max[i]).abs() < 0.01,
                "{name}: max {hi:?} vs {:?}",
                mesh.bounds_max
            );
        }
        checked += 1;
    }
    assert_eq!(checked, 8);
}

/// Every UI resource decodes with exact block consumption.
#[test]
fn decodes_all_ui_resources() {
    use banished_formats::ui;
    let Some(archive) = archive() else { return };
    let mut counts = std::collections::BTreeMap::<&str, usize>::new();
    for name in archive.names() {
        if !name.ends_with(".crs") {
            continue;
        }
        let data = archive.read(name).unwrap();
        let tid = crs::header(&data).unwrap().type_id;
        let class = if ui::is_element(tid) {
            ui::element(&data)
                .unwrap_or_else(|e| panic!("{name}: {e}"))
                .class
        } else if tid == type_id("Dialog") {
            ui::dialog(&data).unwrap_or_else(|e| panic!("{name}: {e}"));
            "Dialog"
        } else if tid == type_id("SpriteSheet") {
            ui::sprite_sheet(&data).unwrap_or_else(|e| panic!("{name}: {e}"));
            "SpriteSheet"
        } else if tid == type_id("Font") {
            let f = ui::font(&data).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert!(f.glyph_index('A').is_some(), "{name}");
            "Font"
        } else if tid == type_id("StringTable") {
            ui::string_table(&data).unwrap_or_else(|e| panic!("{name}: {e}"));
            "StringTable"
        } else {
            continue;
        };
        *counts.entry(class).or_default() += 1;
    }
    eprintln!("{counts:?}");
    assert_eq!(counts["Dialog"], 55);
    assert_eq!(counts["Font"], 4);
    assert_eq!(counts["SpriteSheet"], 31);
    assert_eq!(counts["StringTable"], 115);
    assert_eq!(counts["ButtonDescription"], 138);
    assert_eq!(counts["LabelDescription"], 145);
    assert_eq!(counts.values().sum::<usize>(), 1644);
}
