//! Print the job-related descriptions of templates and the profession list (analysis helper).
//! `cargo run -p unbanished-formats --example job_info -- path/to/WinData Template/WoodHouse.rsc`
use unbanished_formats::game;
use unbanished_formats::pkg::{Archive, Package};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let dir = std::path::Path::new(&args[1]);
    let mut archive = Archive::default();
    for name in ["data1.pkg", "patch1.pkg", "patch0.pkg", "data0.pkg"] {
        archive.add(Package::parse(std::fs::read(dir.join(name)).unwrap()).unwrap());
    }
    let read = |r: &str| archive.read_ref(&r.replace('\\', "/")).ok();
    if let Some(list) = read("Game/Profession/Profession.rsc:professionList")
        .and_then(|d| game::external_list(&d).ok())
    {
        for (i, r) in list.iter().enumerate() {
            let p = read(r).and_then(|d| game::profession(&d).ok());
            println!(
                "profession {i} {r}: {:?}",
                p.map(|p| (
                    p.name, p.value_48, p.flag_4c, p.flag_4d, p.value_54, p.value_58, p.value_50,
                    p.flag_4e
                ))
            );
        }
    }
    for t in &args[2..] {
        let file = t.split(':').next().unwrap();
        println!("{t}:");
        if let Some(d) = read(&format!("{file}:work")) {
            println!("  work {:?}", game::work_description(&d));
        }
        if let Some(d) = read(&format!("{file}:workplace")) {
            println!("  workplace {:?}", game::workplace_description(&d));
        }
        if let Some(d) = read(&format!("{file}:build")) {
            println!("  build {:?}", game::build_description(&d));
        }
        if let Some(d) = read(&format!("{file}:cleararea")) {
            println!("  cleararea {:?}", game::clear_area_description(&d));
        }
        if let Some(d) = read(&format!("{file}:storage")) {
            println!("  storage {:?}", game::storage_description(&d));
        }
        if let Some(d) = read(&format!("{file}:interact")) {
            println!("  interact {:?}", game::interact_description(&d));
        }
    }
}
