//! The multiplayer and recording session around a game (not in the original): lockstep turns
//! and their messages, the links to the other players, digests of the game state, and
//! recordings. The game app drives a game with it. See docs/MULTIPLAYER-PLAN.md.

pub mod digest;
pub mod link;
pub mod lockstep;
pub mod recording;
