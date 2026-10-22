//! The disease check of a citizen with no disease against the original (`FUN_140117ca0` under
//! the emulator, `tools/emu/disease_check.py`): the immunity and clock after it, whether it drew,
//! and whether the citizen fell ill. Skips when the ground truth has not been made.
use unbanished_game::people::life::{outbreak_check, LifeRules};
use unbanished_game::random::Random;

#[test]
fn original_disease_check() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../game-data/derived/emu/disease/cases.tsv");
    let Ok(data) = std::fs::read_to_string(path) else {
        eprintln!("disease ground truth missing; skipping");
        return;
    };
    let mut lines = data.lines();
    let header: Vec<&str> = lines.next().unwrap().split('\t').collect();
    let col = |name: &str| header.iter().position(|h| *h == name).unwrap();
    let (mut count, mut drew, mut ill) = (0, 0, 0);
    for row in lines {
        let p: Vec<&str> = row.split('\t').collect();
        let f = |name: &str| p[col(name)].parse::<f32>().unwrap();
        let i = |name: &str| p[col(name)].parse::<i64>().unwrap();
        let rules = LifeRules::from_values(
            &[0x70, 0x7c, 0x80, 0x84, 0x88, 0x8c, 0x90].map(|o| (o, f(&format!("d{o:x}")))),
        );
        let mut rng = Random::new(i("seed") as u32);
        let before = rng.clone();
        let (mut immunity, mut clock) = (i("immunity"), i("clock"));
        let falls_ill = outbreak_check(
            &rules,
            f("health"),
            &mut immunity,
            &mut clock,
            f("town"),
            i("dt"),
            &mut rng,
        );
        let moved = rng.index() != before.index() || rng.state() != before.state();
        assert_eq!(immunity, i("immunity_after"), "case {count}: immunity");
        assert_eq!(clock, i("clock_after"), "case {count}: clock");
        assert_eq!(moved, i("drew") == 1, "case {count}: drew");
        assert_eq!(falls_ill, i("outbreak") == 1, "case {count}: outbreak");
        drew += moved as usize;
        ill += falls_ill as usize;
        count += 1;
    }
    assert!(count > 0);
    eprintln!("{count} disease checks match the original ({drew} drew, {ill} fell ill)");
}
