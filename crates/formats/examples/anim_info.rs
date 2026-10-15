//! List an AnimationGroup's clips (analysis helper).
//! `cargo run -p banished-formats --example anim_info -- path/to/WinData Models/X/XAnims.rsc:resource`
use banished_formats::pkg::{Archive, Package};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let dir = std::path::Path::new(&args[1]);
    let mut archive = Archive::default();
    for name in ["data1.pkg", "patch1.pkg", "patch0.pkg", "data0.pkg"] {
        archive.add(Package::parse(std::fs::read(dir.join(name)).unwrap()).unwrap());
    }
    for r in &args[2..] {
        let g = banished_formats::types::animation_group(&archive.read_ref(r).unwrap()).unwrap();
        for (i, c) in g.clips.iter().enumerate() {
            println!(
                "{i:3} {} frames {} looping {}",
                c.name,
                c.frames.len(),
                c.looping
            );
        }
    }
}
