//! Finds the player's own copy of Banished: the Steam installs on this computer (native Steam,
//! and on macOS and Linux the Windows Steam in CrossOver bottles and Wine prefixes), or a folder
//! the player names. Nothing is copied: the game reads `WinData` where it is.
//!
//! Each Steam folder lists its libraries in `steamapps/libraryfolders.vdf` ([`vdf`]); Banished
//! is `steamapps/common/Banished/WinData` in one of them. `UNBANISHED_DIR` names a Banished
//! folder directly, as `tools/extract-assets.sh` reads it.
//!
//! Deviation: not in the original, which reads `WinData` beside its own executable only
//! (`FileSystem::GetRootDirectory`, see `paths`).
#![cfg(not(target_arch = "wasm32"))]

pub mod vdf;

use std::path::{Path, PathBuf};

/// The package every copy of Banished has.
const MARKER: &str = "data0.pkg";

/// One copy of Banished found on this computer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Install {
    /// Where it was found, for the player: "Steam", "Steam (CrossOver: Steam)".
    pub label: String,
    /// Its `WinData` folder.
    pub win_data: PathBuf,
}

/// Every copy of Banished found, `UNBANISHED_DIR` first, then each Steam in [`steam_roots`]
/// order. A folder found twice is listed once.
pub fn installs() -> Vec<Install> {
    let mut found: Vec<Install> = Vec::new();
    let mut add = |label: String, win_data: PathBuf| {
        let same = |i: &Install| same_folder(&i.win_data, &win_data);
        if !found.iter().any(same) {
            found.push(Install { label, win_data });
        }
    };
    if let Some(dir) = unbanished_formats::env::var_os("DIR") {
        if let Some(w) = resolve_folder(Path::new(&dir)) {
            add("UNBANISHED_DIR".into(), w);
        }
    }
    for (label, root) in steam_roots() {
        for w in in_steam(&root) {
            add(label.clone(), w);
        }
    }
    found
}

/// The `WinData` folders of Banished in one Steam folder's libraries.
pub fn in_steam(root: &Path) -> Vec<PathBuf> {
    let mut libraries = vec![root.to_path_buf()];
    let vdf = root.join("steamapps").join("libraryfolders.vdf");
    if let Ok(text) = std::fs::read_to_string(&vdf) {
        for p in vdf::library_paths(&text) {
            libraries.push(library_path(root, &p));
        }
    }
    libraries
        .iter()
        .map(|l| l.join("steamapps/common/Banished/WinData"))
        .filter(|w| w.join(MARKER).is_file())
        .fold(Vec::new(), |mut v: Vec<PathBuf>, w| {
            if !v.iter().any(|x| same_folder(x, &w)) {
                v.push(w);
            }
            v
        })
}

/// A folder the player chose: the Banished folder, its `WinData`, or any folder holding the
/// packages. `None` when no `data0.pkg` is there.
pub fn resolve_folder(dir: &Path) -> Option<PathBuf> {
    [dir.to_path_buf(), dir.join("WinData")]
        .into_iter()
        .find(|d| d.join(MARKER).is_file())
}

/// A library path from a Windows Steam's vdf, inside a bottle: `C:\Games\Steam` becomes
/// `ROOT/../../../C/Games/Steam` under the bottle's `drive_c`. A native path is kept.
fn library_path(root: &Path, p: &str) -> PathBuf {
    let b = p.as_bytes();
    let windows = b.len() >= 2 && b[1] == b':' && b[0].is_ascii_alphabetic();
    if windows && !cfg!(windows) {
        // root is …/drive_c/Program Files (x86)/Steam, so the bottle is two levels above drive_c.
        let drive = format!("drive_{}", (b[0] as char).to_ascii_lowercase());
        if let Some(bottle) = root
            .ancestors()
            .find(|a| a.ends_with("drive_c"))
            .and_then(Path::parent)
        {
            let rest = p[2..].trim_start_matches(['\\', '/']).replace('\\', "/");
            return bottle.join(drive).join(rest);
        }
    }
    PathBuf::from(p)
}

/// The Steam folders to look in on this system, with a label for each.
pub fn steam_roots() -> Vec<(String, PathBuf)> {
    let home =
        std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from);
    let mut roots: Vec<(String, PathBuf)> = Vec::new();
    if cfg!(windows) {
        for var in ["ProgramFiles(x86)", "ProgramFiles"] {
            if let Some(p) = std::env::var_os(var) {
                roots.push(("Steam".into(), PathBuf::from(p).join("Steam")));
            }
        }
        return roots;
    }
    let Some(home) = home else { return roots };
    if cfg!(target_os = "macos") {
        roots.push((
            "Steam".into(),
            home.join("Library/Application Support/Steam"),
        ));
        let bottles = home.join("Library/Application Support/CrossOver/Bottles");
        for (name, drive_c) in subfolders(&bottles, "drive_c") {
            roots.extend(windows_steams(&format!("CrossOver: {name}"), &drive_c));
        }
    } else {
        for p in [
            ".steam/steam",
            ".local/share/Steam",
            ".var/app/com.valvesoftware.Steam/.local/share/Steam",
        ] {
            roots.push(("Steam".into(), home.join(p)));
        }
    }
    let prefix = std::env::var_os("WINEPREFIX")
        .map(PathBuf::from)
        .unwrap_or(home.join(".wine"));
    roots.extend(windows_steams("Wine", &prefix.join("drive_c")));
    roots
}

/// A Windows Steam's usual folders under a bottle's or prefix's `drive_c`.
fn windows_steams(name: &str, drive_c: &Path) -> Vec<(String, PathBuf)> {
    ["Program Files (x86)/Steam", "Program Files/Steam"]
        .iter()
        .map(|p| (format!("Steam ({name})"), drive_c.join(p)))
        .collect()
}

/// Each folder in `dir` with its `inner` folder, where that exists, by name.
fn subfolders(dir: &Path, inner: &str) -> Vec<(String, PathBuf)> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut v: Vec<(String, PathBuf)> = entries
        .flatten()
        .map(|e| {
            (
                e.file_name().to_string_lossy().into_owned(),
                e.path().join(inner),
            )
        })
        .filter(|(_, p)| p.is_dir())
        .collect();
    v.sort();
    v
}

fn same_folder(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

#[cfg(test)]
#[path = "steam/tests.rs"]
mod tests;
