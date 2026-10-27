//! The game as the kernel of GameFrame's frame (`unbanished --frame-io`), the launcher that
//! `apps/frame` opens (docs/develop/FRAME.md). The frame shows the player's choices; the
//! kernel turns them into a [`Launch`]: which machine, which copy of Banished, sound on or off.
//! [`link`] speaks GameFrame's JSON lines on standard input and output and runs one game per
//! program; the frame starts the program again for the next game.
//!
//! Deviation: the frame is an addition the original does not have. It reaches the game only
//! through the launch, before the window opens, and through `game.stop` while it runs.
#![cfg(not(target_arch = "wasm32"))]

mod kernel;
pub(crate) mod link;
pub(crate) mod manifest;

pub(crate) use kernel::{Kernel, Launch};

/// With `--frame-io` first, wait for the frame's `game.start` and give the program's
/// arguments for that launch: the data folder, `--vanilla` for Banished, then the player's
/// options. `None` when the frame went away before a start. Other arguments are kept as given.
pub(crate) fn take_args(args: Vec<String>) -> Option<Vec<String>> {
    if args.first().map(String::as_str) != Some("--frame-io") {
        return Some(args);
    }
    let launch = link::wait()?;
    log::info!("frame: {launch:?}");
    if launch.mute {
        // Read by the sound device when it opens; nothing else reads it before then.
        std::env::set_var("UNBANISHED_MUTE", "1");
    }
    let mut out = vec![launch.data.to_string_lossy().into_owned()];
    if launch.vanilla {
        out.push("--vanilla".into());
    }
    out.extend(launch.args);
    out.extend(args.into_iter().skip(1));
    Some(out)
}
