//! A trading post's next merchant against the original (`FUN_14005be50` and `FUN_14005bdb0` under
//! the emulator, `tools/emu/trade_timer.py`): the row for the population, the time to the next
//! merchant, and that one number was drawn. Skips when the ground truth has not been made.
use unbanished_formats::game::TradeRow;
use unbanished_game::commerce::trade::{next_merchant, trade_row};
use unbanished_game::random::Random;

#[test]
fn original_trade_timer() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../game-data/derived/emu/trade_timer/cases.tsv");
    let Ok(data) = std::fs::read_to_string(path) else {
        eprintln!("trade timer ground truth missing; skipping");
        return;
    };
    let mut count = 0;
    for line in data.lines().skip(1) {
        let p: Vec<&str> = line.split('\t').collect();
        let rows: Vec<TradeRow> = p[2]
            .split(';')
            .map(|r| {
                let v: Vec<&str> = r.split(',').collect();
                TradeRow {
                    population: v[0].parse().unwrap(),
                    months: v[1].parse().unwrap(),
                    range: v[2].parse().unwrap(),
                    ..Default::default()
                }
            })
            .collect();
        let row = trade_row(&rows, p[1].parse().unwrap());
        assert_eq!(
            row as i64,
            p[3].parse::<i64>().unwrap(),
            "case {count}: row"
        );
        let mut rng = Random::new(p[0].parse().unwrap());
        let before = rng.clone();
        let timer = next_merchant(&rows[row], &mut rng);
        let moved = rng.index() != before.index() || rng.state() != before.state();
        assert_eq!(timer, p[4].parse::<i64>().unwrap(), "case {count}: timer");
        assert_eq!(moved, p[5] == "1", "case {count}: drew");
        count += 1;
    }
    assert!(count > 0);
    eprintln!("{count} trade timers match the original");
}
