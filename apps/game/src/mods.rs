//! The mods (`IO::Modification`, Runtime): `.pkm` packages in the pack directory (`WinData`).
//!
//! - **The list** (`RefreshList`, `0x1800b0d40`): every `*.pkm` the pack directory holds
//!   (`FileSystem::FindFiles`), read with `PackageFile::ReadPackfile`: its name, author,
//!   description, icon, preview and files. A mod is known by its file's name without the
//!   extension; the settings keep whether it is enabled (`ModEnabled_<file>`) and its place in
//!   the list (`ModIndex_<file>`). Each mod lists the others that hold one of its files (its
//!   conflicts, `+0x70`).
//! - **Saving** (`SaveList`, `0x1800b2e50`) writes both settings for every mod.
//! - **Loading** (`SetupFileSystem`, `0x1800b02d0`): the enabled mods join the game's packages
//!   from the last in the list to the first (`Archive::add_mod`), then each one's
//!   `<file>Resources.rsc:resource` is loaded (`formats::registry`).
//!
//! Inferred: the list is ordered by the saved index, a mod without one after the others in the
//! directory's order.

use std::collections::BTreeSet;

use banished_formats::pkg::{normalise, Package};
use banished_formats::registry::Registry;

use crate::settings::Persistence;

/// One mod (`Modification::Mod`, 0x80 bytes).
#[derive(Clone)]
pub struct Mod {
    /// The package's file name without `.pkm` (`GetPathFilename`).
    pub file: String,
    pub name: String,
    pub author: String,
    pub description: String,
    /// The game version it was made with (`+0x18`) and the author's version (`+0x1c`).
    pub game_version: u32,
    pub user_version: u32,
    pub crc: u32,
    /// The 48×48 icon (`0xAARRGGBB`).
    pub icon: (u32, u32, Vec<u8>),
    /// Its files, normalised (`+0x60`), and as the package names them, in that order.
    pub files: BTreeSet<String>,
    pub names: Vec<String>,
    /// `+0x50` and `+0x20`.
    pub enabled: bool,
    pub index: i32,
    /// The other mods sharing a file (`+0x70`), by file name.
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

#[derive(Default, Clone)]
pub struct Modification {
    pub mods: Vec<Mod>,
    /// `+0`: the list changed (Enable, Swap) since the packages were set up.
    pub dirty: bool,
}

impl Modification {
    /// `RefreshList` over the packages found: `(file name without .pkm, bytes)`.
    pub fn refresh(found: Vec<(String, Vec<u8>)>, settings: &Persistence) -> Modification {
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

    /// `RefreshList` over the packages already read: the settings' order and enabled mods
    /// again (the Mods dialog's Cancel). The list stays dirty.
    pub fn revert(&mut self, settings: &Persistence) {
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
    pub fn save_list(&self, settings: &mut Persistence) {
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

    /// `SaveLoad::SaveMetaData`: each enabled mod as the save records it, its name, its file
    /// under the data directory (`WinData\<file>.pkm`, inferred from `GetPackDirectory`) and
    /// its crc (`+0x2c`), game version (`+0x18`) and author's version (`+0x1c`).
    pub fn saved(&self) -> Vec<banished_formats::save::SavedMod> {
        self.mods
            .iter()
            .filter(|m| m.enabled)
            .map(|m| banished_formats::save::SavedMod {
                name: m.name.clone(),
                file: m.path(),
                values: [m.crc, m.game_version, m.user_version],
            })
            .collect()
    }

    /// `SaveLoad::CheckMod` (Runtime `0x1800c3900`): 0 when the saved mod is installed as it
    /// was, 1 when it is not installed (by its file, without case), 4 when its crc differs, 3
    /// when the author's version differs and 2 when it was made before game version 1.0.4.
    pub fn check(&self, saved: &banished_formats::save::SavedMod) -> u32 {
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

/// What a reload starts once the packages are set up again (`FUN_140072110`).
pub enum Reload {
    /// The main menu (`FUN_1400b9c60`).
    Menu,
    /// The game saved to memory just before (`Save::SavedState`, `FUN_1400b9ec0`).
    Game(Vec<u8>),
}

thread_local! {
    /// `Object::ExternalFactory::_singleton`'s loaded set: the registry the game last loaded.
    static REGISTRY: std::cell::RefCell<Option<std::rc::Rc<Registry>>> = const { std::cell::RefCell::new(None) };
}

/// The game's loaded resources (set when the packages load).
pub fn set_registry(r: std::rc::Rc<Registry>) {
    REGISTRY.with(|c| *c.borrow_mut() = Some(r));
}

/// `ExternalFactory::FindNext` over `class`: the loaded resources of that type, or `None`
/// before any game loaded.
pub fn loaded_of_type(class: &str) -> Option<Vec<String>> {
    REGISTRY.with(|c| {
        c.borrow().as_ref().map(|r| {
            r.of_type(banished_formats::crc::type_id(class))
                .into_iter()
                .map(str::to_string)
                .collect()
        })
    })
}

/// How many raw materials are loaded (`FindNext` over RawMaterialDescription, `FUN_1400bdcf0`).
pub fn raw_material_count(archive: &banished_formats::pkg::Archive) -> usize {
    loaded_of_type("RawMaterialDescription").map_or_else(
        || {
            archive
                .names()
                .into_iter()
                .filter(|n| n.ends_with("_rawmaterial.crs"))
                .count()
        },
        |v| v.len(),
    )
}

/// The loaded templates (ComponentDescription `:resource` objects, `FUN_140059fd0`): the game's
/// in `Game/GameResources.rsc`'s order (inferred for the factory's), then the mods' others.
pub fn templates(archive: &banished_formats::pkg::Archive) -> Vec<String> {
    let mut list: Vec<String> = archive
        .read_ref("Game/GameResources.rsc:resource")
        .ok()
        .and_then(|d| banished_formats::game::external_list(&d).ok())
        .unwrap_or_default();
    if let Some(loaded) = loaded_of_type("ComponentDescription") {
        let known: std::collections::BTreeSet<String> = list.iter().map(|r| normalise(r)).collect();
        list.extend(
            loaded
                .into_iter()
                .filter(|r| !known.contains(&normalise(r))),
        );
    }
    list
}

/// `FUN_1400ae570`: the loaded professions that are shown (`+0x4e`), by their sort priority
/// (`+0x48`); before any game loaded, `Profession.rsc:professionList`.
pub fn profession_list(archive: &banished_formats::pkg::Archive) -> Vec<String> {
    let read = |r: &str| {
        archive
            .read_ref(&r.replace('\\', "/"))
            .ok()
            .and_then(|d| banished_formats::game::profession(&d).ok())
    };
    match loaded_of_type("Profession") {
        Some(refs) => {
            let mut v: Vec<(i32, String)> = refs
                .into_iter()
                .filter_map(|r| {
                    let p = read(&r)?;
                    p.flag_4e.then_some((p.value_48, r))
                })
                .collect();
            v.sort_by_key(|(order, _)| *order);
            v.into_iter().map(|(_, r)| r).collect()
        }
        None => archive
            .read_ref("Game/Profession/Profession.rsc:professionList")
            .ok()
            .and_then(|d| banished_formats::game::external_list(&d).ok())
            .unwrap_or_default(),
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
        let mut s = Persistence::default();
        s.set_bool("ModEnabled_", "apiary", true);
        s.set_int("ModIndex_", "apiary", 1);
        s.set_int("ModIndex_", "StorageCart", 0);
        let mut list = Modification::refresh(vec![a, b], &s);
        assert_eq!(list.mods[0].file, "StorageCart");
        assert_eq!(list.mods[1].name, "Sample - Apiary");
        assert!(list.mods[1].enabled && !list.mods[0].enabled);
        assert_eq!(list.enabled_files(), vec!["apiary".to_string()]);
        list.swap(0, 1);
        let mut saved = Persistence::default();
        list.save_list(&mut saved);
        let mut i = -1;
        saved.get_int("ModIndex_", "apiary", &mut i);
        assert_eq!(i, 0);
    }
}
