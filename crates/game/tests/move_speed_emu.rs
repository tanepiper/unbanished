//! The speed factor of a MoveTo action against the original (`FUN_1400ce640` under the emulator,
//! `tools/emu/move_speed.py`): the factor's bits, and whether a number was drawn. Skips when the
//! ground truth has not been made.
use unbanished_game::behaviour::character::MoveTo;
use unbanished_game::random::Random;

#[test]
fn original_move_speed() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../game-data/derived/emu/move_speed/cases.tsv");
    let Ok(data) = std::fs::read_to_string(path) else {
        eprintln!("move speed ground truth missing; skipping");
        return;
    };
    let (mut count, mut drew) = (0, 0);
    for row in data.lines().skip(1) {
        let p: Vec<u64> = row.split('\t').map(|v| v.parse().unwrap()).collect();
        let mut rng = Random::new(p[0] as u32);
        let before = rng.clone();
        let speed = f32::from_bits(p[1] as u32);
        let m = MoveTo::new([1.0, 2.0, 3.0], 3, 4, 5, 6, speed, &mut rng);
        let moved = rng.index() != before.index() || rng.state() != before.state();
        assert_eq!(
            m.speed.to_bits(),
            p[2] as u32,
            "case {count}: speed {speed}"
        );
        assert_eq!(moved, p[3] == 1, "case {count}: drew");
        drew += moved as usize;
        count += 1;
    }
    assert!(count > 0);
    eprintln!("{count} MoveTo speeds match the original ({drew} drew)");
}
