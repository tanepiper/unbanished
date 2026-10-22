//! The chance of an event that happens on average once in a span of years: an injury at work, a
//! tornado while it rains, a fire that starts by itself.

use crate::random::Random;
use crate::scheduler::seconds;

/// Whether an event that happens on average once in `years × odds` game years happens now, given
/// the time on its clock (game ticks since the last check). The clock restarts. With odds of 0
/// or less nothing is drawn.
///
/// Measured: `tools/emu/once_in_years.py` (the original on 2,000 cases).
///
/// Original: `FUN_1400ae760`.
pub fn once_in_years(timer: &mut i64, years: f32, odds: f32, rng: &mut Random) -> bool {
    // param_2[1] <= 0: the clock clears, no draw.
    if odds <= 0.0 {
        *timer = 0;
        return false;
    }
    // DAT_1401788b8 (12, months) and DAT_140178934 (300 s, a month), multiplied in that order.
    let span = years * 12.0 * 300.0;
    let since = seconds(*timer);
    *timer = 0;
    rng.rand_float() < since / (span * odds)
}
