//! Print an entity template's component list (analysis helper).
//! `cargo run -p unbanished-formats --example template_info -- path/to/WinData Template/X.rsc ...`
use unbanished_formats::pkg::{Archive, Package};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let dir = std::path::Path::new(&args[1]);
    let mut archive = Archive::default();
    for name in ["data1.pkg", "patch1.pkg", "patch0.pkg", "data0.pkg"] {
        archive.add(Package::parse(std::fs::read(dir.join(name)).unwrap()).unwrap());
    }
    for r in &args[2..] {
        let t = unbanished_formats::game::component_description(
            &archive.read_ref(&format!("{r}:resource")).unwrap(),
        )
        .unwrap();
        println!("{r}: bucket {} {:?}", t.bucket, t.descriptions);
        for d in t.descriptions.iter().flatten() {
            let Ok(data) = archive.read_ref(&d.replace('\\', "/")) else {
                continue;
            };
            use unbanished_formats::game::*;
            if let Ok(v) = growth_description(&data) {
                println!("  {d}: {v:?}");
            }
            if let Ok(v) = spawn_description(&data) {
                println!("  {d}: {v:?}");
            }
            if let Ok(v) = map_description(&data) {
                println!("  {d}: {v:?}");
            }
            if let Ok(v) = natural_resource_description(&data) {
                println!("  {d}: {v:?}");
            }
            if let Ok(v) = model_description(&data) {
                println!(
                    "  {d}: variants {} meshes {:?} variant {} mesh {} random {}",
                    v.variants.len(),
                    v.variants
                        .iter()
                        .map(|x| x.meshes.len())
                        .collect::<Vec<_>>(),
                    v.variant,
                    v.mesh,
                    v.random_variant
                );
            }
        }
    }
}
