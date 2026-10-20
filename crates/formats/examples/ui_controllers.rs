//! Print a template's UIDescription: host, page and controllers (analysis helper).
use unbanished_formats::pkg::{Archive, Package};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let dir = std::path::Path::new(&args[1]);
    let mut archive = Archive::default();
    for name in ["data1.pkg", "patch1.pkg", "patch0.pkg", "data0.pkg"] {
        if let Ok(d) = std::fs::read(dir.join(name)) {
            archive.add(Package::parse(d).unwrap());
        }
    }
    for t in &args[2..] {
        let d = unbanished_formats::game::ui_description(
            &archive.read_ref(&format!("Template/{t}.rsc:ui")).unwrap(),
        )
        .unwrap();
        println!("{t}: host {:?} page {}", d.dialog, d.flags);
        for c in d.controllers {
            println!(
                "  {:#010x} slot {:12} element {:?} config {:?} tab {:?}",
                c.type_id, c.slot, c.element, c.config, c.tab
            );
        }
    }
}
