//! A citizen's health after an interval's events against the original (`FUN_140117e60` under the
//! emulator, `tools/emu/health.py`), NaN ages and healths among the cases, bit for bit. Skips when
//! the ground truth has not been made.
use unbanished_game::people::life::health_after;

#[test]
fn original_health() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../game-data/derived/emu/health/cases.tsv");
    let Ok(data) = std::fs::read_to_string(path) else {
        eprintln!("health ground truth missing; skipping");
        return;
    };
    let mut count = 0;
    for line in data.lines().skip(1) {
        let p: Vec<u32> = line
            .split('\t')
            .map(|v| u32::from_str_radix(v, 16).unwrap())
            .collect();
        let f = f32::from_bits;
        let got = health_after(p[0], f(p[1]), f(p[2]), f(p[4]), p[3] == 1, f(p[5]));
        assert_eq!(
            got.to_bits(),
            p[6],
            "case {count}: events {:#x} health {} age {} school {} -> {} (original {})",
            p[0],
            f(p[1]),
            f(p[2]),
            p[3],
            got,
            f(p[6])
        );
        assert_eq!(p[7], 0, "case {count}: the events are cleared");
        count += 1;
    }
    assert!(count > 0);
    eprintln!("{count} health updates match the original");
}
