//! List an animation group's clips with their root motion (analysis helper).
use banished_formats::pkg::{Archive, Package};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let dir = std::path::Path::new(&args[1]);
    let mut archive = Archive::default();
    for name in ["data1.pkg", "patch1.pkg", "patch0.pkg", "data0.pkg"] {
        if let Ok(d) = std::fs::read(dir.join(name)) {
            archive.add(Package::parse(d).unwrap());
        }
    }
    let g = banished_formats::types::animation_group(&archive.read_ref(&args[2]).unwrap()).unwrap();
    for (i, c) in g.clips.iter().enumerate() {
        let root = c.frames.first().map(|f| f[0].translation);
        println!(
            "{i:2} {:24} loop {} rate {} frames {} root {:?}",
            c.name,
            c.looping,
            c.sample_rate,
            c.frames.len(),
            root
        );
    }
}
