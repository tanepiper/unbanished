//! The game loop's doors to its extensions: a multiplayer game's state ([`Online`], the
//! `multiplayer` feature) and a recording's ([`Recording`], the `replay` feature). The game
//! loop asks these few methods and nothing else; without a feature its state is empty and the
//! answers are a single player's.

#[cfg(feature = "multiplayer")]
#[path = "extensions/online.rs"]
mod online;
#[cfg(not(feature = "multiplayer"))]
#[path = "extensions/online_off.rs"]
mod online;
#[cfg(feature = "replay")]
#[path = "extensions/recording.rs"]
mod recording;
#[cfg(not(feature = "replay"))]
#[path = "extensions/recording_off.rs"]
mod recording;

pub use online::Online;
pub use recording::Recording;
