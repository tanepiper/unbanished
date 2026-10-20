//! `tornado::displace` against the original's `FUN_1400592e0` (`tools/emu/tornado_path.py`;
//! skipped without its output in game-data/derived/emu): the same 129 points, bit for bit.

use unbanished_game::disasters::tornado::displace;
use unbanished_game::random::Random;

#[test]
fn matches_the_original() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../game-data/derived/emu/tornado_path.tsv"
    );
    let Ok(text) = std::fs::read_to_string(path) else {
        eprintln!("no emulator output; skipping");
        return;
    };
    let mut n = 0;
    for line in text.lines() {
        let (head, points) = line.split_once('\t').unwrap();
        let h: Vec<&str> = head.split(' ').collect();
        let seed: u32 = h[0].parse().unwrap();
        let e: Vec<f32> = h[1..]
            .iter()
            .map(|s| s.parse::<f64>().unwrap() as f32)
            .collect();
        let mut line_points = vec![[0.0f32; 3]; 129];
        line_points[0] = [e[0], e[1], 0.0];
        line_points[128] = [e[2], e[3], 0.0];
        let mut rng = Random::new(seed);
        displace(&mut line_points, 0, 128, &mut rng);
        for (i, p) in points.split(' ').enumerate() {
            let want: Vec<u32> = p
                .split(',')
                .map(|b| u32::from_str_radix(b, 16).unwrap())
                .collect();
            let got: Vec<u32> = line_points[i].iter().map(|v| v.to_bits()).collect();
            assert_eq!(got, want, "seed {seed} point {i}");
        }
        n += 1;
    }
    assert!(n > 0);
    eprintln!("{n} lines");
}
