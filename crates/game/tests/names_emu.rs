//! Parity of the name generator with the original `FUN_1400aa000` under emulation
//! (`tools/emu/names.py`). Skipped without game-data/.

use banished_formats::names::NameDatabase;
use banished_game::names::generate;
use banished_game::random::Random;
use std::path::PathBuf;

#[test]
fn names_match_the_original() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../game-data");
    let Ok(truth) = std::fs::read_to_string(root.join("derived/emu/names/names.tsv")) else {
        eprintln!("no emulator ground truth; run tools/emu/names.py");
        return;
    };
    let mut dbs = std::collections::HashMap::new();
    for name in ["Male", "Female", "Cities"] {
        let data = std::fs::read(root.join(format!("extracted/Game/Names/{name}_resource.crs")))
            .expect("extracted names");
        dbs.insert(name, NameDatabase::read(&data).unwrap());
    }
    let (mut total, mut wrong) = (0, 0);
    let mut first = None;
    let mut stream: Option<(String, u32, Random)> = None;
    for line in truth.lines() {
        let f: Vec<&str> = line.split('\t').collect();
        let (db, seed, expect) = (f[0], f[1].parse::<u32>().unwrap(), f[3]);
        if stream.as_ref().map(|s| (s.0.as_str(), s.1)) != Some((db, seed)) {
            stream = Some((db.to_string(), seed, Random::new(seed)));
        }
        let rng = &mut stream.as_mut().unwrap().2;
        let (lo, hi) = if db == "Cities" { (5, 20) } else { (3, 10) };
        let got = generate(&dbs[db], rng, lo, hi);
        total += 1;
        if got != expect {
            wrong += 1;
            first.get_or_insert(format!("{line}: got {got}"));
        }
    }
    assert_eq!(wrong, 0, "{wrong}/{total} differ; first: {first:?}");
}
