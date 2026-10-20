//! The settings file as `name=value` lines, for tools that run without the game (the mod
//! manager). It keeps every value it reads, so writing the file back changes only what was
//! set. The game reads the same file through its own `Persistence`.

use std::collections::BTreeMap;
#[cfg(not(target_arch = "wasm32"))]
use std::path::PathBuf;

use crate::Settings;

/// The settings as names and values, in name order (as the game writes them).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TextSettings {
    values: BTreeMap<String, String>,
}

impl TextSettings {
    /// Read `name=value` lines; other lines are ignored.
    pub fn parse(text: &str) -> TextSettings {
        let mut s = TextSettings::default();
        for line in text.lines() {
            if let Some((k, v)) = line.split_once('=') {
                s.values.insert(k.trim().to_string(), v.trim().to_string());
            }
        }
        s
    }

    /// The text of the file, one `name=value` line per value.
    pub fn serialize(&self) -> String {
        self.values
            .iter()
            .map(|(k, v)| format!("{k}={v}\n"))
            .collect()
    }

    /// A text value, or `None` when absent or empty.
    pub fn get_text(&self, section: &str, name: &str) -> Option<&str> {
        self.values
            .get(&format!("{section}{name}"))
            .map(String::as_str)
            .filter(|v| !v.is_empty())
    }

    /// Store a text value.
    pub fn set_text(&mut self, section: &str, name: &str, value: &str) {
        self.values
            .insert(format!("{section}{name}"), value.to_string());
    }

    /// Every value whose name starts with `prefix`, as (rest of the name, value).
    pub fn with_prefix<'a>(&'a self, prefix: &'a str) -> impl Iterator<Item = (&'a str, &'a str)> {
        self.values
            .range(prefix.to_string()..)
            .take_while(move |(k, _)| k.starts_with(prefix))
            .map(move |(k, v)| (&k[prefix.len()..], v.as_str()))
    }

    /// Read the file at `path`; an absent file is empty settings.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn load(path: &std::path::Path) -> std::io::Result<TextSettings> {
        match std::fs::read_to_string(path) {
            Ok(text) => Ok(TextSettings::parse(&text)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(TextSettings::default()),
            Err(e) => Err(e),
        }
    }

    /// Write the file at `path`, through a temporary file so that a failed write leaves the
    /// old file as it was.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn save(&self, path: &std::path::Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let temp = path.with_extension("txt.new");
        std::fs::write(&temp, self.serialize())?;
        std::fs::rename(&temp, path)
    }
}

impl Settings for TextSettings {
    fn get_bool(&self, section: &str, name: &str, value: &mut bool) {
        if let Some(v) = self.values.get(&format!("{section}{name}")) {
            *value = v == "true";
        }
    }

    fn get_int(&self, section: &str, name: &str, value: &mut i32) {
        if let Some(n) = self
            .values
            .get(&format!("{section}{name}"))
            .and_then(|v| v.parse().ok())
        {
            *value = n;
        }
    }

    fn set_bool(&mut self, section: &str, name: &str, value: bool) {
        self.set_text(section, name, if value { "true" } else { "false" });
    }

    fn set_int(&mut self, section: &str, name: &str, value: i32) {
        self.set_text(section, name, &value.to_string());
    }
}

/// The game's settings file: `UNBANISHED_SETTINGS` if set, else `settings.txt` in the
/// `banished-wasm` folder of the platform's configuration folder, as the game reads it.
#[cfg(not(target_arch = "wasm32"))]
pub fn settings_path() -> Option<PathBuf> {
    if let Ok(p) = unbanished_formats::env::var("SETTINGS") {
        return (!p.is_empty()).then(|| PathBuf::from(p));
    }
    let base = if cfg!(target_os = "windows") {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support"))
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
    }?;
    Some(base.join("banished-wasm").join("settings.txt"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_what_it_does_not_set() {
        let mut s = TextSettings::parse("VideoVSync=true\nModEnabled_apiary=false\n");
        s.set_bool("ModEnabled_", "apiary", true);
        s.set_int("ModIndex_", "apiary", 2);
        assert_eq!(
            s.serialize(),
            "ModEnabled_apiary=true\nModIndex_apiary=2\nVideoVSync=true\n"
        );
        let mut on = false;
        s.get_bool("ModEnabled_", "apiary", &mut on);
        assert!(on);
        let found: Vec<_> = s.with_prefix("Mod").collect();
        assert_eq!(found.len(), 2);
    }
}
