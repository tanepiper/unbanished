//! Bounds of a skinned mesh in the bind pose and a clip pose (analysis helper).
use banished_formats::pkg::{Archive, Package};
use banished_game::behaviour::animation::sample_pose;

fn bounds(v: &[u8]) -> ([f32; 3], [f32; 3]) {
    let mut lo = [f32::MAX; 3];
    let mut hi = [f32::MIN; 3];
    for c in v.chunks_exact(24) {
        for a in 0..3 {
            let x = i16::from_le_bytes([c[a * 2], c[a * 2 + 1]]) as f32 / 512.0;
            lo[a] = lo[a].min(x);
            hi[a] = hi[a].max(x);
        }
    }
    (lo, hi)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let dir = std::path::Path::new(&args[1]);
    let mut archive = Archive::default();
    for name in ["data1.pkg", "patch1.pkg", "patch0.pkg", "data0.pkg"] {
        if let Ok(d) = std::fs::read(dir.join(name)) {
            archive.add(Package::parse(d).unwrap());
        }
    }
    let mesh =
        banished_formats::types::graphics_mesh(&archive.read_ref(&args[2]).unwrap()).unwrap();
    let g = banished_formats::types::animation_group(&archive.read_ref(&args[3]).unwrap()).unwrap();
    let clip = g.clips.iter().find(|c| c.name == args[4]).unwrap();
    let v = &mesh.parts[0].skinned_vertices;
    let bind = banished_formats::skeleton::bind_palette(&mesh.bones);
    println!(
        "bind {:?}",
        bounds(&banished_formats::skeleton::skin_to_static(v, &bind))
    );
    for t in [0i64, 1000, 100000] {
        let pal = banished_formats::skeleton::palette(&mesh.bones, &sample_pose(clip, t, 0));
        println!(
            "t={t} {:?}",
            bounds(&banished_formats::skeleton::skin_to_static(v, &pal))
        );
    }
    println!(
        "clip {} mesh_name {} bones {}",
        clip.name, clip.mesh_name, clip.bone_count
    );
    for (i, b) in mesh.bones.iter().enumerate() {
        print!("{i}:{} ", b.name);
    }
    println!();
    let mut sums = std::collections::BTreeMap::new();
    for c in v.chunks_exact(32) {
        let s: u32 = c[24..28].iter().map(|&b| b as u32).sum();
        *sums.entry(s).or_insert(0) += 1;
    }
    println!("weight sums {sums:?}");
    for c in v.chunks_exact(32).take(6) {
        println!("w {:?} i {:?} pad {:?}", &c[24..28], &c[28..32], &c[20..24]);
    }
    let part = &mesh.parts[0];
    for r in &part.ranges {
        let (ix, vx, stride) = part.geometry(r);
        let idx: Vec<u16> = ix
            [r.start_index as usize * 2..(r.start_index + r.index_count) as usize * 2]
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        println!(
            "range bone {} sv {} vc {} si {} ic {} idx min {} max {} buf verts {}",
            r.bone,
            r.start_vertex,
            r.vertex_count,
            r.start_index,
            r.index_count,
            idx.iter().min().unwrap_or(&0),
            idx.iter().max().unwrap_or(&0),
            vx.len() / stride
        );
    }
}
