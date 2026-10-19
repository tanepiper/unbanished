//! Path finding, ported bit for bit from the original game:
//!
//! - [`pathfind`]: the A* pathfinder over the cell grid (`FUN_14001d760` and its cost tables)
//!   and the walkable regions (`FUN_1400a1950`);
//! - [`sectors`]: the sector graph the citizens' searches rank distances on.
//!
//! `banished-game` re-exports both modules.

pub mod pathfind;
pub mod sectors;
