//! Print a start-condition profile (analysis helper).
//! `cargo run -p unbanished-formats --example profile_info -- path/to/WinData Game/StartCondition/Medium.rsc:resource`
use unbanished_formats::pkg::{Archive, Package};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let dir = std::path::Path::new(&args[1]);
    let mut archive = Archive::default();
    for name in ["data1.pkg", "patch1.pkg", "patch0.pkg", "data0.pkg"] {
        archive.add(Package::parse(std::fs::read(dir.join(name)).unwrap()).unwrap());
    }
    for r in &args[2..] {
        let p = unbanished_formats::game::map_generator(&archive.read_ref(r).unwrap()).unwrap();
        println!("{r}: {p:#?}");
    }
}
