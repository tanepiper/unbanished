//! Rule hooks from WebAssembly plugins (not in the original): mods that change the game at fixed
//! points of the step. Each plugin is a WebAssembly module made for the hook API
//! ([`plugin::API_VERSION`]); they run in the order they were loaded, after every iteration, and
//! behave the same on every computer: no floating point, a fixed budget per call, and the
//! game's own random stream. See docs/develop/RULE-HOOKS.md.
//!
//! Deviation: not in the original, which has no rule hooks.

mod host;
pub mod plugin;

use std::path::Path;

use unbanished_sim::hooks::{HookContext, RuleHooks};

pub use plugin::{Plugin, API_VERSION, FUEL};

/// The plugins of a game, in their order.
#[derive(Default)]
pub struct Plugins {
    pub plugins: Vec<Plugin>,
    /// Their fingerprint (see [`fingerprint`]).
    pub fingerprint: u32,
}

impl Plugins {
    /// Loads `(name, bytes)` plugins in this order.
    pub fn load(files: &[(String, Vec<u8>)]) -> Result<Plugins, String> {
        let plugins = files
            .iter()
            .map(|(name, bytes)| Plugin::load(name, bytes))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Plugins {
            plugins,
            fingerprint: fingerprint(files),
        })
    }

    /// The `.wasm` files of `dir`, in the order of their names (none when the folder does not
    /// exist).
    pub fn files(dir: &Path) -> Vec<(String, Vec<u8>)> {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return Vec::new();
        };
        let mut paths: Vec<_> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "wasm"))
            .collect();
        paths.sort();
        paths
            .into_iter()
            .filter_map(|p| {
                let name = p.file_stem()?.to_string_lossy().into_owned();
                Some((name, std::fs::read(&p).ok()?))
            })
            .collect()
    }

    /// The names of the plugins, in order.
    pub fn names(&self) -> Vec<String> {
        self.plugins.iter().map(|p| p.name.clone()).collect()
    }
}

impl RuleHooks for Plugins {
    fn after_iteration(&mut self, cx: &mut HookContext<'_>) {
        for p in &mut self.plugins {
            p.after_iteration(cx);
        }
    }
}

/// A fingerprint of a set of plugins, in their order: the CRC-32 of each name and its bytes. Two
/// games with the same plugins in the same order have the same fingerprint; with none it is 0.
pub fn fingerprint(files: &[(String, Vec<u8>)]) -> u32 {
    if files.is_empty() {
        return 0;
    }
    let mut c = !0u32;
    for (name, bytes) in files {
        c = unbanished_formats::crc::crc32_raw(c, name.as_bytes());
        c = unbanished_formats::crc::crc32_raw(c, &(bytes.len() as u64).to_le_bytes());
        c = unbanished_formats::crc::crc32_raw(c, bytes);
    }
    !c
}

#[cfg(test)]
mod tests;
