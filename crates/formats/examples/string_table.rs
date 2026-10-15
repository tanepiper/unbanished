//! Print string table entries (analysis helper).
//! `cargo run -p banished-formats --example string_table -- path/to/WinData Dialog/StringTable.rsc:toolBar [KEY...]`
use banished_formats::pkg::{Archive, Package};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let dir = std::path::Path::new(&args[1]);
    let mut archive = Archive::default();
    for name in ["data1.pkg", "patch1.pkg", "patch0.pkg", "data0.pkg"] {
        archive.add(Package::parse(std::fs::read(dir.join(name)).unwrap()).unwrap());
    }
    let table = banished_formats::ui::string_table(&archive.read_ref(&args[2]).unwrap()).unwrap();
    for (k, v) in table {
        if args.len() <= 3 || args[3..].contains(&k) {
            println!("{k} = {v:?}");
        }
    }
}
