//! The calendar at Game/Game +0x128 (constructor FUN_14009bfd0, update FUN_14009b850).
use crate::scheduler::{seconds, ticks};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Calendar {
    pub year: u32,
    /// +0x08: integer System::Time, not a sum of f32 frame deltas.
    pub elapsed: i64,
    /// +0x10: Time(3600 seconds), DAT_14017895c.
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
    /// Port of FUN_14009b850's inline calendar update. It subtracts one threshold per iteration,
    /// preserving the original behavior even for an unusually large delta.
    pub fn advance(&mut self, delta: i64) {
        self.elapsed += delta;
        if self.elapsed >= self.year_ticks {
            self.year = self.year.wrapping_add(1);
            self.elapsed -= self.year_ticks;
        }
    }

    pub fn seconds(&self) -> f32 {
        seconds(self.elapsed)
    }
}
