//! Original-binary parity (tools/emu/growth.py); local ground truth is never committed.
use banished_formats::game::GrowthDescription;
use banished_game::growth::Growth;
use banished_game::random::Random;
use banished_game::scheduler::{ticks, EntityScheduler};
use std::path::PathBuf;

fn description() -> GrowthDescription {
    GrowthDescription {
        max_base: 1.0,
        max_range: 0.0,
        f28: 2.0,
        b2c: false,
        f30: 60.0,
        f34: 20.0,
        b38: false,
        f3c: 0.0,
        f40: 0.5,
        b44: true,
        spawn_radius: 1,
        timer_months: 1,
        spawn_one_in: 1,
        threshold: 0.75,
        max_neighbours: 8,
        max_same: 8,
        f60: 0.1,
        tall: false,
    }
}

fn truth(name: &str) -> Option<String> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../game-data/derived/emu/growth")
        .join(name);
    match std::fs::read_to_string(path) {
        Ok(data) => Some(data),
        Err(_) => {
            eprintln!("no emulator ground truth; run tools/emu/growth.py ({name})");
            None
        }
    }
}

#[test]
fn scheduled_growth_and_smoothing_match_original_bits() {
    let Some(data) = truth("growth.tsv") else {
        return;
    };
    let mut total = 0;
    for line in data.lines() {
        let f: Vec<_> = line.split('\t').collect();
        let float = |i: usize| f[i].parse::<f32>().unwrap();
        let int = |i: usize| f[i].parse::<i64>().unwrap();
        let mut d = description();
        d.f28 = float(0);
        d.b2c = int(1) != 0;
        d.tall = int(7) != 0;
        let mut value = float(3);
        let mut state = Growth {
            maximum: float(4),
            timer: int(6),
            smoothed: value.min(1.0),
            scheduled: true,
            smoothing: false,
            allow_negative: int(8) != 0,
        };
        let result = state.update(&d, &mut value, ticks(float(5)), float(2), int(9) != 0);
        let mut output = [0.0; 4];
        state.smooth(&d, value, &mut output);
        let byte = result
            .map_adjustment
            .map_or(128, |a| banished_game::growth::adjust_map_byte(128, a));
        let mut got = vec![
            value.to_bits() as i64,
            state.timer,
            state.smoothed.to_bits() as i64,
            (if state.scheduled { 8 } else { 0 }) | (if state.smoothing { 16 } else { 0 }),
            result.spawn_attempts as i64,
            byte as i64,
        ];
        got.extend(if state.scheduled {
            output.map(|v| v.to_bits() as i64)
        } else {
            [0; 4]
        });
        let expected: Vec<i64> = f[10..].iter().map(|s| s.parse().unwrap()).collect();
        assert_eq!(got, expected, "growth case: {line}");
        total += 1;
    }
    assert_eq!(total, 15_360);
    eprintln!("{total} growth/transition/smoothing cases matched original bits");
}

#[test]
fn initialization_retains_original_timer_and_scale() {
    let Some(data) = truth("initialise.tsv") else {
        return;
    };
    let mut total = 0;
    for line in data.lines() {
        let f: Vec<_> = line.split('\t').collect();
        let mut rng = Random::new(f[0].parse().unwrap());
        let mut d = description();
        d.tall = f[3] == "1";
        let (state, value, output) =
            Growth::initialise(&d, f[1].parse().unwrap(), f[2].parse().unwrap(), &mut rng);
        let mut got = vec![
            value.to_bits() as i64,
            state.timer,
            state.smoothed.to_bits() as i64,
        ];
        got.extend(output.map(|v| v.to_bits() as i64));
        let expected: Vec<i64> = f[4..].iter().map(|s| s.parse().unwrap()).collect();
        assert_eq!(got, expected, "initialization case: {line}");
        total += 1;
    }
    assert_eq!(total, 480);
    eprintln!("{total} growth initialization cases matched original bits");
}

#[test]
fn five_bucket_visits_match_original() {
    let Some(data) = truth("scheduler.tsv") else {
        return;
    };
    let mut scheduler = EntityScheduler::default();
    let mut next = 0;
    for (bucket, count) in [53, 18, 11, 8, 5].into_iter().enumerate() {
        for _ in 0..count {
            scheduler.register(next, bucket);
            next += 1;
        }
    }
    let mut rows = Vec::new();
    for step in 0..8 {
        scheduler.advance(0.25);
        for bucket in 0..5 {
            rows.extend(
                scheduler
                    .visit_bucket(bucket)
                    .into_iter()
                    .map(|(_, id, dt)| format!("{step}\t{id}\t{dt}")),
            );
        }
    }
    assert_eq!(rows.len(), 640);
    let expected: Vec<_> = data.lines().collect();
    assert_eq!(rows.len(), expected.len());
    for (i, (got, expected)) in rows.iter().zip(expected).enumerate() {
        assert_eq!(got, expected, "scheduler visit {i}");
    }
}
