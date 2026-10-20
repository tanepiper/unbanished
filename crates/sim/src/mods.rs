//! The mods: the list (`unbanished_mods::Modification`, shared with the mod manager), how a reload
//! restarts the game, and what the loaded packages contain.
//!
//! - **Loading** (`SetupFileSystem`): the enabled mods join the game's packages from the last in
//!   the list to the first (`Archive::add_mod`), then each one's `<file>Resources.rsc:resource` is
//!   loaded (`formats::registry`).
//!
//! Original: Runtime `0x1800b02d0`.

use unbanished_formats::pkg::normalise;
use unbanished_formats::registry::Registry;

#[cfg(not(target_arch = "wasm32"))]
pub use unbanished_mods::find;
pub use unbanished_mods::{Mod, Modification};

/// What a reload starts once the packages are set up again.
///
/// Original: `FUN_140072110`.
pub enum Reload {
    /// The main menu.
    ///
    /// Original: `FUN_1400b9c60`.
    Menu,
    /// The game saved to memory just before (`Save::SavedState`).
    ///
    /// Original: `FUN_1400b9ec0`.
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
            r.of_type(unbanished_formats::crc::type_id(class))
                .into_iter()
                .map(str::to_string)
                .collect()
        })
    })
}

/// How many raw materials are loaded (`FindNext` over RawMaterialDescription).
///
/// Original: `FUN_1400bdcf0`.
pub fn raw_material_count(archive: &unbanished_formats::pkg::Archive) -> usize {
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

/// The loaded templates (ComponentDescription `:resource` objects): the game's in
/// `Game/GameResources.rsc`'s order (inferred for the factory's), then the mods' others.
///
/// Original: `FUN_140059fd0`.
pub fn templates(archive: &unbanished_formats::pkg::Archive) -> Vec<String> {
    let mut list: Vec<String> = archive
        .read_ref("Game/GameResources.rsc:resource")
        .ok()
        .and_then(|d| unbanished_formats::game::external_list(&d).ok())
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

// +0x4e; +0x48
/// The loaded professions that are shown, by their sort priority; before any game loaded,
/// `Profession.rsc:professionList`.
///
/// Original: `FUN_1400ae570`.
pub fn profession_list(archive: &unbanished_formats::pkg::Archive) -> Vec<String> {
    let read = |r: &str| {
        archive
            .read_ref(&r.replace('\\', "/"))
            .ok()
            .and_then(|d| unbanished_formats::game::profession(&d).ok())
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
            // `FUN_14001f120`: the CRT qsort by sort priority (`+0x48`).
            unbanished_game::behaviour::ai::crt_qsort_less(&mut v, |a, b| a.0 < b.0);
            v.into_iter().map(|(_, r)| r).collect()
        }
        None => archive
            .read_ref("Game/Profession/Profession.rsc:professionList")
            .ok()
            .and_then(|d| unbanished_formats::game::external_list(&d).ok())
            .unwrap_or_default(),
    }
}
