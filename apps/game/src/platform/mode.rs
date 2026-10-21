//! Pure Banished: a run with only the core, the original game and its own mods (BanishedKit
//! `.pkm` files). Every extension built into the program stays off: no Multiplayer button, no
//! Browse tabs for mod sites, no probe, no recording and no cinematic camera. It is chosen when
//! the program starts: `--pure` on the command line, `?pure` in the browser's address.
//!
//! Deviation: not in the original, which has no extensions to turn off.

use std::sync::atomic::{AtomicBool, Ordering};

static PURE: AtomicBool = AtomicBool::new(false);

/// Starts a Pure Banished run.
pub fn set_pure() {
    PURE.store(true, Ordering::Relaxed);
    log::info!("Pure Banished: the extensions are off");
}

/// Whether this is a Pure Banished run. A build with no extensions never asks.
#[cfg_attr(
    not(any(
        all(feature = "cinematic", not(target_arch = "wasm32")),
        feature = "mod-sites",
        feature = "probe",
        feature = "replay"
    )),
    allow(dead_code)
)]
pub fn pure() -> bool {
    PURE.load(Ordering::Relaxed)
}
