use super::*;

fn fair() -> Climate {
    Climate {
        baseline: 50.0,
        seasonal: 30.0,
        noise: 5.0,
        baseline_range: 40.0,
        months: [[0.4, 0.5, 0.7, 1.0]; 12],
        bias_range: 0.4,
        tornado: (0.0, 0.0),
    }
}

#[test]
fn slices_and_reseeding() {
    assert_eq!(weather_slice(0.0), 0);
    assert_eq!(weather_slice(75.0), 1);
    assert_eq!(weather_slice(3599.0), 47);
    let mut env = Environment::new(fair());
    let mut rng = Random::new(290);
    env.update(0.0, 0.0, &mut rng);
    assert_eq!(env.slice, 0);
    // Two Rand + 24 RandFloatTol + one RandFloat consumed.
    let mut expect = Random::new(290);
    for _ in 0..2 {
        expect.rand();
    }
    for _ in 0..24 {
        expect.rand_float_tol();
    }
    expect.rand_float();
    assert_eq!(rng.rand(), expect.rand());
    assert!((0..=3).contains(&env.state));
    assert!((0..=5).contains(&env.level));
}

#[test]
fn temperature_follows_the_season() {
    let mut env = Environment::new(Climate {
        noise: 0.0,
        ..fair()
    });
    let mut rng = Random::new(1);
    // Month 0 starts at sin(−30°): baseline − 15.
    env.update(0.0, 0.0, &mut rng);
    assert!((env.temperature - 35.0).abs() < 0.01, "{}", env.temperature);
    assert_eq!(env.season.snow, 0.0);
    // Peak at month 4 (sin 90°).
    env.update(1200.0, 0.0, &mut rng);
    assert!(env.temperature > env.baseline + 29.0);
}

#[test]
fn rain_fades_in_and_out() {
    let mut env = Environment::new(fair());
    env.fade_times = [Some(20.0), Some(20.0)];
    env.temperature = 50.0;
    env.state = 0;
    env.switch_effects();
    assert_eq!(env.effects[0].unwrap().intensity, 0.0);
    assert!(env.effects[1].is_none());
    // 10 s of a 20 s fade.
    for _ in 0..100 {
        env.step_effects(0.1);
    }
    assert!((env.effects[0].unwrap().intensity - 0.5).abs() < 1e-3);
    // Too cold for rain: the rain fades out, the state becomes 3 and snow does not start.
    env.temperature = 30.0;
    env.switch_effects();
    assert_eq!(env.state, 3);
    assert_eq!(env.effects[0].unwrap().direction, -1.0);
    assert!(env.effects[1].is_none());
    for _ in 0..101 {
        env.step_effects(0.1);
    }
    assert!(env.effects[0].is_none());
}

#[test]
fn leaves_and_autumn() {
    let mut env = Environment::new(fair());
    env.update_season(0.0);
    assert_eq!(env.season.leaf_loss, 1.0);
    env.update_season(300.0 * 6.75);
    assert!((env.season.autumn - 0.5).abs() < 1e-6);
    env.update_season(300.0 * 8.0);
    assert_eq!(env.season.leaf_loss, 1.0);
}

/// The weather pick and yearly reseed against the original (`FUN_140098ac0` under the emulator,
/// `tools/emu/weather_pick.py`). Skips when the ground truth has not been made.
#[test]
fn original_weather_pick() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../game-data/derived/emu/weather_pick/cases.tsv");
    let Ok(data) = std::fs::read_to_string(path) else {
        eprintln!("weather pick ground truth missing; skipping");
        return;
    };
    let (mut count, mut reseeded) = (0, 0);
    for line in data.lines().skip(1) {
        let p: Vec<&str> = line.split('\t').collect();
        let f = |i: usize| p[i].parse::<f32>().unwrap();
        let u = |i: usize| p[i].parse::<u32>().unwrap();
        let cuts: Vec<f32> = p[7].split(',').map(|m| m.parse().unwrap()).collect();
        let mut months = [[0.0; 4]; 12];
        for (m, row) in months.iter_mut().enumerate() {
            row.copy_from_slice(&cuts[m * 4..m * 4 + 4]);
        }
        let mut env = Environment::new(Climate {
            baseline: f(4),
            baseline_range: f(5),
            bias_range: f(6),
            months,
            ..fair()
        });
        env.level = p[2].parse().unwrap();
        env.bias = f(3);
        env.baseline_target = -1.0;
        env.bias_target = -1.0;
        let before = env.noise;
        let mut rng = Random::new(u(0));
        let slice = weather_slice(crate::scheduler::seconds(p[1].parse().unwrap()));
        env.next_weather(slice, &mut rng);
        assert_eq!(
            env.state,
            p[8].parse::<i32>().unwrap(),
            "case {count}: state"
        );
        assert_eq!(
            env.level,
            p[9].parse::<i32>().unwrap(),
            "case {count}: level"
        );
        assert_eq!(
            env.baseline_target.to_bits(),
            u(10),
            "case {count}: baseline"
        );
        assert_eq!(env.bias_target.to_bits(), u(11), "case {count}: bias");
        if p[12] != "ee".repeat(16) {
            let n = env.noise;
            let mine: Vec<u8> = [n.persistence.to_bits(), n.octaves.to_bits()]
                .into_iter()
                .chain([n.seed_x as u32, n.seed_y as u32])
                .flat_map(u32::to_le_bytes)
                .collect();
            let hex: String = mine.iter().map(|b| format!("{b:02x}")).collect();
            assert_eq!(hex, p[12], "case {count}: noise");
            reseeded += 1;
        } else {
            assert_eq!(env.noise, before, "case {count}: noise kept");
        }
        assert_eq!(rng.rand(), u(13), "case {count}: draws");
        count += 1;
    }
    assert!(count > 0);
    eprintln!("{count} weather picks match the original ({reseeded} reseeded)");
}
