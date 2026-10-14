//! Print a Dialog resource (analysis helper).
//! `cargo run -p banished-formats --example dialog_info -- path/to/WinData REFERENCE...`
use banished_formats::pkg::{Archive, Package};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let dir = std::path::Path::new(&args[1]);
    let mut archive = Archive::default();
    for name in ["data1.pkg", "patch1.pkg", "patch0.pkg", "data0.pkg"] {
        archive.add(Package::parse(std::fs::read(dir.join(name)).unwrap()).unwrap());
    }
    for r in &args[2..] {
        let d = banished_formats::ui::dialog(&archive.read_ref(r).unwrap()).unwrap();
        println!("{r}: {d:#?}");
    }
}
