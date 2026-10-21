//! An example rule-hook mod for Un-Banished: a harvest festival every October.
//!
//! On the first iteration of October, if the town has food, the festival is posted to the event
//! log. One year in four (the game's own dice decide, so every player sees the same) the harvest
//! is called bountiful as well.
//!
//! Build it with `cargo build --release --target wasm32-unknown-unknown`, then copy
//! `target/wasm32-unknown-unknown/release/harvest_festival.wasm` into the `Hooks` folder of your
//! Un-Banished data. docs/develop/RULE-HOOKS.md explains every function.
//!
//! The rules a rule hook keeps: no floating point (only integers), nothing kept between calls
//! (a save does not keep a plugin's memory, so every decision comes from what the game gives),
//! and only the game's functions.
#![no_std]

// The game's functions (API version 1).
#[link(wasm_import_module = "unbanished")]
extern "C" {
    fn time_in_year() -> i64;
    fn citizens() -> i32;
    fn stock(slot: i32) -> i32;
    fn random_below(n: i32) -> i32;
    fn post_event(severity: i32, text: *const u8, len: i32);
}

/// The stock slot of food (wood, stone, iron, firewood, tools, food, …).
const FOOD: i32 = 5;
/// A year is 3,600 s of game time, a month 300 s; times are in microseconds.
const MONTH: i64 = 300_000_000;
/// October, the tenth month.
const OCTOBER: i64 = 9 * MONTH;
/// One iteration of the game: 1/60 s.
const ITERATION: i64 = 16_666;

fn post(severity: i32, text: &str) {
    unsafe { post_event(severity, text.as_ptr(), text.len() as i32) }
}

/// Made for the game's rule-hook API, version 1.
#[no_mangle]
pub extern "C" fn api_version() -> i32 {
    1
}

/// After each iteration of the game.
#[no_mangle]
pub extern "C" fn after_iteration() {
    let now = unsafe { time_in_year() };
    // Only on the first iteration of October.
    if !(OCTOBER..OCTOBER + ITERATION).contains(&now) {
        return;
    }
    let (food, people) = unsafe { (stock(FOOD), citizens()) };
    if people == 0 || food == 0 {
        return;
    }
    post(1, "The town holds its harvest festival.");
    if unsafe { random_below(4) } == 0 {
        post(
            4,
            "A bountiful harvest: the festival goes on into the night.",
        );
    }
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    core::arch::wasm32::unreachable()
}
