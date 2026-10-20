//! `scheduler::ticks` and `scheduler::seconds` against the Runtime's `System::Time`
//! (`tools/emu/time.py`; skipped without its output in game-data/derived/emu).

use unbanished_game::scheduler::{seconds, ticks};

#[test]
fn matches_the_runtime() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../game-data/derived/emu/time.tsv"
    );
    let Ok(text) = std::fs::read_to_string(path) else {
        eprintln!("no emulator output; skipping");
        return;
    };
    let mut n = 0;
    for line in text.lines() {
        let (a, b) = line.split_once('\t').unwrap();
        let (s, t) = a.split_once(' ').unwrap();
        let s = f32::from_bits(u32::from_str_radix(s, 16).unwrap());
        assert_eq!(ticks(s), t.parse::<i64>().unwrap(), "Time({s:e})");
        let (k, f) = b.split_once(' ').unwrap();
        let k: i64 = k.parse().unwrap();
        assert_eq!(
            seconds(k).to_bits(),
            u32::from_str_radix(f, 16).unwrap(),
            "AsSeconds({k})"
        );
        n += 1;
    }
    assert!(n > 0);
}
