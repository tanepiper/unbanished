//! Print skeleton world positions for a clip at several times (analysis helper).
use banished_formats::pkg::{Archive, Package};
use banished_formats::skeleton::{local_bind, world_transforms};
use banished_game::behaviour::animation::sample_pose;
use banished_game::scheduler::ticks;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let dir = std::path::Path::new(&args[1]);
    let mut archive = Archive::default();
    for name in ["data1.pkg", "patch1.pkg", "patch0.pkg", "data0.pkg"] {
        archive.add(Package::parse(std::fs::read(dir.join(name)).unwrap()).unwrap());
    }
    let mesh =
        banished_formats::types::graphics_mesh(&archive.read_ref(&args[2]).unwrap()).unwrap();
    let g = banished_formats::types::animation_group(&archive.read_ref(&args[3]).unwrap()).unwrap();
    let clip = g.clips.iter().find(|c| c.name == args[4]).unwrap();
    let bind: Vec<_> = mesh.bones.iter().map(local_bind).collect();
    let wb = world_transforms(&mesh.bones, &bind);
    for (i, b) in mesh.bones.iter().enumerate() {
        println!(
            "bone {i} {} parent {} bind {:?}",
            b.name,
            b.parent,
            wb[i].apply([0.0; 3])
        );
    }
    {
        let pose = sample_pose(clip, 0, 0);
        let pal = banished_formats::skeleton::palette(&mesh.bones, &pose);
        let part = &mesh.parts[0];
        let v = &part.skinned_vertices;
        let cpu = banished_formats::skeleton::skin_to_static(v, &pal);
        let mats: Vec<[f32; 16]> = pal.iter().map(|t| t.to_matrix()).collect();
        let mut worst = 0.0f32;
        for (k, vert) in v.chunks_exact(32).enumerate() {
            let p = [0, 2, 4].map(|o| i16::from_le_bytes([vert[o], vert[o + 1]]) as f32 / 512.0);
            let mut m = [0.0f32; 16];
            for j in 0..4 {
                let w = vert[24 + j] as f32 / 255.0;
                let b = mats[vert[28 + j] as usize];
                for e in 0..16 {
                    m[e] += b[e] * w;
                }
            }
            let g = [
                m[0] * p[0] + m[4] * p[1] + m[8] * p[2] + m[12],
                m[1] * p[0] + m[5] * p[1] + m[9] * p[2] + m[13],
                m[2] * p[0] + m[6] * p[1] + m[10] * p[2] + m[14],
            ];
            let c = [0, 2, 4]
                .map(|o| i16::from_le_bytes([cpu[k * 24 + o], cpu[k * 24 + o + 1]]) as f32 / 512.0);
            let d = ((g[0] - c[0]).powi(2) + (g[1] - c[1]).powi(2) + (g[2] - c[2]).powi(2)).sqrt();
            worst = worst.max(d);
        }
        println!(
            "gpu-vs-cpu worst {worst}, bones max idx {}",
            v.chunks_exact(32)
                .map(|x| *x[28..32].iter().max().unwrap())
                .max()
                .unwrap()
        );
    }
    for (i, b) in banished_formats::skeleton::bind_palette(&mesh.bones)
        .iter()
        .enumerate()
    {
        println!("bindpal {i}: q {:?} t {:?}", b.rotation, b.translation);
    }
    for t in [0.0f32, 0.5, 1.0, 1.7, 2.5, 3.5] {
        let pose = sample_pose(clip, ticks(t), ticks(1.0 / 30.0));
        let pal = banished_formats::skeleton::palette(&mesh.bones, &pose);
        let worst: Vec<String> = (1..mesh.bones.len())
            .map(|i| {
                let j = wb[i].apply([0.0; 3]);
                let mut m = 0.0f32;
                for d in [[0.05, 0.0, 0.0], [0.0, 0.05, 0.0], [0.0, 0.0, 0.05]] {
                    let q = [j[0] + d[0], j[1] + d[1], j[2] + d[2]];
                    let r = pal[i].apply(q);
                    let dist =
                        ((r[0] - q[0]).powi(2) + (r[1] - q[1]).powi(2) + (r[2] - q[2]).powi(2))
                            .sqrt();
                    m = m.max(dist);
                }
                format!("{i}:{m:.2}")
            })
            .collect();
        println!("palette disp t={t}: {}", worst.join(" "));
        let pose = sample_pose(clip, ticks(t), ticks(1.0 / 30.0));
        let w = world_transforms(&mesh.bones, &pose);
        let p: Vec<String> = w
            .iter()
            .map(|x| {
                let a = x.apply([0.0; 3]);
                format!("({:.2},{:.2},{:.2})", a[0], a[1], a[2])
            })
            .collect();
        println!("t={t}: {}", p.join(" "));
    }
}

#[allow(dead_code)]
fn unused() {}
