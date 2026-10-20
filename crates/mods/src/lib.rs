//! The mod list Un-Banished shares between the game and the mod manager: the `.pkm` packages in the
//! pack directory, which are enabled and in what order, and their conflicts. It is the port of
//! `IO::Modification` (Runtime), moved out of the game so that both read and write the same
//! settings in the same way.
//!
//! - **The list** (`RefreshList`): every `*.pkm` the pack directory holds
//!   (`FileSystem::FindFiles`), read with `PackageFile::ReadPackfile`: its name, author,
//!   description, icon, preview and files. A mod is known by its file's name without the extension;
//!   the settings keep whether it is enabled (`ModEnabled_<file>`) and its place in the list
//!   (`ModIndex_<file>`). Each mod lists the others that hold one of its files (its conflicts).
//! - **Saving** (`SaveList`) writes both settings for every mod.
//!
//! Inferred: the list is ordered by the saved index, a mod without one after the others in the
//! directory's order.
//!
//! [`text`] is a settings store for the settings file, for tools that run without the game.
//!
//! Original: Runtime `0x1800b0d40`, Runtime `0x1800b2e50`.
#![doc(
    html_logo_url = "https://tanepiper.github.io/unbanished/assets/un-banished-icon.svg",
    html_favicon_url = "https://tanepiper.github.io/unbanished/favicon.svg"
)]

// The conflicts list is at Mod +0x70.
use std::collections::BTreeSet;

use unbanished_formats::pkg::{normalise, Package};

pub mod preview;
pub mod text;

/// Where the mod list reads and writes its settings: the game's `Persistence`, or a
/// [`text::TextSettings`]. Names are made from a section and a name (`ModEnabled_` and the
/// file), as `IO::Persistence` does.
pub trait Settings {
    /// Unchanged when absent; `"true"` is true, anything else false.
    fn get_bool(&self, section: &str, name: &str, value: &mut bool);
    /// Unchanged when absent or not a number.
    fn get_int(&self, section: &str, name: &str, value: &mut i32);
    /// Store a bool as `true` or `false`.
    fn set_bool(&mut self, section: &str, name: &str, value: bool);
    /// Store a number.
    fn set_int(&mut self, section: &str, name: &str, value: i32);
}

/// One mod (`Modification::Mod`, 0x80 bytes).
#[derive(Clone)]
pub struct Mod {
    /// The package's file name without `.pkm` (`GetPathFilename`).
    pub file: String,
    /// The name the mod's author gave it.
    pub name: String,
    /// The mod's author.
    pub author: String,
    /// The mod's description.
    pub description: String,
    // +0x18; +0x1c
    /// The game version it was made with and the author's version.
    pub game_version: u32,
    /// The author's own version number.
    pub user_version: u32,
    /// The package's checksum, which saves record.
    pub crc: u32,
    /// The 48×48 icon (`0xAARRGGBB`).
    pub icon: (u32, u32, Vec<u8>),
    // +0x60
    /// Its files, normalised, and as the package names them, in that order.
    pub files: BTreeSet<String>,
    /// Its files as the package names them, sorted, for the details.
    pub names: Vec<String>,
    // +0x50; +0x20
    /// Whether it is enabled.
    pub enabled: bool,
    /// Its place in the list.
    pub index: i32,
    // +0x70
    /// The other mods sharing a file, by file name.
    pub conflicts: Vec<String>,
    /// The package's bytes.
    pub bytes: std::rc::Rc<Vec<u8>>,
}

impl Mod {
    /// `+0x58`: where it is, under the data directory (`WinData\<file>.pkm`). Deviation: the
    /// original shows the full path.
    pub fn path(&self) -> String {
        format!("WinData\\{}.pkm", self.file)
    }
}

/// The mod list: every package found, in list order.
#[derive(Default, Clone)]
pub struct Modification {
    /// The mods, the first winning over the others when they share a file.
    pub mods: Vec<Mod>,
    /// `+0`: the list changed (Enable, Swap) since the packages were set up.
    pub dirty: bool,
}

impl Modification {
    /// `RefreshList` over the packages found: `(file name without .pkm, bytes)`.
    pub fn refresh(found: Vec<(String, Vec<u8>)>, settings: &impl Settings) -> Modification {
        let mut mods = Vec::new();
        for (order, (file, bytes)) in found.into_iter().enumerate() {
            let package = match Package::parse(bytes.clone()) {
                Ok(p) => p,
                Err(e) => {
                    log::warn!("{file}.pkm: {e}");
                    continue;
                }
            };
            let h = &package.header;
            if h.skip != 0 {
                continue;
            }
            let mut enabled = false;
            settings.get_bool("ModEnabled_", &file, &mut enabled);
            let mut index = i32::MAX / 2 + order as i32;
            settings.get_int("ModIndex_", &file, &mut index);
            mods.push(Mod {
                name: h.name.clone(),
                author: h.author.clone(),
                description: h.description.clone(),
                game_version: h.game_version,
                user_version: h.user_version,
                crc: h.crc,
                icon: h.icon.clone(),
                files: package.entries.iter().map(|e| normalise(&e.name)).collect(),
                names: {
                    let mut v: Vec<String> =
                        package.entries.iter().map(|e| e.name.clone()).collect();
                    v.sort_by_key(|n| normalise(n));
                    v.dedup_by_key(|n| normalise(n));
                    v
                },
                enabled,
                index,
                conflicts: Vec::new(),
                bytes: std::rc::Rc::new(bytes),
                file,
            });
        }
        let mut list = Modification { mods, dirty: false };
        list.order();
        list
    }

    /// By the saved index, renumbered, with each mod's conflicts.
    fn order(&mut self) {
        self.mods.sort_by_key(|m| m.index);
        self.renumber();
        self.find_conflicts();
    }

    /// Mods added while the game runs (dropped in the browser, or installed from Nexus Mods):
    /// the list is read again with them, replacing any of the same name. They take effect
    /// after a reload; the list is marked changed when an enabled mod was replaced or the
    /// enabled set changed.
    pub fn add(&mut self, found: Vec<(String, Vec<u8>)>, settings: &impl Settings) {
        if found.is_empty() {
            return;
        }
        let names: Vec<String> = found.iter().map(|(n, _)| n.clone()).collect();
        let mut all: Vec<(String, Vec<u8>)> = self
            .mods
            .iter()
            .filter(|m| !names.contains(&m.file))
            .map(|m| (m.file.clone(), m.bytes.as_ref().clone()))
            .collect();
        all.extend(found);
        let before = self.enabled_files();
        let dirty = self.dirty
            || self
                .mods
                .iter()
                .any(|m| m.enabled && names.contains(&m.file));
        *self = Modification::refresh(all, settings);
        self.dirty = dirty || self.enabled_files() != before;
    }

    /// `RefreshList` over the packages already read: the settings' order and enabled mods
    /// again (the Mods dialog's Cancel). The list stays dirty.
    pub fn revert(&mut self, settings: &impl Settings) {
        for (order, m) in self.mods.iter_mut().enumerate() {
            m.enabled = false;
            settings.get_bool("ModEnabled_", &m.file, &mut m.enabled);
            m.index = i32::MAX / 2 + order as i32;
            settings.get_int("ModIndex_", &m.file, &mut m.index);
        }
        self.order();
    }

    fn renumber(&mut self) {
        for (i, m) in self.mods.iter_mut().enumerate() {
            m.index = i as i32;
        }
    }

    /// Each mod's conflicts: the other mods with a file in common.
    fn find_conflicts(&mut self) {
        let n = self.mods.len();
        for i in 0..n {
            let conflicts = (0..n)
                .filter(|&j| j != i && !self.mods[i].files.is_disjoint(&self.mods[j].files))
                .map(|j| self.mods[j].file.clone())
                .collect();
            self.mods[i].conflicts = conflicts;
        }
    }

    /// `SaveList`.
    pub fn save_list(&self, settings: &mut impl Settings) {
        for m in &self.mods {
            settings.set_bool("ModEnabled_", &m.file, m.enabled);
            settings.set_int("ModIndex_", &m.file, m.index);
        }
    }

    /// `Enable`.
    pub fn enable(&mut self, i: usize, on: bool) {
        self.dirty = true;
        if let Some(m) = self.mods.get_mut(i) {
            m.enabled = on;
        }
    }

    /// `Swap`: two places in the list.
    pub fn swap(&mut self, a: usize, b: usize) {
        if a != b && a < self.mods.len() && b < self.mods.len() {
            self.dirty = true;
            self.mods.swap(a, b);
            self.renumber();
        }
    }

    /// Whether mod `i` is enabled with another enabled mod sharing a file.
    pub fn in_conflict(&self, i: usize) -> bool {
        let Some(m) = self.mods.get(i).filter(|m| m.enabled) else {
            return false;
        };
        m.conflicts
            .iter()
            .any(|f| self.mods.iter().any(|o| o.enabled && &o.file == f))
    }

    /// The enabled mods in the order `SetupFileSystem` adds them: from the last in the list to
    /// the first.
    pub fn load_order(&self) -> Vec<&Mod> {
        self.mods.iter().rev().filter(|m| m.enabled).collect()
    }

    // +0x2c; +0x18; +0x1c
    /// `SaveLoad::SaveMetaData`: each enabled mod as the save records it, its name, its file under
    /// the data directory (`WinData\<file>.pkm`, inferred from `GetPackDirectory`) and its crc,
    /// game version and author's version.
    pub fn saved(&self) -> Vec<unbanished_formats::save::SavedMod> {
        self.mods
            .iter()
            .filter(|m| m.enabled)
            .map(|m| unbanished_formats::save::SavedMod {
                name: m.name.clone(),
                file: m.path(),
                values: [m.crc, m.game_version, m.user_version],
            })
            .collect()
    }

    /// 0 when the saved mod is installed as it was, 1 when it is not installed (by its file,
    /// without case), 4 when its crc differs, 3 when the author's version differs and 2 when it was
    /// made before game version 1.0.4.
    ///
    /// Original: `SaveLoad::CheckMod`, Runtime `0x1800c3900`.
    pub fn check(&self, saved: &unbanished_formats::save::SavedMod) -> u32 {
        let stem = saved
            .file
            .rsplit(['\\', '/'])
            .next()
            .unwrap_or("")
            .trim_end_matches(".pkm")
            .trim_end_matches(".PKM");
        let Some(m) = self.mods.iter().find(|m| m.file.eq_ignore_ascii_case(stem)) else {
            return 1;
        };
        let [crc, game_version, user_version] = saved.values;
        if m.crc != crc {
            4
        } else if m.user_version != user_version {
            3
        } else if game_version < 0x0100_0400 {
            2
        } else {
            0
        }
    }

    /// The enabled mods' file names, in list order (their resource lists load in this order).
    pub fn enabled_files(&self) -> Vec<String> {
        self.mods
            .iter()
            .filter(|m| m.enabled)
            .map(|m| m.file.clone())
            .collect()
    }
}

/// `FileSystem::FindFiles(GetPackDirectory(), "*.pkm")`: the packages in the folder.
#[cfg(not(target_arch = "wasm32"))]
pub fn find(dir: &std::path::Path) -> Vec<(String, Vec<u8>)> {
    let Ok(read) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut paths: Vec<std::path::PathBuf> = read
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("pkm")))
        .collect();
    paths.sort();
    paths
        .into_iter()
        .filter_map(|p| {
            let stem = p.file_stem()?.to_str()?.to_string();
            match std::fs::read(&p) {
                Ok(b) => Some((stem, b)),
                Err(e) => {
                    log::warn!("{}: {e}", p.display());
                    None
                }
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pkm(name: &str) -> Option<(String, Vec<u8>)> {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let b = std::fs::read(root.join("game-data/mods/downloads").join(name)).ok()?;
        Some((name.trim_end_matches(".pkm").to_string(), b))
    }

    /// The settings decide what is enabled and the order; the list saves back the same way.
    #[test]
    fn the_list_follows_the_settings() {
        let (Some(a), Some(b)) = (pkm("apiary.pkm"), pkm("StorageCart.pkm")) else {
            return;
        };
        let mut s = text::TextSettings::default();
        s.set_bool("ModEnabled_", "apiary", true);
        s.set_int("ModIndex_", "apiary", 1);
        s.set_int("ModIndex_", "StorageCart", 0);
        let mut list = Modification::refresh(vec![a, b], &s);
        assert_eq!(list.mods[0].file, "StorageCart");
        assert_eq!(list.mods[1].name, "Sample - Apiary");
        assert!(list.mods[1].enabled && !list.mods[0].enabled);
        assert_eq!(list.enabled_files(), vec!["apiary".to_string()]);
        list.swap(0, 1);
        let mut saved = text::TextSettings::default();
        list.save_list(&mut saved);
        let mut i = -1;
        saved.get_int("ModIndex_", "apiary", &mut i);
        assert_eq!(i, 0);
    }
}
