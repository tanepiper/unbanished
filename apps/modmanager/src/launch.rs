//! Finding the pack directory and starting the game.

use std::path::{Path, PathBuf};

/// The settings key (section and name) where the manager remembers the pack directory:
/// `ModManagerFolder=…`. The game does not read it.
pub const FOLDER_SECTION: &str = "ModManager";
pub const FOLDER_NAME: &str = "Folder";

/// The executable's folder.
fn exe_dir() -> Option<PathBuf> {
    Some(std::env::current_exe().ok()?.parent()?.to_path_buf())
}

/// The pack directory, as the game finds it (`platform::paths::data_dir`): a folder given on
/// the command line, else `WinData` beside the executable; then the one the manager used last;
/// then the repository's copy (development builds).
pub fn pack_dir(args: &[String], remembered: Option<&str>) -> PathBuf {
    if let Some(dir) = args
        .iter()
        .find(|a| !a.starts_with("--") && Path::new(a).is_dir())
    {
        return dir.into();
    }
    if let Some(dir) = exe_dir().map(|r| r.join("WinData")).filter(|d| d.is_dir()) {
        return dir;
    }
    if let Some(dir) = remembered.map(PathBuf::from).filter(|d| d.is_dir()) {
        return dir;
    }
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../game-data/original/WinData")
}

/// The game's executable: `banished` beside this one (a release puts them together), else on
/// the `PATH`.
pub fn game_executable() -> PathBuf {
    let name = if cfg!(windows) {
        "banished.exe"
    } else {
        "banished"
    };
    exe_dir()
        .map(|d| d.join(name))
        .filter(|p| p.is_file())
        .unwrap_or_else(|| PathBuf::from(name))
}

/// Start the game with the pack directory, without waiting for it.
pub fn start_game(dir: &Path) -> std::io::Result<()> {
    std::process::Command::new(game_executable())
        .arg(dir)
        .spawn()
        .map(|_| ())
}

/// Where named mod lists are kept: `Documents/Banished/ModLists` (beside the saves; see the
/// game's `platform::paths::user_dir`, and `BANISHED_USER_DIR`).
pub fn lists_dir() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("BANISHED_USER_DIR") {
        return (!dir.is_empty()).then(|| PathBuf::from(dir).join("ModLists"));
    }
    let home = if cfg!(windows) {
        std::env::var_os("USERPROFILE")
    } else {
        std::env::var_os("HOME")
    }?;
    Some(
        PathBuf::from(home)
            .join("Documents")
            .join("Banished")
            .join("ModLists"),
    )
}
