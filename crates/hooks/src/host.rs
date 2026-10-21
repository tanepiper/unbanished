//! What a plugin may call: the host functions of API version 1, all in the import module
//! `unbanished`, all integers. During a call a plugin's store holds a copy of the values it may
//! read, the game's own random stream (moved in for the call and moved back after it), and the
//! events it posts, which are added to the log in order after the call.

use unbanished_game::random::Random;
use wasmi::{Caller, Extern, Linker, StoreLimits};

/// The values a plugin may read, and what it did, for one call.
pub struct Host {
    pub year: i32,
    pub month: i32,
    pub time_in_year: i64,
    pub citizens: i32,
    pub stock: Vec<i32>,
    /// The game's random stream during a call.
    pub rng: Random,
    /// The events posted during a call: severity and text.
    pub events: Vec<(i32, String)>,
    /// How much memory a plugin may grow to.
    pub limits: StoreLimits,
}

/// The longest event text a plugin may post, in bytes.
const MAX_EVENT: usize = 256;

/// The host functions of API version 1.
pub fn link(linker: &mut Linker<Host>) -> Result<(), wasmi::Error> {
    linker.func_wrap("unbanished", "year", |c: Caller<'_, Host>| c.data().year)?;
    linker.func_wrap("unbanished", "month", |c: Caller<'_, Host>| c.data().month)?;
    linker.func_wrap("unbanished", "time_in_year", |c: Caller<'_, Host>| {
        c.data().time_in_year
    })?;
    linker.func_wrap("unbanished", "citizens", |c: Caller<'_, Host>| {
        c.data().citizens
    })?;
    linker.func_wrap("unbanished", "stock", |c: Caller<'_, Host>, slot: i32| {
        usize::try_from(slot)
            .ok()
            .and_then(|s| c.data().stock.get(s))
            .copied()
            .unwrap_or(-1)
    })?;
    linker.func_wrap(
        "unbanished",
        "random_below",
        |mut c: Caller<'_, Host>, n: i32| -> i32 {
            if n < 1 {
                return 0;
            }
            c.data_mut().rng.rand_below(n as u32) as i32
        },
    )?;
    linker.func_wrap(
        "unbanished",
        "post_event",
        |mut c: Caller<'_, Host>, severity: i32, ptr: i32, len: i32| -> Result<(), wasmi::Error> {
            let memory = match c.get_export("memory") {
                Some(Extern::Memory(m)) => m,
                _ => return Err(wasmi::Error::new("post_event: the plugin has no memory")),
            };
            let len = usize::try_from(len).unwrap_or(0).min(MAX_EVENT);
            let mut bytes = vec![0; len];
            memory
                .read(&c, ptr as u32 as usize, &mut bytes)
                .map_err(|_| wasmi::Error::new("post_event: text outside the plugin's memory"))?;
            let text = String::from_utf8_lossy(&bytes).into_owned();
            c.data_mut().events.push((severity, text));
            Ok(())
        },
    )?;
    Ok(())
}
