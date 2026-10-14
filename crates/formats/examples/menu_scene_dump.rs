//! Summarise a main-menu scene's placed meshes (analysis helper).
//! `cargo run -p banished-formats --example menu_scene_dump -- path/to/WinData [SCENE]`
use banished_formats::pkg::{Archive, Package};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let dir = std::path::Path::new(&args[1]);
    let index: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);
    let mut archive = Archive::default();
    for name in ["data1.pkg", "patch1.pkg", "patch0.pkg", "data0.pkg"] {
        archive.add(Package::parse(std::fs::read(dir.join(name)).unwrap()).unwrap());
    }
    let r = format!("Game/MenuScene/Scene{index}.rsc:resource");
    let s = banished_formats::menu_scene::menu_scene(&archive.read_ref(&r).unwrap()).unwrap();
    {
        let m = &s.map;
        let z: Vec<i16> = m
            .water_vertices
            .iter()
            .map(|v| i16::from_le_bytes([v[4], v[5]]))
            .collect();
        let w: Vec<i16> = m
            .water_vertices
            .iter()
            .map(|v| i16::from_le_bytes([v[6], v[7]]))
            .collect();
        let mut zs = z.clone();
        zs.sort();
        zs.dedup();
        let mut ws = w.clone();
        ws.sort();
        ws.dedup();
        println!(
            "WATER z values {:?} (n={}) w {:?}",
            &zs[..zs.len().min(12)],
            zs.len(),
            &ws[..ws.len().min(8)]
        );
        println!("WATER first verts {:?}", &m.water_vertices[..3]);
        println!("RIVERS {:?}\nLAKES {:?}", m.river_ranges, m.lake_ranges);
        println!(
            "DECALS {} extra {} materials {:?}",
            m.decals.len(),
            m.extra_decals.len(),
            m.decal_materials
        );
        for d in m.decals.iter().chain(m.extra_decals.iter()).take(4) {
            println!(
                "DECAL {:?} ints {:?} b {} i6 {} f7 {} floats {:?} f13 {} fv2 {} verts {} idx {}",
                d.material,
                d.ints,
                d.byte,
                d.int6,
                d.flag7,
                d.floats,
                d.flag13,
                d.flag_v2,
                d.vertices.len(),
                d.indices.len()
            );
            if let Some(v) = d.vertices.first() {
                println!("  v0 {:?}", v);
            }
        }
        println!("INTS {:?}", &m.ints[..m.ints.len().min(20)]);
    }
    let mut counts = vec![0usize; s.meshes.len()];
    for p in &s.placements {
        counts[p.mesh as usize] += 1;
    }
    for (i, m) in s.meshes.iter().enumerate() {
        let r = m.as_deref().unwrap_or("-");
        let info = archive
            .read_ref(&r.replace('\\', "/"))
            .and_then(|d| banished_formats::types::graphics_mesh(&d))
            .map(|g| {
                let ranges: Vec<i32> = g
                    .parts
                    .first()
                    .map_or(vec![], |p| p.ranges.iter().map(|r| r.bone).collect());
                format!(
                    "parts {} bones {} ranges(bone) {:?} sets {}",
                    g.parts.len(),
                    g.bones.len(),
                    ranges,
                    g.material_instances.len()
                )
            })
            .unwrap_or_else(|e| format!("ERR {e}"));
        println!("{i:3} x{:5} {r} | {info}", counts[i]);
    }
    for name in ["ModelLayout", "ModelBillboardLayout"] {
        let l = banished_formats::types::vertex_layout(
            &archive
                .read_ref(&format!("MeshLayout/{name}.rsc:resource"))
                .unwrap(),
        )
        .unwrap();
        println!("LAYOUT {name}: {:?}", l.elements);
    }
    for m in [
        "Models/NaturalResource/Tree/OakTreeMesh.rsc:resource",
        "Models/Rocks/Rock01Mesh.rsc:resource",
    ] {
        let g = banished_formats::types::graphics_mesh(&archive.read_ref(m).unwrap()).unwrap();
        let p = &g.parts[0];
        let verts: i32 = p
            .ranges
            .iter()
            .map(|r| r.start_vertex + r.vertex_count)
            .max()
            .unwrap_or(0);
        println!(
            "STRIDE {m}: {} bytes / {} verts = {}",
            p.static_vertices.len(),
            verts,
            p.static_vertices.len() as f32 / verts as f32
        );
    }
    // Material bases used by each mesh's ranges.
    let mut bases = std::collections::BTreeMap::<String, Vec<String>>::new();
    for m in s.meshes.iter().flatten() {
        let Ok(g) = archive
            .read_ref(&m.replace('\\', "/"))
            .and_then(|d| banished_formats::types::graphics_mesh(&d))
        else {
            continue;
        };
        for set in &g.material_instances {
            for mi in set.iter().flatten() {
                let base = archive
                    .read_ref(&mi.replace('\\', "/"))
                    .and_then(|d| banished_formats::types::material_instance(&d))
                    .ok()
                    .and_then(|i| i.material)
                    .unwrap_or_default();
                bases
                    .entry(base)
                    .or_default()
                    .push(m.rsplit('\\').next().unwrap().to_string());
            }
        }
    }
    for (base, users) in &bases {
        println!(
            "MATERIAL {base}: {} uses, e.g. {:?}",
            users.len(),
            &users[..users.len().min(4)]
        );
    }
    for p in s.placements.iter().take(6) {
        println!("{:?} mesh {} mat {}", p.floats, p.mesh, p.material);
    }
    for p in s.particles.iter().take(3) {
        println!("particle {:?}", p);
    }
    println!("particle systems {:?}", s.particle_systems);
    println!("effect {:?} {}", s.effect, s.effect_value);
}

#[allow(dead_code)]
fn unused() {}
