//! Port-side random-draw trace of three male citizen names from seed 7, for comparison with
//! the original under tools/emu/rng_trace.py (see tools/triage/README.md). Analysis helper.
use banished_formats::names::NameDatabase;
use banished_game::names::generate;
use banished_game::random::{trace, Random};

fn main() {
    let out = std::env::args().nth(1).expect("OUT.rng");
    let data = std::fs::read("game-data/extracted/Game/Names/Male_resource.crs").unwrap();
    let db = NameDatabase::read(&data).unwrap();
    let mut rng = Random::new(7);
    trace::start();
    for _ in 0..3 {
        generate(&db, &mut rng, 3, 10);
    }
    let draws = trace::take();
    trace::write(std::path::Path::new(&out), &draws).unwrap();
    println!("{} draws", draws.len());
}
