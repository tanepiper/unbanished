//! `chance::once_in_years` against the original (`FUN_1400ae760` under the emulator,
//! `tools/emu/once_in_years.py`): the clock after it, whether it drew, and the result. Skips when
//! the ground truth has not been made.
use unbanished_game::chance::once_in_years;
use unbanished_game::random::Random;

#[test]
fn original_once_in_years() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../game-data/derived/emu/once_in_years/cases.tsv");
    let Ok(data) = std::fs::read_to_string(path) else {
        eprintln!("once-in-years ground truth missing; skipping");
        return;
    };
    let mut count = 0;
    for row in data.lines().skip(1) {
        let p: Vec<&str> = row.split('\t').collect();
        let mut rng = Random::new(p[0].parse().unwrap());
        let before = rng.clone();
        let mut timer: i64 = p[1].parse().unwrap();
        let happens = once_in_years(
            &mut timer,
            p[2].parse().unwrap(),
            p[3].parse().unwrap(),
            &mut rng,
        );
        let drew = rng.index() != before.index() || rng.state() != before.state();
        assert_eq!(timer, p[4].parse::<i64>().unwrap(), "case {count}: clock");
        assert_eq!(drew, p[5] == "1", "case {count}: drew");
        assert_eq!(happens, p[6] == "1", "case {count}: result");
        count += 1;
    }
    assert!(count > 0);
    eprintln!("{count} cases match the original");
}
