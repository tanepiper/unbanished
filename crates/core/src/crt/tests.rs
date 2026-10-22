use super::*;

/// The table against the Runtime's own, when the binary is at hand.
#[test]
fn the_atan_table_is_the_runtimes() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../game-data/original/Runtime-steam-x64.dll"
    );
    let Ok(img) = std::fs::read(path) else {
        return;
    };
    // .rdata: virtual 0x16f000 at file offset 0x16d400 (the DLL's section table).
    let va = 0x1801724a0u64 - 0x180000000;
    let pe = u32::from_le_bytes(img[0x3c..0x40].try_into().unwrap()) as usize;
    let n = u16::from_le_bytes(img[pe + 6..pe + 8].try_into().unwrap()) as usize;
    let opt = u16::from_le_bytes(img[pe + 20..pe + 22].try_into().unwrap()) as usize;
    let mut off = None;
    for k in 0..n {
        let o = pe + 24 + opt + 40 * k;
        let rd = |i: usize| u32::from_le_bytes(img[o + i..o + i + 4].try_into().unwrap()) as u64;
        let (vsize, rva, raw) = (rd(8), rd(12), rd(20));
        if rva <= va && va < rva + vsize {
            off = Some((va - rva + raw) as usize);
        }
    }
    let off = off.unwrap();
    for (k, &want) in ATAN_TABLE.iter().enumerate() {
        let got = u64::from_le_bytes(img[off + 8 * k..off + 8 * k + 8].try_into().unwrap());
        assert_eq!(got, want, "entry {k}");
    }
}

#[test]
fn plain_values() {
    assert_eq!(sinf(0.0), 0.0);
    assert_eq!(cosf(0.0), 1.0);
    assert_eq!(acosf(1.0), 0.0);
    assert!((atan2f(1.0, 1.0) - std::f32::consts::FRAC_PI_4).abs() < 1e-7);
}
