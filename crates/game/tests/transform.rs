//! `math::Transform` against the Runtime's (`tools/emu/transform.py`; skipped without its
//! output in game-data/derived/emu): the quaternion, the transformed point and the turned
//! heading and the inverse, bit for bit.

use unbanished_game::math::Transform;

#[test]
fn matches_the_original() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../game-data/derived/emu/transform.tsv"
    );
    let Ok(text) = std::fs::read_to_string(path) else {
        eprintln!("no emulator output; skipping");
        return;
    };
    let f = |s: &str| f32::from_bits(u32::from_str_radix(s, 16).unwrap());
    let fs = |s: &str| s.split(' ').map(f).collect::<Vec<f32>>();
    let bits = |v: &[f32]| v.iter().map(|x| x.to_bits()).collect::<Vec<_>>();
    let mut n = 0;
    for line in text.lines() {
        let cols: Vec<&str> = line.split('\t').collect();
        let i = fs(cols[0]);
        let t = Transform::from_yaw_pitch_offset(i[0], i[1], [i[2], i[3], i[4]]);
        assert_eq!(bits(&t.q), bits(&fs(cols[1])), "q {line}");
        let p = t.apply([i[5], i[6], i[7]]);
        assert_eq!(bits(&p), bits(&fs(cols[2])), "point {line}");
        assert_eq!(
            t.heading_of(i[8]).to_bits(),
            f(cols[3]).to_bits(),
            "heading {line}"
        );
        if let Some(c) = cols.get(4) {
            let inv = t.inverse();
            let got: Vec<f32> = inv.q.iter().chain(&inv.p).copied().collect();
            assert_eq!(bits(&got), bits(&fs(c)), "inverse {line}");
        }
        n += 1;
    }
    assert!(n > 0);
    eprintln!("{n} transforms");
}
