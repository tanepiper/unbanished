//! The town event log (`Game +0x138`) and the stock warnings that post to it. See
//! docs/EVENT-LOG.md.

use crate::stock::{stock_table, RawMaterial, Storage, STOCK_SLOTS};

/// `DAT_140178910`: a warning waits 120 s before it can repeat.
const WARNING_COOLDOWN: i64 = 120_000_000;

/// Event types: entry `type` of `Dialog/StringTable.rsc:events`.
pub mod kind {
    pub const STARVED: i32 = 0;
    pub const FROZE: i32 = 1;
    pub const OLD_AGE: i32 = 2;
    pub const BORN: i32 = 8;
    pub const ADULT: i32 = 9;
    pub const STUDENT: i32 = 10;
    pub const LOW_LOGS: i32 = 17;
    pub const NEAR_CAPACITY_GOODS: i32 = 23;
    pub const NEAR_CAPACITY_RAW: i32 = 24;
    pub const TORNADO: i32 = 30;
    pub const FIRE: i32 = 31;
}

/// One log entry (0x28 bytes: colour, formatted text, type, severity, cell, category, icon).
/// The port keeps the format arguments; the text is formatted with the string tables when
/// shown, from the month and year the event happened in.
#[derive(Debug, Clone, PartialEq)]
pub struct EventEntry {
    pub kind: i32,
    pub severity: i32,
    /// ARGB from the severity (`FUN_14009a2d0`).
    pub color: u32,
    /// `@0` month (index into `:seasons`), `@1` year.
    pub month: usize,
    pub year: i32,
    /// `@2`, `@3`, `@4`.
    pub args: [String; 3],
    /// `FUN_14009a500`: the entity's map-rectangle centre cell, or -1.
    pub cell: i32,
    /// `FUN_14009a5a0`: notification category.
    pub category: i32,
    /// Popup icon (12, 13 for types 0x1a-0x1d), -1 otherwise.
    pub icon: i32,
}

#[derive(Debug, Clone, Default)]
pub struct EventLog {
    pub entries: Vec<EventEntry>,
    /// Set when an event asks the game to slow down (option `AutoSlow`, types 11, 12, 30, 31).
    pub slow_request: bool,
}

/// `FUN_14009a2d0`'s severity colours.
pub fn severity_color(severity: i32) -> u32 {
    match severity {
        1 => 0xffff_ff00,
        2 => 0xff80_80ff,
        3 => 0xffff_6060,
        4 => 0xffea_7517,
        _ => 0xffff_ffff,
    }
}

/// `FUN_14009a5a0`.
pub fn category(kind: i32) -> i32 {
    match kind {
        0..=7 => 0,
        0xb => 1,
        0xc => 2,
        0xd => 5,
        0xe => 8,
        0x11..=0x16 => 6,
        0x17 | 0x18 => 7,
        0x19 => 9,
        0x1a | 0x1c => 10,
        0x1b | 0x1d => 11,
        0x1e => 4,
        0x1f => 3,
        _ => -1,
    }
}

impl EventLog {
    /// `FUN_14009a2d0(log, type, severity, entity, @2, @3, @4)`.
    #[allow(clippy::too_many_arguments)]
    pub fn add(
        &mut self,
        kind: i32,
        severity: i32,
        cell: i32,
        month: usize,
        year: i32,
        args: [String; 3],
        auto_slow: bool,
    ) {
        let icon = match kind {
            0x1a | 0x1c => 12,
            0x1b | 0x1d => 13,
            _ => -1,
        };
        self.entries.push(EventEntry {
            kind,
            severity,
            color: severity_color(severity),
            month,
            year,
            args,
            cell,
            category: category(kind),
            icon,
        });
        if kind > 10 && (kind < 0xd || (0x1e..=0x1f).contains(&kind)) && auto_slow {
            self.slow_request = true;
        }
    }
}

/// The stock manager's warning state (`Game +0xf0`: slots `+0x170`, capacity records `+0x180`).
#[derive(Debug, Clone)]
pub struct StockWarnings {
    /// Slot `+0x0` count and `+0x4` previous count (`FUN_14008f3a0`).
    pub count: [i32; STOCK_SLOTS],
    pub previous: [i32; STOCK_SLOTS],
    /// Slot `+0x8`: production limit (`FUN_140095f00` defaults; the Town Hall changes them).
    pub limits: [i32; STOCK_SLOTS],
    /// Slot `+0x18`: warning cooldown.
    pub cooldown: [i64; STOCK_SLOTS],
    /// `+0x180` records: produced goods and logs/stone/iron, `(used, capacity, cooldown)`.
    pub capacity: [(i32, i32, i64); 2],
}

impl Default for StockWarnings {
    fn default() -> Self {
        let mut limits = [1000; STOCK_SLOTS];
        limits[..12].copy_from_slice(&[200, 200, 200, 200, 50, 5000, 100, 50, 500, 200, 200, 1000]);
        StockWarnings {
            count: [0; STOCK_SLOTS],
            previous: [0; STOCK_SLOTS],
            limits,
            cooldown: [0; STOCK_SLOTS],
            capacity: [(0, 0, 0); 2],
        }
    }
}

/// `FUN_14008f3a0`'s capacity split: materials with flags `0x1c40` (logs, stone, iron) are raw.
const RAW_FLAGS: u32 = 0x1c40;

impl StockWarnings {
    /// `FUN_14008f3a0`: shift counts to `previous`, recount, and total used/capacity by kind.
    pub fn recompute(&mut self, storages: &[Storage], materials: &[RawMaterial]) {
        self.previous = self.count;
        self.count = stock_table(storages, materials);
        let mut used = [0i32; 2];
        let mut cap = [0i32; 2];
        for s in storages {
            let (mut goods, mut raw) = (false, false);
            for item in &s.items {
                let m = &materials[item.material];
                let size = m.size * item.count;
                if m.flags & RAW_FLAGS == 0 {
                    used[0] += size;
                    goods = true;
                } else {
                    used[1] += size;
                    raw = true;
                }
            }
            if raw {
                cap[1] += s.capacity;
            }
            if goods {
                cap[0] += s.capacity;
            }
        }
        for i in 0..2 {
            self.capacity[i].0 = used[i];
            self.capacity[i].1 = cap[i];
        }
    }

    /// `FUN_1400915d0`: low reserve (types 17..22) when a slot falls below 5% of its limit,
    /// near capacity (23 goods, 24 logs/stone/iron) above 95%, each at most every 120 s.
    /// Returns the event types to post at severity 3.
    pub fn check(&mut self, dt: i64) -> Vec<i32> {
        let mut out = Vec::new();
        for i in 0..STOCK_SLOTS {
            self.cooldown[i] -= dt;
            if self.cooldown[i] >= 1 {
                continue;
            }
            self.cooldown[i] = 0;
            if i >= 6 {
                continue;
            }
            let (mut count, mut previous, mut limit) =
                (self.count[i], self.previous[i], self.limits[i]);
            // Firewood includes coal (slot 9).
            if i == 3 {
                count += self.count[9];
                previous += self.previous[9];
                limit += self.limits[9];
            }
            if count < limit / 20 && limit / 20 <= previous {
                out.push(kind::LOW_LOGS + i as i32);
                self.cooldown[i] = WARNING_COOLDOWN;
            }
        }
        for (i, (used, cap, cooldown)) in self.capacity.iter_mut().enumerate() {
            *cooldown -= dt;
            if *cooldown >= 1 {
                continue;
            }
            *cooldown = 0;
            if (*cap / 20) * 19 < *used {
                out.push(kind::NEAR_CAPACITY_GOODS + i as i32);
                *cooldown = WARNING_COOLDOWN;
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn low_reserve_fires_on_crossing_only() {
        let mut w = StockWarnings::default();
        w.previous[0] = 10;
        w.count[0] = 9;
        assert_eq!(w.check(0), vec![kind::LOW_LOGS]);
        // Cooling down.
        assert!(w.check(1_000_000).is_empty());
        // No crossing: both below the threshold.
        w.cooldown[0] = 0;
        w.previous[0] = 5;
        w.count[0] = 4;
        assert!(w.check(0).is_empty());
    }

    #[test]
    fn colours_and_categories() {
        assert_eq!(severity_color(3), 0xffff6060);
        assert_eq!(category(17), 6);
        assert_eq!(category(23), 7);
    }
}
