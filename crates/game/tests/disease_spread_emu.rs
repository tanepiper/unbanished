//! A sick citizen passing the disease on, against the original (`FUN_140117940` under the
//! emulator, `tools/emu/disease_spread.py`): one `RandFloat` against the spread chance, then at
//! most one citizen offered it, the first in the tree's list that is neither the sick citizen
//! nor sick. Skips when the ground truth has not been made.
use unbanished_game::people::life::spread_target;
use unbanished_game::random::Random;

#[test]
fn original_disease_spread() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../game-data/derived/emu/disease_spread/cases.tsv");
    let Ok(data) = std::fs::read_to_string(path) else {
        eprintln!("disease spread ground truth missing; skipping");
        return;
    };
    let (mut count, mut offers) = (0, 0);
    for line in data.lines().skip(1) {
        let p: Vec<&str> = line.split('\t').collect();
        let chance: f32 = p[1].parse().unwrap();
        let listed: Vec<(bool, bool)> = p[2]
            .chars()
            .filter(|&c| c != '-')
            .map(|c| (c == 's', c == '1'))
            .collect();
        let mut rng = Random::new(p[0].parse().unwrap());
        let before = rng.clone();
        // The draw comes first; the list is only looked at when it passes.
        let passes = rng.rand_float() <= chance;
        let offered = if passes { spread_target(&listed) } else { None };
        let moved = rng.index() != before.index() || rng.state() != before.state();
        let expected = match p[4] {
            "-" => None,
            k => Some(k.parse::<usize>().unwrap()),
        };
        assert_eq!(moved, p[3] == "1", "case {count}: drew");
        assert_eq!(offered, expected, "case {count}: {line}");
        offers += offered.is_some() as usize;
        count += 1;
    }
    assert!(count > 0);
    eprintln!("{count} spreads match the original ({offers} offered the disease)");
}
