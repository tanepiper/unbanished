//! A citizen's happiness after its events against the original (`FUN_140117fb0` under the
//! emulator, `tools/emu/happiness.py`), with and without a home and a priest, bit for bit. Skips
//! when the ground truth has not been made.
use unbanished_game::people::life::{happiness_after, home_bonus, LifeRules};

#[test]
fn original_happiness() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../game-data/derived/emu/happiness/cases.tsv");
    let Ok(data) = std::fs::read_to_string(path) else {
        eprintln!("happiness ground truth missing; skipping");
        return;
    };
    let mut count = 0;
    for line in data.lines().skip(1) {
        let p: Vec<&str> = line.split('\t').collect();
        let hex = |i: usize| f32::from_bits(u32::from_str_radix(p[i], 16).unwrap());
        let int = |i: usize| p[i].parse::<i32>().unwrap();
        let layers: Vec<u8> = p[6].split(',').map(|v| v.parse().unwrap()).collect();
        let rules = LifeRules::from_values(&[
            (0xa4, hex(8)),
            (0xa8, int(9) as f32),
            (0xac, int(10) as f32),
            (0xb0, int(11) as f32),
        ]);
        let bonus = (int(5) == 1).then(|| home_bonus(&layers, int(7) > 1));
        let got = happiness_after(
            &rules,
            p[0].parse().unwrap(),
            hex(1),
            int(2),
            int(3),
            bonus,
            int(4) == 1,
        );
        assert_eq!(
            got.to_bits(),
            u32::from_str_radix(p[12], 16).unwrap(),
            "case {count}: {line} -> {got}"
        );
        count += 1;
    }
    assert!(count > 0);
    eprintln!("{count} happiness updates match the original");
}
