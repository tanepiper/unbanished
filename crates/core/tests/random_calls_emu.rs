//! The game's Random against the original Runtime (`tools/emu/random_calls.py`): mixed
//! sequences of every kind of call on 300 seeds, edge arguments among them (`Rand(0)`,
//! reversed and wide ranges, negative tolerances), each result exactly. Skips when the ground
//! truth has not been made.
use unbanished_core::random::Random;

#[test]
fn original_random_calls() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../game-data/derived/emu/random_calls/cases.tsv");
    let Ok(data) = std::fs::read_to_string(path) else {
        eprintln!("random ground truth missing; skipping");
        return;
    };
    let mut rng = Random::new(0);
    let mut seed = None;
    let mut count = 0;
    for line in data.lines().skip(1) {
        let p: Vec<&str> = line.split('\t').collect();
        let s: u32 = p[0].parse().unwrap();
        if seed != Some(s) || p[1] == "0" {
            rng = Random::new(s);
            seed = Some(s);
        }
        let a: i64 = p[3].parse().unwrap();
        let b: i64 = p[4].parse().unwrap();
        let got: i64 = match p[2] {
            "rand" => rng.rand() as i64,
            "below" => rng.rand_below(a as u32) as i64,
            "range" => rng.rand_range(a as i32, b as i32) as i64,
            "tol" => rng.rand_tol(a as i32) as i64,
            "float" => rng.rand_float().to_bits() as i64,
            _ => rng.rand_float_tol().to_bits() as i64,
        };
        let want: i64 = p[5].parse().unwrap();
        let want = if matches!(p[2], "rand" | "below" | "float" | "floattol") {
            want as u32 as i64
        } else {
            want
        };
        assert_eq!(got, want, "seed {s} call {}: {} {a} {b}", p[1], p[2]);
        count += 1;
    }
    assert!(count > 0);
    eprintln!("{count} Random calls match the original");
}
