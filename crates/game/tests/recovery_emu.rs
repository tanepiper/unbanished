//! The end of a citizen's illness against the original (`FUN_140117880` under the emulator,
//! `tools/emu/recovery.py`): whether the citizen recovers, the immunity after, and that one number
//! was drawn. Skips when the ground truth has not been made.
use unbanished_game::people::life::{immunity_after, recovers, LifeRules};
use unbanished_game::random::Random;

#[test]
fn original_recovery() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../game-data/derived/emu/recovery/cases.tsv");
    let Ok(data) = std::fs::read_to_string(path) else {
        eprintln!("recovery ground truth missing; skipping");
        return;
    };
    let mut lines = data.lines();
    let header: Vec<&str> = lines.next().unwrap().split('\t').collect();
    let col = |name: &str| header.iter().position(|h| *h == name).unwrap();
    let (mut count, mut lived) = (0, 0);
    for row in lines {
        let p: Vec<&str> = row.split('\t').collect();
        let f = |name: &str| p[col(name)].parse::<f32>().unwrap();
        let i = |name: &str| p[col(name)].parse::<i64>().unwrap();
        let rules = LifeRules::from_values(&[(0x128, f("factor")), (0x9c, f("immunity_minutes"))]);
        let mut rng = Random::new(i("seed") as u32);
        let before = rng.clone();
        let lives = recovers(&rules, f("recovery"), i("doctor") == 1, &mut rng);
        let moved = rng.index() != before.index() || rng.state() != before.state();
        assert_eq!(lives, i("returned") == 1, "case {count}: recovers");
        assert_eq!(moved, i("drew") == 1, "case {count}: drew");
        if lives {
            assert_eq!(
                immunity_after(&rules),
                i("immunity_after"),
                "case {count}: immunity"
            );
            assert_eq!(i("spread_after"), 0, "case {count}: spread");
        }
        lived += lives as usize;
        count += 1;
    }
    assert!(count > 0);
    eprintln!("{count} recoveries match the original ({lived} recovered)");
}
