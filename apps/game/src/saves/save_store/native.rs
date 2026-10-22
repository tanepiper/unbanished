//! The saves on a computer: files in the save folder.

use super::{Entry, EPOCH_DIFFERENCE};
use unbanished_formats::save::Save;

fn file_time(t: std::time::SystemTime) -> u64 {
    let d = t.duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
    (d.as_secs() + EPOCH_DIFFERENCE) * 10_000_000 + d.subsec_nanos() as u64 / 100
}

/// `Persistence::BeginGetSaveGameList`: every `*.sav` in the folder.
pub fn list() -> Vec<Entry> {
    crate::platform::paths::save_dir()
        .and_then(|d| std::fs::read_dir(d).ok())
        .into_iter()
        .flatten()
        .filter_map(|e| {
            let e = e.ok()?;
            let path = e.path();
            if !path
                .extension()
                .is_some_and(|x| x.eq_ignore_ascii_case("sav"))
            {
                return None;
            }
            let name = path.file_stem()?.to_str()?.to_string();
            let date = e.metadata().and_then(|m| m.modified()).map_or(0, file_time);
            let metadata = std::fs::read(&path)
                .map_err(|e| unbanished_formats::Error::Format(e.to_string()))
                .and_then(|b| Save::read_metadata(&b))
                .map_err(|e| log::warn!("{}: {e}", path.display()))
                .ok();
            Some(Entry {
                name,
                date,
                metadata,
            })
        })
        .collect()
}

pub fn write(name: &str, bytes: &[u8]) -> Result<(), String> {
    let path = crate::platform::paths::save_file(name).ok_or("no save folder")?;
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    std::fs::write(&path, bytes).map_err(|e| format!("{}: {e}", path.display()))
}

pub fn delete(name: &str) {
    if let Some(path) = crate::platform::paths::save_file(name) {
        if let Err(e) = std::fs::remove_file(&path) {
            log::error!("{}: {e}", path.display());
        }
    }
}

pub struct Read(Option<Result<Vec<u8>, String>>);

impl Read {
    pub fn poll(&mut self) -> Option<Result<Vec<u8>, String>> {
        self.0.take()
    }
}

pub fn read(name: &str) -> super::Read {
    let result = crate::platform::paths::save_file(name)
        .ok_or_else(|| "no save folder".to_string())
        .and_then(|p| std::fs::read(&p).map_err(|e| format!("{}: {e}", p.display())));
    super::Read(Read(Some(result)))
}
