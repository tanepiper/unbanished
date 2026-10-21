//! The door for rule hooks (not in the original): code from a mod that changes the game at a
//! fixed point of the step. The simulation calls its [`RuleHooks`] after every iteration, with a
//! [`HookContext`]: a narrow view of the town with what a hook may read and do.
//!
//! A rule hook changes the game's results, so it must behave the same on every computer: it gets
//! integers only, draws from the game's own random stream in a fixed order, and runs in the order
//! the hooks were declared. With no hooks (the default, and always in Pure Banished) the step is
//! exactly the original's. `unbanished-hooks` runs hooks from WebAssembly plugins.
//!
//! Deviation: not in the original, which has no rule hooks; a game with hooks plays differently.

use unbanished_game::calendar::Calendar;
use unbanished_game::events::EventLog;
use unbanished_game::random::Random;

/// Code that changes the game at fixed points of the step, in a fixed order.
pub trait RuleHooks: Send {
    /// After each iteration of the step (the town, the calendar, the weather, the stock).
    fn after_iteration(&mut self, cx: &mut HookContext<'_>);
}

/// What a rule hook may read and do. Every value is an integer.
pub struct HookContext<'a> {
    calendar: &'a Calendar,
    month: usize,
    citizens: i32,
    stock: &'a [i32],
    rng: &'a mut Random,
    events: &'a mut EventLog,
}

/// The event-log type of a hook's lines: the original's "other" category (achievements), with
/// no popup and no AutoSlow.
const HOOK_EVENT: i32 = 0x19;

impl<'a> HookContext<'a> {
    /// A hook's view of a game: its calendar and month, its living citizens, its stock slots,
    /// its random stream and its event log.
    pub fn new(
        calendar: &'a Calendar,
        month: usize,
        citizens: i32,
        stock: &'a [i32],
        rng: &'a mut Random,
        events: &'a mut EventLog,
    ) -> HookContext<'a> {
        HookContext {
            calendar,
            month,
            citizens,
            stock,
            rng,
            events,
        }
    }

    /// The year, from 1.
    pub fn year(&self) -> i32 {
        self.calendar.year as i32
    }

    /// The month, 0 to 11.
    pub fn month(&self) -> i32 {
        self.month as i32
    }

    /// The time into the year, in game ticks (microseconds).
    pub fn time_in_year(&self) -> i64 {
        self.calendar.elapsed
    }

    /// The citizens alive.
    pub fn citizens(&self) -> i32 {
        self.citizens
    }

    /// The stock of slot `slot` (the Town Hall's stock slots: food, fuel, clothing…), or -1 for
    /// a slot that does not exist.
    pub fn stock(&self, slot: i32) -> i32 {
        usize::try_from(slot)
            .ok()
            .and_then(|s| self.stock.get(s))
            .copied()
            .unwrap_or(-1)
    }

    /// How many stock slots there are.
    pub fn stock_slots(&self) -> i32 {
        self.stock.len() as i32
    }

    /// A random number below `n` from the game's own stream (0 for `n` below 1). It moves the
    /// game's stream on, as every computer does at the same point.
    pub fn random_below(&mut self, n: i32) -> i32 {
        if n < 1 {
            return 0;
        }
        self.rng.rand_below(n as u32) as i32
    }

    /// Swaps the game's random stream with `other` (a plugin draws from it during a call, then
    /// it is swapped back).
    pub fn swap_random(&mut self, other: &mut Random) {
        std::mem::swap(self.rng, other);
    }

    /// Posts `text` to the event log. `severity` 0 to 4 colours the line as the original's
    /// events of that severity: white, yellow, blue, red, orange.
    pub fn post_event(&mut self, severity: i32, text: &str) {
        let (month, year) = (self.month, self.year());
        self.events.add(
            HOOK_EVENT,
            severity.clamp(0, 4),
            -1,
            month,
            year,
            Default::default(),
            false,
        );
        if let Some(entry) = self.events.entries.last_mut() {
            entry.text = Some(text.to_string());
        }
    }
}

/// The folder a save's rule-hook entries name: `Hooks/NAME.wasm`.
const SAVED_FOLDER: &str = "Hooks/";

/// A save's entry for a rule hook, among the mods the save lists: its file and its CRC-32. The
/// original game reports it as a missing mod, which it is: the town was played with it.
pub fn saved_entry(name: &str, bytes: &[u8]) -> unbanished_formats::save::SavedMod {
    unbanished_formats::save::SavedMod {
        name: format!("Rule hook: {name}"),
        file: format!("{SAVED_FOLDER}{name}.wasm"),
        values: [unbanished_formats::crc::crc32(bytes.iter().copied()), 0, 0],
    }
}

/// Whether a save's mod entry is a rule hook's ([`saved_entry`]).
pub fn is_saved_hook(m: &unbanished_formats::save::SavedMod) -> bool {
    m.file.starts_with(SAVED_FOLDER)
}
