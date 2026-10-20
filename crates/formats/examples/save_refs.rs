//! Print a save's external reference table (analysis helper).
//! `cargo run -p unbanished-formats --example save_refs -- IN.sav`
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let save = unbanished_formats::save::Save::parse(&std::fs::read(&args[1]).unwrap()).unwrap();
    for (id, path) in save.references().unwrap() {
        println!("{id}\t{path}");
    }
}
