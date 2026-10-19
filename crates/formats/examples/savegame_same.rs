//! Whether two saves hold exactly the same game state (the header's name, date and preview
//! aside): exit 0 when the state bytes are equal, 1 otherwise, with the first difference.
//! `cargo run -p banished-formats --example savegame_same -- A.sav B.sav`
use banished_formats::save::Save;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let state = |p: &str| Save::parse(&std::fs::read(p).unwrap()).unwrap().state;
    let (a, b) = (state(&args[0]), state(&args[1]));
    if a == b {
        println!("same game state ({} bytes)", a.len());
        return;
    }
    let first = a
        .iter()
        .zip(&b)
        .position(|(x, y)| x != y)
        .unwrap_or(a.len().min(b.len()));
    println!(
        "different game state: {} vs {} bytes, first difference at byte {first}",
        a.len(),
        b.len()
    );
    std::process::exit(1);
}
