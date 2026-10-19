//! SpawnComponent's independent per-band clocks.
//!
//! Original: `FUN_140056430`.
use crate::scheduler::ticks;

/// The spawn state of a resource that makes new resources around it (for example a tree that
/// seeds saplings).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spawn {
    // +0x28/+0x30
    /// Component: one System::Time per SpawnDescription entry, in resource order.
    pub timers: Vec<i64>,
}

impl Spawn {
    /// Newly allocated entry clocks start at zero, without RNG.
    ///
    /// Original: `FUN_1400567f0`, `FUN_140001bb0`.
    pub fn new(count: usize) -> Self {
        Self {
            timers: vec![0; count],
        }
    }

    // +0x28
    /// Entries contain inclusive growth bounds and the CHILD GrowthDescription duration.
    /// Out-of-band clocks retain their previous value. Unlike GrowthComponent's periodic timer,
    /// this consumes intervals at equality.
    ///
    /// Original: `FUN_140056430`.
    pub fn update(
        &mut self,
        growth: f32,
        delta: i64,
        entries: &[(f32, f32, f32)],
    ) -> Vec<(usize, usize)> {
        assert_eq!(self.timers.len(), entries.len());
        let mut attempts = Vec::new();
        for (index, (&(lo, hi, duration), timer)) in
            entries.iter().zip(&mut self.timers).enumerate()
        {
            if !(lo <= growth && growth <= hi) {
                continue;
            }
            *timer += delta;
            let interval = ticks(duration * 300.0); // DAT_140178934
                                                    // Deviation: invalid zero/negative resource periods must not hang the browser.
            if interval <= 0 {
                continue;
            }
            let mut count = 0;
            while interval <= *timer {
                *timer -= interval;
                count += 1;
            }
            if count != 0 {
                attempts.push((index, count));
            }
        }
        attempts
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clocks_freeze_outside_band_and_fire_at_exact_interval() {
        let entries = [(0.1, 1.1, 1.0), (1.1, 3.0, 0.5)];
        let mut spawn = Spawn::new(2);
        assert!(spawn.update(0.5, 200_000_000, &entries).is_empty());
        assert_eq!(spawn.timers, [200_000_000, 0]);
        assert!(spawn.update(4.0, 999_999_999, &entries).is_empty());
        assert_eq!(spawn.timers, [200_000_000, 0]);
        assert_eq!(spawn.update(1.1, 100_000_000, &entries), [(0, 1)]);
        assert_eq!(spawn.timers, [0, 100_000_000]);
        assert_eq!(spawn.update(1.1, 650_000_000, &entries), [(0, 2), (1, 5)]);
        assert_eq!(spawn.timers, [50_000_000, 0]);
    }
}
