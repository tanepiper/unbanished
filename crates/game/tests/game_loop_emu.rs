//! Original Game/Game pause/speed guards, calendar and phase observations (game_loop.py).
use std::path::PathBuf;
use unbanished_game::calendar::Calendar;

#[test]
fn calendar_and_phase_contract_match_original_loop() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../game-data/derived/emu/game-loop/loop.tsv");
    let Ok(data) = std::fs::read_to_string(path) else {
        eprintln!("no loop ground truth; run tools/emu/game_loop.py");
        return;
    };
    let mut total = 0;
    for line in data.lines() {
        let fields: Vec<_> = line.split('\t').collect();
        let speed: usize = fields[0].parse().unwrap();
        let paused = fields[1] == "1";
        let modal = fields[2] == "1";
        let initial: i64 = fields[3].parse().unwrap();
        let delta: i64 = fields[4].parse().unwrap();
        let mut calendar = Calendar {
            elapsed: initial,
            ..Calendar::default()
        };
        let iterations = if paused || modal { 0 } else { speed };
        let mut phases = Vec::new();
        for iteration in 0..iterations {
            phases.push("map".to_owned());
            phases.push(format!(
                "animations:{}",
                usize::from(iteration + 1 < iterations)
            ));
            phases.push(format!("entities:{}", calendar.elapsed));
            calendar.advance(delta);
            phases.push(format!("environment:{}", calendar.elapsed));
            phases.push("data".to_owned());
        }
        if !modal {
            phases.push("followup".to_owned());
        }
        phases.push("ui".to_owned());
        assert_eq!(calendar.year, fields[5].parse().unwrap(), "year: {line}");
        assert_eq!(calendar.elapsed, fields[6].parse().unwrap(), "time: {line}");
        assert_eq!(phases.join(","), fields[7], "phase contract: {line}");
        total += 1;
    }
    assert_eq!(total, 96);
    eprintln!("{total} original pause/speed/calendar/phase observations matched");
}
