//! The renderer's seasonal values against the original (`FUN_1400988f0` under the emulator,
//! `tools/emu/season.py`): snow, autumn, leaf loss and rain, NaN temperatures among the cases, bit
//! for bit. Skips when the ground truth has not been made.
use unbanished_game::nature::environment::season_values;
use unbanished_game::scheduler::seconds;

#[test]
fn original_season() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../game-data/derived/emu/season/cases.tsv");
    let Ok(data) = std::fs::read_to_string(path) else {
        eprintln!("season ground truth missing; skipping");
        return;
    };
    let (mut count, mut differ) = (0, Vec::new());
    for line in data.lines().skip(1) {
        let p: Vec<&str> = line.split('\t').collect();
        let hex = |s: &str| f32::from_bits(u32::from_str_radix(s, 16).unwrap());
        let intensities: Vec<f32> = p[2]
            .split(',')
            .filter(|s| !s.is_empty() && *s != "-")
            .map(hex)
            .collect();
        let v = season_values(hex(p[0]), seconds(p[1].parse().unwrap()), &intensities);
        let got = [v.snow, v.autumn, v.leaf_loss, v.rain].map(f32::to_bits);
        let want = [p[3], p[4], p[5], p[6]].map(|s| u32::from_str_radix(s, 16).unwrap());
        if got != want {
            differ.push(format!("{line} -> {:?}", got.map(f32::from_bits)));
        }
        count += 1;
    }
    assert!(count > 0);
    assert!(
        differ.is_empty(),
        "{} of {count} differ:\n{}",
        differ.len(),
        differ[..differ.len().min(6)].join("\n")
    );
    eprintln!("{count} seasonal values match the original");
}
