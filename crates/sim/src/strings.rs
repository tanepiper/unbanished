//! The game's text as the simulation needs it. Some of what the town writes is text the player
//! reads in their own language: the event log keeps each entry's text, and the saves keep it
//! too. The simulation asks for that text through [`Strings`]; the game app answers from its
//! string tables.

use unbanished_game::events::EventEntry;

/// Looks up the game's strings.
pub trait Strings {
    /// The string `key` of the string table `table` (the default table when `None`).
    fn string(&mut self, table: Option<&str>, key: &str) -> String;
    /// The string at `index` in the string table `table`.
    fn string_at(&mut self, table: &str, index: i32) -> String;
}

/// An event's text: its own text if it has one, else its kind's line of
/// `Dialog/StringTable.rsc:events` with `@0` the season, `@1` the year and `@2`.. its
/// arguments: `FormatIndexed(events[type], season, year, @2, @3, @4)`.
///
/// Original: `FUN_14009a2d0`.
pub fn format_event(strings: &mut dyn Strings, e: &EventEntry) -> String {
    if let Some(text) = &e.text {
        return text.clone();
    }
    let template = strings.string_at("Dialog/StringTable.rsc:events", e.kind);
    let season = strings.string_at("Dialog/StringTable.rsc:seasons", e.month as i32);
    let mut out = template;
    for (i, v) in [season, e.year.to_string()]
        .into_iter()
        .chain(e.args.iter().cloned())
        .enumerate()
    {
        out = out.replace(&format!("@{i}"), &v);
    }
    out
}

/// The game's string tables, read from the archive as they are first wanted: the answers the
/// interface gives, for a simulation with no interface (a server).
pub struct ArchiveStrings<'a> {
    archive: &'a unbanished_formats::pkg::Archive,
    tables: std::collections::HashMap<String, Option<Vec<(String, String)>>>,
}

impl<'a> ArchiveStrings<'a> {
    /// The string tables of `archive`.
    pub fn new(archive: &'a unbanished_formats::pkg::Archive) -> ArchiveStrings<'a> {
        ArchiveStrings {
            archive,
            tables: Default::default(),
        }
    }

    fn table(&mut self, table: &str) -> Option<&Vec<(String, String)>> {
        let archive = self.archive;
        self.tables
            .entry(table.to_string())
            .or_insert_with(|| {
                archive
                    .read_ref(table)
                    .and_then(|d| unbanished_formats::ui::string_table(&d))
                    .ok()
            })
            .as_ref()
    }
}

impl Strings for ArchiveStrings<'_> {
    fn string(&mut self, table: Option<&str>, key: &str) -> String {
        let Some(table) = table else {
            return key.to_string();
        };
        // `StringTable::GetText(name)`: a name the table lacks is an empty string; without a
        // table the name is the text.
        match self.table(table) {
            // A name the table holds twice gives its last text, as the interface's map does.
            Some(t) => t
                .iter()
                .rev()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v.clone())
                .unwrap_or_default(),
            None => key.to_string(),
        }
    }

    fn string_at(&mut self, table: &str, index: i32) -> String {
        let Some(t) = self.table(table) else {
            return String::new();
        };
        usize::try_from(index)
            .ok()
            .and_then(|i| t.get(i))
            .map(|(_, v)| v.clone())
            .unwrap_or_default()
    }
}
