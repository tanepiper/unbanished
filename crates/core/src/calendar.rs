//! The game's calendar: the year, and the time since the year began.
//!
//! A year is 3,600 seconds of game time, with twelve months of 300 seconds.
//!
//! ```
//! use banished_core::calendar::Calendar;
//! use banished_core::scheduler::ticks;
//!
//! let mut calendar = Calendar::default();
//! calendar.advance(ticks(3600.0));
//! assert_eq!(calendar.year, 2);
//! ```
//!
//! Original: `FUN_14009bfd0` (set-up), `FUN_14009b850` (update).
// The calendar object is at Game/Game +0x128.
use crate::scheduler::{seconds, ticks};

#[derive(Debug, Clone, PartialEq, Eq)]
/// The year, and the game time since it began.
pub struct Calendar {
    /// The year, from 1.
    pub year: u32,
    // +0x08
    /// The game time since the year began, in ticks. It is integer time, not a sum of rounded
    /// float steps.
    pub elapsed: i64,
    // +0x10; DAT_14017895c
    /// The length of a year: 3,600 seconds.
    pub year_ticks: i64,
}

impl Default for Calendar {
    fn default() -> Self {
        Self {
            year: 1,
            elapsed: 0,
            year_ticks: ticks(3600.0),
        }
    }
}

impl Calendar {
    /// Add `delta` ticks. When a year is complete, the year goes up by one. Like the original, it
    /// subtracts at most one year per call, even for a very large `delta`.
    ///
    /// Original: `FUN_14009b850` (inline calendar update).
    pub fn advance(&mut self, delta: i64) {
        self.elapsed += delta;
        if self.elapsed >= self.year_ticks {
            self.year = self.year.wrapping_add(1);
            self.elapsed -= self.year_ticks;
        }
    }

    /// The time since the year began, in seconds.
    pub fn seconds(&self) -> f32 {
        seconds(self.elapsed)
    }
}
