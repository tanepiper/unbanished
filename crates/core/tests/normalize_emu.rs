//! Vector normalisation against the original Runtime (`Vector3::Normalize` with and without its
//! length, `Vector2::Normalize` with and without, under the emulator, `tools/emu/normalize.py`):
//! every result bit for bit. Skips when the ground truth has not been made.
use unbanished_core::math::{normalize2, normalize3, normalize3_len};

fn f(hex: &str) -> f32 {
    f32::from_bits(u32::from_str_radix(hex, 16).unwrap())
}

fn bits(v: &[f32]) -> Vec<u32> {
    v.iter().map(|x| x.to_bits()).collect()
}

#[test]
fn original_normalize() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../game-data/derived/emu/normalize/cases.tsv");
    let Ok(data) = std::fs::read_to_string(path) else {
        eprintln!("normalize ground truth missing; skipping");
        return;
    };
    let mut count = 0;
    for line in data.lines().skip(1) {
        let p: Vec<f32> = line.split('\t').map(f).collect();
        let v = [p[0], p[1], p[2]];
        let (n, len) = normalize3_len(v);
        assert_eq!(
            bits(&n),
            bits(&p[3..6]),
            "case {count}: Vector3 with its length {v:?}"
        );
        assert_eq!(len.to_bits(), p[6].to_bits(), "case {count}: length {v:?}");
        assert_eq!(
            bits(&normalize3(v)),
            bits(&p[7..10]),
            "case {count}: Vector3 {v:?}"
        );
        let v2 = normalize2([p[0], p[1]]);
        assert_eq!(
            bits(&v2),
            bits(&p[10..12]),
            "case {count}: Vector2 with length {v:?}"
        );
        assert_eq!(bits(&v2), bits(&p[13..15]), "case {count}: Vector2 {v:?}");
        count += 1;
    }
    assert!(count > 0);
    eprintln!("{count} normalisations match the original");
}
