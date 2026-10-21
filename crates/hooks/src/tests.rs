//! Plugins written as WebAssembly text (wasmi reads it in tests).

use unbanished_game::calendar::Calendar;
use unbanished_game::events::EventLog;
use unbanished_game::random::Random;
use unbanished_sim::hooks::{HookContext, RuleHooks};

use super::*;

/// A plugin that, after each iteration, draws a number below 100 and posts an event with it as
/// a letter: "A" + n % 26.
const FESTIVAL: &str = r#"
(module
  (import "unbanished" "random_below" (func $random (param i32) (result i32)))
  (import "unbanished" "post_event" (func $post (param i32 i32 i32)))
  (import "unbanished" "citizens" (func $citizens (result i32)))
  (memory (export "memory") 1)
  (func (export "api_version") (result i32) i32.const 1)
  (func (export "after_iteration")
    (i32.store8 (i32.const 0)
      (i32.add (i32.const 65) (i32.rem_u (call $random (i32.const 100)) (i32.const 26))))
    (if (i32.gt_s (call $citizens) (i32.const 0))
      (then (call $post (i32.const 1) (i32.const 0) (i32.const 1))))))
"#;

/// Runs `hooks` once with `rng`, 5 citizens and no stock: the events posted.
fn run(hooks: &mut Plugins, rng: &mut Random) -> Vec<String> {
    let calendar = Calendar::default();
    let mut events = EventLog::default();
    let stock = [0; 4];
    let mut cx = HookContext::new(&calendar, 0, 5, &stock, rng, &mut events);
    hooks.after_iteration(&mut cx);
    events.entries.into_iter().filter_map(|e| e.text).collect()
}

fn load(text: &str) -> Result<Plugins, String> {
    Plugins::load(&[("test".to_string(), text.as_bytes().to_vec())])
}

#[test]
fn a_plugin_draws_from_the_games_stream_and_posts_events() {
    let mut hooks = load(FESTIVAL).unwrap();
    let mut rng = Random::new(1234);
    let posted = run(&mut hooks, &mut rng);
    // The same draw, made directly, gives the same letter and leaves the stream in the same place.
    let mut direct = Random::new(1234);
    let letter = char::from(b'A' + (direct.rand_below(100) % 26) as u8);
    assert_eq!(posted, [letter.to_string()]);
    assert_eq!(
        rng.rand(),
        direct.rand(),
        "the stream moved on exactly once"
    );
}

#[test]
fn two_runs_from_one_seed_are_the_same() {
    let mut a = load(FESTIVAL).unwrap();
    let mut b = load(FESTIVAL).unwrap();
    let (mut ra, mut rb) = (Random::new(9), Random::new(9));
    for _ in 0..50 {
        assert_eq!(run(&mut a, &mut ra), run(&mut b, &mut rb));
    }
}

#[test]
fn floating_point_is_refused() {
    let text = r#"(module (func (export "api_version") (result i32)
        (i32.trunc_f32_s (f32.const 1.5))))"#;
    let e = load(text).err().expect("refused");
    assert!(e.contains("floating point"), "{e}");
}

#[test]
fn another_api_version_is_refused() {
    let text = r#"(module (func (export "api_version") (result i32) i32.const 2))"#;
    let e = load(text).err().expect("refused");
    assert!(e.contains("API 2") && e.contains("API 1"), "{e}");
}

#[test]
fn only_the_games_functions_may_be_imported() {
    let text = r#"(module (import "env" "open_file" (func))
        (func (export "api_version") (result i32) i32.const 1))"#;
    assert!(load(text).is_err());
}

#[test]
fn a_plugin_that_never_stops_runs_out_of_fuel_and_is_switched_off() {
    let text = r#"(module
      (import "unbanished" "random_below" (func $random (param i32) (result i32)))
      (func (export "api_version") (result i32) i32.const 1)
      (func (export "after_iteration")
        (drop (call $random (i32.const 10)))
        (loop $forever (br $forever))))"#;
    let mut hooks = load(text).unwrap();
    let mut rng = Random::new(5);
    assert!(run(&mut hooks, &mut rng).is_empty());
    assert!(hooks.plugins[0].stopped);
    // The draw it made before it ran out stays made, as on every computer.
    let mut direct = Random::new(5);
    direct.rand_below(10);
    assert_eq!(rng.rand(), direct.rand());
    // Switched off, it no longer runs.
    let before = rng.clone();
    run(&mut hooks, &mut rng);
    assert_eq!(rng.rand(), before.clone().rand());
}

#[test]
fn the_fingerprint_follows_the_plugins_and_their_order() {
    let a = ("a".to_string(), vec![1u8, 2]);
    let b = ("b".to_string(), vec![3u8]);
    assert_eq!(fingerprint(&[]), 0);
    assert_ne!(
        fingerprint(&[a.clone(), b.clone()]),
        fingerprint(&[b, a.clone()])
    );
    assert_ne!(
        fingerprint(&[a.clone()]),
        fingerprint(&[("a".to_string(), vec![1u8, 3])])
    );
}

/// The example mod (examples/rule-hooks/harvest-festival), when it has been built: it celebrates
/// on the first iteration of Autumn and nowhere else.
#[test]
fn the_example_mod_holds_its_festival_in_autumn() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(
        "../../examples/rule-hooks/harvest-festival/target/wasm32-unknown-unknown/release/harvest_festival.wasm",
    );
    let Ok(bytes) = std::fs::read(&path) else {
        eprintln!("the example mod is not built; skipping");
        return;
    };
    let mut hooks = Plugins::load(&[("harvest_festival".to_string(), bytes)]).unwrap();
    let posted = |hooks: &mut Plugins, elapsed: i64, seed: u32| {
        let calendar = Calendar {
            elapsed,
            ..Calendar::default()
        };
        let mut events = EventLog::default();
        let mut stock = [0; 21];
        stock[5] = 300;
        let mut rng = Random::new(seed);
        let mut cx = HookContext::new(&calendar, 7, 40, &stock, &mut rng, &mut events);
        hooks.after_iteration(&mut cx);
        events.entries.len()
    };
    let autumn = 7 * 300_000_000;
    assert_eq!(
        posted(&mut hooks, autumn - 1, 1),
        0,
        "the end of Late Summer"
    );
    assert_eq!(
        posted(&mut hooks, autumn + 16_666, 1),
        0,
        "the second iteration of Autumn"
    );
    // On the first iteration of Autumn: the festival, and in some years a second line.
    let lines: Vec<usize> = (0..40)
        .map(|seed| posted(&mut hooks, autumn, seed))
        .collect();
    assert!(lines.iter().all(|&n| n == 1 || n == 2), "{lines:?}");
    assert!(lines.contains(&2) && lines.contains(&1), "{lines:?}");
}
