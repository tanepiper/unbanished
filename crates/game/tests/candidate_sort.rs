//! `ai::crt_qsort` against the original candidate sort (`tools/emu/candidate_sort.py`; skipped
//! without its output in game-data/derived/emu).

use banished_game::ai::{crt_qsort, Candidate};

fn parse(s: &str) -> Vec<Candidate> {
    s.split(',')
        .map(|e| {
            let (a, p) = e.split_once(':').unwrap();
            (a.parse().unwrap(), p.parse().unwrap())
        })
        .collect()
}

#[test]
fn matches_the_original() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../game-data/derived/emu/candidate_sort.tsv"
    );
    let Ok(text) = std::fs::read_to_string(path) else {
        eprintln!("no emulator output; skipping");
        return;
    };
    let mut n = 0;
    for line in text.lines() {
        let (input, expected) = line.split_once('\t').unwrap();
        let mut v = parse(input);
        crt_qsort(&mut v);
        assert_eq!(v, parse(expected), "{input}");
        n += 1;
    }
    assert_eq!(n, 400);
}
