//! The player's rule hooks (not in the original): WebAssembly plugins from the `Hooks` folder of
//! the player's data (`Documents/Banished/Hooks`), loaded into every game in the order of their
//! names (`unbanished-hooks`). Pure Banished loads none, and the browser has no such folder.
//!
//! Deviation: not in the original, which has no rule hooks.

/// The plugins of the `Hooks` folder: name and bytes, in the order of their names.
pub fn files() -> Vec<(String, Vec<u8>)> {
    #[cfg(all(feature = "rule-hooks", not(target_arch = "wasm32")))]
    if !crate::platform::mode::pure() {
        if let Some(dir) = crate::platform::paths::user_dir() {
            return unbanished_hooks::Plugins::files(&dir.join("Hooks"));
        }
    }
    Vec::new()
}

/// Their fingerprint, which multiplayer compares (0: none).
#[cfg_attr(not(feature = "multiplayer"), allow(dead_code))]
pub fn fingerprint() -> u32 {
    #[cfg(feature = "rule-hooks")]
    {
        unbanished_hooks::fingerprint(&files())
    }
    #[cfg(not(feature = "rule-hooks"))]
    {
        0
    }
}

/// Their entries among the mods a save lists.
pub fn saved_entries() -> Vec<unbanished_formats::save::SavedMod> {
    files()
        .iter()
        .map(|(name, bytes)| unbanished_sim::hooks::saved_entry(name, bytes))
        .collect()
}

/// Loads them into `sim`. A plugin that does not load leaves the game without hooks, and says
/// why in the log.
pub fn install(sim: &mut unbanished_sim::simulation::Simulation) {
    #[cfg(feature = "rule-hooks")]
    {
        let files = files();
        if files.is_empty() {
            return;
        }
        match unbanished_hooks::Plugins::load(&files) {
            Ok(plugins) => {
                log::info!("rule hooks: {}", plugins.names().join(", "));
                sim.hooks = Some(Box::new(plugins));
            }
            Err(e) => log::error!("rule hooks: {e}; the game runs without them"),
        }
    }
    #[cfg(not(feature = "rule-hooks"))]
    let _ = sim;
}
