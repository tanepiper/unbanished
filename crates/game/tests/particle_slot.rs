//! `Tornado::step_slot` against the original's particle slot fade (`FUN_140053490`,
//! `tools/emu/particle_slot.py`; skipped without its output in game-data/derived/emu): the same
//! alpha after every step, and gone on the same step.

use unbanished_game::disasters::tornado::Tornado;
use unbanished_game::scheduler::seconds;

#[test]
fn matches_the_original() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../game-data/derived/emu/particle_slot.tsv"
    );
    let Ok(text) = std::fs::read_to_string(path) else {
        eprintln!("no emulator output; skipping");
        return;
    };
    let (mut fades, mut gone) = (0, 0);
    for line in text.lines() {
        let (head, steps) = line.split_once('\t').unwrap();
        let (fade, out) = head.split_once(' ').unwrap();
        let (fade, out): (i64, usize) = (fade.parse().unwrap(), out.parse().unwrap());
        let rate = 1.0 / seconds(fade);
        let mut t = Tornado {
            target: [0.0; 3],
            path: Vec::new(),
            index: 0,
            timer: 0,
            position: [0.0; 3],
            emitting: true,
            slot: Some((0.0, rate)),
        };
        for (k, step) in steps.split(' ').enumerate() {
            if k == out {
                if let Some((_, r)) = &mut t.slot {
                    *r = -(1.0 / seconds(fade));
                }
            }
            let (dt, want) = step.split_once(':').unwrap();
            t.step_slot(dt.parse().unwrap());
            if want == "gone" {
                assert!(t.slot.is_none(), "{head} step {k}: still there");
                gone += 1;
            } else {
                let bits = u32::from_str_radix(want, 16).unwrap();
                let (alpha, _) = t
                    .slot
                    .unwrap_or_else(|| panic!("{head} step {k}: gone early"));
                assert_eq!(alpha.to_bits(), bits, "{head} step {k}");
            }
        }
        fades += 1;
    }
    assert!(fades > 0 && gone > 0);
    eprintln!("{fades} fades, {gone} gone");
}

/// One step from a NaN, infinite or zero alpha or rate (`particle_slot_nan.tsv`): the slot goes
/// on the same steps, and the alpha and rate left are the original's to the bit.
#[test]
fn nan_and_infinite_slots_match_the_original() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../game-data/derived/emu/particle_slot_nan.tsv"
    );
    let Ok(text) = std::fs::read_to_string(path) else {
        eprintln!("no emulator output; skipping");
        return;
    };
    let bits = |s: &str| f32::from_bits(u32::from_str_radix(s, 16).unwrap());
    let mut cases = 0;
    for line in text.lines() {
        let (head, want) = line.split_once('\t').unwrap();
        let f: Vec<&str> = head.split(' ').collect();
        let mut t = Tornado {
            target: [0.0; 3],
            path: Vec::new(),
            index: 0,
            timer: 0,
            position: [0.0; 3],
            emitting: true,
            slot: Some((bits(f[0]), bits(f[1]))),
        };
        t.step_slot(f[2].parse().unwrap());
        let got = match t.slot {
            None => "gone".to_owned(),
            Some((a, r)) => format!("{:08x} {:08x}", a.to_bits(), r.to_bits()),
        };
        assert_eq!(got, want, "{head}");
        cases += 1;
    }
    assert_eq!(cases, 120);
}
