//! Where the game's files live, as the original keeps them, so the port can stand in for the
//! executable (native builds).
//!
//! - **Game data**: `FileSystem::Startup` (Runtime `0x1800acba0`) reads the `.pkg` files from
//!   the root directory (`FileSystem::GetRootDirectory` `0x1800ae5a0`: the executable's folder)
//!   joined with the platform and build names (`Configuration::GetPlatformName` "Win",
//!   `GetBuildName` "Data"): `WinData` beside the executable.
//! - **Saves**: `FileSystem::GetSaveDirectory` (`0x1800adf20`) is the Documents folder
//!   (`SHGetFolderPathW` `CSIDL_PERSONAL`) joined with `Banished\`; saves are
//!   `%sSave\%s.sav` in it.
//!
//! Deviation: Documents is `%USERPROFILE%\Documents` on Windows (a redirected Documents folder
//! is not followed) and `~/Documents` elsewhere. `BANISHED_USER_DIR` names the `Banished` folder
//! directly (tests, captures).
#![cfg(not(target_arch = "wasm32"))]

use std::path::{Path, PathBuf};

/// The executable's folder (`FileSystem::GetRootDirectory`).
pub fn root_dir() -> Option<PathBuf> {
    Some(std::env::current_exe().ok()?.parent()?.to_path_buf())
}

/// The game data: a folder given on the command line (`args` without the program), else `WinData` beside the executable,
/// else the repository's extracted copy (development builds).
pub fn data_dir(args: &[String]) -> PathBuf {
    if let Some(dir) = args
        .iter()
        .find(|a| !a.starts_with("--") && Path::new(a).is_dir())
    {
        return dir.into();
    }
    if let Some(dir) = root_dir().map(|r| r.join("WinData")).filter(|d| d.is_dir()) {
        return dir;
    }
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../game-data/original/WinData")
}

/// `FileSystem::GetSaveDirectory`: `Documents/Banished`.
pub fn user_dir() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("BANISHED_USER_DIR") {
        return (!dir.is_empty()).then(|| PathBuf::from(dir));
    }
    let home = if cfg!(target_os = "windows") {
        std::env::var_os("USERPROFILE")
    } else {
        std::env::var_os("HOME")
    }?;
    Some(PathBuf::from(home).join("Documents").join("Banished"))
}

/// The saves' folder (`%sSave\`).
pub fn save_dir() -> Option<PathBuf> {
    Some(user_dir()?.join("Save"))
}

/// A save's file (`%sSave\%s.sav`).
pub fn save_file(name: &str) -> Option<PathBuf> {
    Some(save_dir()?.join(format!("{name}.sav")))
}
