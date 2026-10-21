//! The game's string tables as the simulation asks for them (`unbanished_sim::strings`).

use unbanished_ui::Resources;

/// The interface's string tables, lent to the simulation.
pub struct UiStrings<'a>(pub &'a mut Resources);

impl unbanished_sim::strings::Strings for UiStrings<'_> {
    fn string(&mut self, table: Option<&str>, key: &str) -> String {
        self.0.string(table, key)
    }

    fn string_at(&mut self, table: &str, index: i32) -> String {
        self.0.string_at(table, index)
    }
}
