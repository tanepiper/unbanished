//! A list shuffled with the game's Random against the original (`FUN_14008eb70` under the
//! emulator, `tools/emu/shuffle.py`): the order after and the next number, which shows that the
//! same numbers were drawn. Skips when the ground truth has not been made.
use unbanished_game::random::Random;
use unbanished_game::work::professions::shuffle;

#[test]
fn original_shuffle() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../game-data/derived/emu/shuffle/cases.tsv");
    let Ok(data) = std::fs::read_to_string(path) else {
        eprintln!("shuffle ground truth missing; skipping");
        return;
    };
    let mut count = 0;
    for line in data.lines().skip(1) {
        let p: Vec<&str> = line.split('\t').collect();
        let n: usize = p[1].parse().unwrap();
        let mut list: Vec<usize> = (0..n).collect();
        let mut rng = Random::new(p[0].parse().unwrap());
        shuffle(&mut list, &mut rng);
        let expected: Vec<usize> = if p[2].is_empty() {
            Vec::new()
        } else {
            p[2].split(',').map(|v| v.parse().unwrap()).collect()
        };
        assert_eq!(list, expected, "case {count}: order of {n}");
        assert_eq!(
            rng.rand(),
            p[3].parse::<u32>().unwrap(),
            "case {count}: draws"
        );
        count += 1;
    }
    assert!(count > 0);
    eprintln!("{count} shuffles match the original");
}
