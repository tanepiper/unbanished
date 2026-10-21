//! A vanilla run: a run with only the core, the original game and its own mods (BanishedKit
//! `.pkm` files). Every extension built into the program stays off: no Multiplayer button, no
//! Browse tabs for mod sites, no probe, no recording and no cinematic camera. It is chosen when
//! the program starts: `--vanilla` on the command line, `?vanilla` in the browser's address.
//!
//! Deviation: not in the original, which has no extensions to turn off.

use std::sync::atomic::{AtomicBool, Ordering};

static VANILLA: AtomicBool = AtomicBool::new(false);

/// Starts a vanilla run.
pub fn set_vanilla() {
    VANILLA.store(true, Ordering::Relaxed);
    log::info!("vanilla run: the extensions are off");
}

/// Whether this is a vanilla run. A build with no extensions never asks.
#[cfg_attr(
    not(any(
        all(feature = "cinematic", not(target_arch = "wasm32")),
        feature = "mod-sites",
        feature = "probe",
        feature = "replay"
    )),
    allow(dead_code)
)]
pub fn vanilla() -> bool {
    VANILLA.load(Ordering::Relaxed)
}
