//! `crt` against the original C runtime maths (`tools/emu/crt_math.py`; skipped without its
//! output in game-data/derived/emu): every result bit for bit (NaNs as NaNs).

use unbanished_game::crt::{acosf, atan2f, cosf, sinf};

#[test]
fn matches_the_original() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../game-data/derived/emu/crt_math.tsv"
    );
    let Ok(text) = std::fs::read_to_string(path) else {
        eprintln!("no emulator output; skipping");
        return;
    };
    let f = |s: &str| f32::from_bits(u32::from_str_radix(s, 16).unwrap());
    let mut checked = std::collections::BTreeMap::<&str, usize>::new();
    let mut wrong = Vec::new();
    for line in text.lines() {
        let mut w = line.split('\t');
        let (name, args, want) = (w.next().unwrap(), w.next().unwrap(), f(w.next().unwrap()));
        let got = match name {
            "sinf" => sinf(f(args)),
            "cosf" => cosf(f(args)),
            "acosf" => acosf(f(args)),
            "atan2f" => {
                let (y, x) = args.split_once(',').unwrap();
                atan2f(f(y), f(x))
            }
            _ => unreachable!(),
        };
        let same = got.to_bits() == want.to_bits() || (got.is_nan() && want.is_nan());
        if !same && wrong.len() < 10 {
            wrong.push(format!(
                "{name}({args}) = {got:?} ({:08x}), want {want:?} ({:08x})",
                got.to_bits(),
                want.to_bits()
            ));
        }
        *checked.entry(name).or_default() += 1;
    }
    eprintln!("{checked:?}");
    assert!(wrong.is_empty(), "{wrong:#?}");
}
