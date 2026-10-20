//! Print a GraphicsMesh's parts, ranges and bounds (analysis helper).
//! `cargo run -p unbanished-formats --example mesh_info -- path/to/WinData REFERENCE...`
use unbanished_formats::pkg::{Archive, Package};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let dir = std::path::Path::new(&args[1]);
    let mut archive = Archive::default();
    for name in ["data1.pkg", "patch1.pkg", "patch0.pkg", "data0.pkg"] {
        archive.add(Package::parse(std::fs::read(dir.join(name)).unwrap()).unwrap());
    }
    for r in &args[2..] {
        let g = unbanished_formats::types::graphics_mesh(&archive.read_ref(r).unwrap()).unwrap();
        println!(
            "{r}: bounds {:?}..{:?} bones {} materials {:?}",
            g.bounds_min,
            g.bounds_max,
            g.bones.len(),
            g.material_instances
        );
        for p in &g.parts {
            println!(
                "  part '{}' lod {} ao {:?}: static {}B/{}B skinned {}B/{}B",
                p.name,
                p.lod_distance,
                p.ao_texture,
                p.static_vertices.len(),
                p.static_indices.len(),
                p.skinned_vertices.len(),
                p.skinned_indices.len()
            );
            for r in &p.ranges {
                println!("    {r:?}");
            }
        }
    }
}
