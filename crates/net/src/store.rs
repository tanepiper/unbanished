//! Where a server keeps its rooms' towns between games, when its settings name a folder: the last
//! save each room's host sent, by room code. With a kept town, a host who opens the room again
//! goes on from it.

use std::collections::BTreeMap;
use std::path::PathBuf;

/// A room's kept town: the save, and the update it was made at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Kept {
    /// The update the save was made at.
    pub update: u64,
    /// The save file's bytes.
    pub bytes: Vec<u8>,
}

/// Where rooms' towns are kept.
pub trait Store: Send {
    /// The town kept for room `code`, if there is one.
    fn load(&self, code: &str) -> Option<Kept>;
    /// Keeps `kept` for room `code`, replacing the one before.
    fn keep(&mut self, code: &str, kept: Kept);
}

/// Whether a room code can name a kept town: 1 to 16 letters and digits. A code is chosen by a
/// player, so nothing else may become a file name.
pub fn keepable(code: &str) -> bool {
    (1..=16).contains(&code.len()) && code.bytes().all(|b| b.is_ascii_alphanumeric())
}

/// Towns kept in memory (tests).
#[derive(Debug, Default)]
pub struct MemoryStore(BTreeMap<String, Kept>);

impl Store for MemoryStore {
    fn load(&self, code: &str) -> Option<Kept> {
        self.0.get(code).cloned()
    }

    fn keep(&mut self, code: &str, kept: Kept) {
        self.0.insert(code.to_string(), kept);
    }
}

/// Towns kept in a folder: `CODE.sav` and `CODE.update` for each room.
#[derive(Debug)]
pub struct FolderStore(pub PathBuf);

impl Store for FolderStore {
    fn load(&self, code: &str) -> Option<Kept> {
        if !keepable(code) {
            return None;
        }
        let bytes = std::fs::read(self.0.join(format!("{code}.sav"))).ok()?;
        let update = std::fs::read_to_string(self.0.join(format!("{code}.update")))
            .ok()?
            .trim()
            .parse()
            .ok()?;
        Some(Kept { update, bytes })
    }

    fn keep(&mut self, code: &str, kept: Kept) {
        if !keepable(code) {
            return;
        }
        // Written beside, then renamed over: a crash never leaves half a town.
        let write = |name: String, bytes: &[u8]| -> std::io::Result<()> {
            let path = self.0.join(&name);
            let part = self.0.join(format!("{name}.part"));
            std::fs::write(&part, bytes)?;
            std::fs::rename(part, path)
        };
        let result = std::fs::create_dir_all(&self.0)
            .and_then(|_| write(format!("{code}.sav"), &kept.bytes))
            .and_then(|_| write(format!("{code}.update"), kept.update.to_string().as_bytes()));
        if let Err(e) = result {
            log::error!("keeping room {code} in {}: {e}", self.0.display());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_letters_and_digits_name_a_kept_town() {
        assert!(keepable("ABCDEF"));
        assert!(keepable("room42"));
        for bad in ["", "../etc", "a/b", "a.b", "x".repeat(17).as_str(), "ÅÄÖ"] {
            assert!(!keepable(bad), "{bad}");
        }
    }

    #[test]
    fn a_folder_keeps_a_town_and_gives_it_back() {
        let dir = std::env::temp_dir().join(format!("unbanished-store-{}", std::process::id()));
        let mut store = FolderStore(dir.clone());
        assert_eq!(store.load("ROOM"), None);
        let kept = Kept {
            update: 1200,
            bytes: vec![1, 2, 3],
        };
        store.keep("ROOM", kept.clone());
        assert_eq!(store.load("ROOM"), Some(kept));
        store.keep(
            "../ROOM",
            Kept {
                update: 1,
                bytes: Vec::new(),
            },
        );
        assert!(!dir.parent().unwrap().join("ROOM.sav").exists());
        let _ = std::fs::remove_dir_all(dir);
    }
}
