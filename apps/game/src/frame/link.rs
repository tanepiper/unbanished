//! `--frame-io`: the program started by the frame (GameFrame's JSON lines, through
//! `gameframe_frame::Link`). The first line on standard output is the manifest; then each line
//! in and out is one event. The game's log goes to standard error, which the frame shows in its
//! console, so nothing else may print to standard output.
//!
//! One program runs one game: winit makes its event loop once in a process, so the frame starts
//! the program again for the next game (GameFrame's `Respawn`).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

use gameframe_frame::{report, Link};

use super::{Kernel, Launch};
use crate::platform::steam;

static LINK: OnceLock<Link> = OnceLock::new();
static RUNNING: AtomicBool = AtomicBool::new(false);

/// Answer the frame until it starts a game. `None` when the frame went away first.
pub(crate) fn wait() -> Option<Launch> {
    let mut kernel = Kernel::new(steam::installs());
    let (launch, link) = Link::wait(&mut kernel, Kernel::take_launch)?;
    report::install(report::Meta {
        game: "Un-Banished".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        issues: "tanepiper/banished-wasm".into(),
        ..Default::default()
    });
    // Panics become reports, sent to the frame's Play tab.
    link.send_reports(serde_json::json!({ "video": super::manifest::WGPU }));
    let _ = LINK.set(link);
    Some(launch)
}

/// Whether the frame asked the running game to stop, or went away.
pub(crate) fn stopping() -> bool {
    LINK.get().is_some_and(Link::stopping)
}

/// `game.status running`, once, when the game has its window and its packages.
pub(crate) fn running() {
    if let Some(link) = LINK.get() {
        if !RUNNING.swap(true, Ordering::Relaxed) {
            link.status("running", "");
        }
    }
}

/// `game.status` when the game ends: `stopped`, or `failed` with the reason.
pub(crate) fn ended(state: &str, detail: &str) {
    if let Some(link) = LINK.get() {
        report::flush();
        link.status(state, detail);
    }
}
