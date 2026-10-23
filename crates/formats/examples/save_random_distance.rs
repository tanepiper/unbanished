//! How many draws the game's Random made between two saves of one town (analysis helper), and
//! how much entity-manager time passed. The Random is a Mersenne Twister whose index and 624
//! words a save stores, so stepping the earlier state forward until it equals the later one
//! counts the draws exactly. Pairs with `--sim-continue`: a port that runs the same time from
//! the earlier save should make the same number of draws.
//! `cargo run -p unbanished-formats --example save_random_distance -- EARLIER.sav LATER.sav [MAX_GROUPS]`
use unbanished_formats::save::Save;
use unbanished_formats::savegame::GameState;

const N: usize = 624;
const M: usize = 397;

/// One regeneration of the 624 words, as `unbanished_core::random` does.
fn next_group(s: &mut [u32]) {
    for i in 0..N {
        let y = (s[i] & 0x8000_0000) | (s[(i + 1) % N] & 0x7fff_ffff);
        let mut v = s[(i + M) % N] ^ (y >> 1);
        if y & 1 != 0 {
            v ^= 0x9908_b0df;
        }
        s[i] = v;
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let load = |p: &str| {
        GameState::decode(&Save::parse(&std::fs::read(p).unwrap()).unwrap().state).unwrap()
    };
    let (a, b) = (load(&args[0]), load(&args[1]));
    let max: u64 = args
        .get(2)
        .and_then(|s| s.parse().ok())
        .unwrap_or(10_000_000);
    let dt = b.entity_time - a.entity_time;
    println!(
        "entity time {} -> {} ({dt} µs, {:.6} s)",
        a.entity_time,
        b.entity_time,
        dt as f64 / 1e6
    );
    // An index of 0 means the group is used up and is regenerated at the next draw.
    let pos = |i: u32| if i == 0 { N as u64 } else { i as u64 };
    let mut s = a.random_state.clone();
    for g in 0..=max {
        if s == b.random_state {
            let draws = g * N as u64 + pos(b.random_index) - pos(a.random_index);
            println!(
                "random draws {draws} ({g} groups; index {} -> {})",
                a.random_index, b.random_index
            );
            return;
        }
        next_group(&mut s);
    }
    println!("the later state is not within {max} groups of the earlier one");
}
