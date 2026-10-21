//! The Un-Banished engine version, `<version>-<build>`: the workspace version from `Cargo.toml` and
//! the build id that `build.rs` sets (a short git commit hash, `-dirty` for uncommitted changes).
//!
//! This is not the Banished game version. The main menu still shows the original's own text
//! ("Version 1.0.7 - Build 170910") from the game data, because that names the data the engine
//! plays. The engine version appears only outside the original's interface: `unbanished --version`,
//! the first log line, and the browser's loading panel.

/// For example `Un-Banished More 0.5.0-1382712`, or `Un-Banished More 0.5.0-1382712-dirty`.
/// A build with the `more` feature (every stable extension, the default) is Un-Banished More; a
/// build without it is Un-Banished.
#[cfg(feature = "more")]
pub const NAME_VERSION: &str = concat!(
    "Un-Banished More ",
    env!("CARGO_PKG_VERSION"),
    "-",
    env!("UNBANISHED_BUILD")
);
/// For example `Un-Banished 0.5.0-1382712`: a build without the `more` feature.
#[cfg(not(feature = "more"))]
pub const NAME_VERSION: &str = concat!(
    "Un-Banished ",
    env!("CARGO_PKG_VERSION"),
    "-",
    env!("UNBANISHED_BUILD")
);
