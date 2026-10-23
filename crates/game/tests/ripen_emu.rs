//! An orchard tree's fruit ripening against the original (`FUN_14004a740` under the emulator,
//! `tools/emu/ripen.py`), NaN growths and ranges of 0 among the cases, bit for bit. Skips when the
//! ground truth has not been made.
use unbanished_game::production::orchard::{ripen, Fruit, FruitTemplate};
use unbanished_game::scheduler::seconds;

#[test]
fn original_ripen() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../game-data/derived/emu/ripen/cases.tsv");
    let Ok(data) = std::fs::read_to_string(path) else {
        eprintln!("ripen ground truth missing; skipping");
        return;
    };
    let (mut count, mut differ) = (0, Vec::new());
    for line in data.lines().skip(1) {
        let p: Vec<&str> = line.split('\t').collect();
        let hex = |i: usize| f32::from_bits(u32::from_str_radix(p[i], 16).unwrap());
        let d = FruitTemplate {
            months: hex(0),
            best: hex(1),
            range: hex(2),
            pile: None,
        };
        let mut f = Fruit {
            ripeness: hex(4),
            picked: p[5] == "1",
        };
        let time: i64 = p[6].parse().unwrap();
        ripen(
            &mut f,
            &d,
            hex(3),
            p[8].parse().unwrap(),
            seconds(time),
            hex(7),
        );
        let want = u32::from_str_radix(p[9], 16).unwrap();
        if f.ripeness.to_bits() != want || f.picked != (p[10] == "1") {
            differ.push(format!("{line} -> {} {}", f.ripeness, f32::from_bits(want)));
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
    eprintln!("{count} ripenings match the original");
}
