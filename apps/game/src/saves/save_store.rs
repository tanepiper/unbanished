//! Where saves are kept (`IO::Persistence`'s files, Runtime).
//!
//! Native builds use the original's folder (`crate::platform::paths::save_dir`, `NAME.sav`). In the
//! browser the saves live in IndexedDB (database `banished-wasm`): store `files` holds each
//! whole `.sav` by name, store `meta` its write time and metadata block, so listing reads no
//! game state. The metadata is mirrored in memory when the page starts (`init`), so the list is
//! at hand as the original's is after its listing operation; writes and deletes update the
//! mirror at once and the database in the background; a load is a request polled each frame.
//!
//! Store `mods` holds the mods (`.pkm`) the player added, by file name without the extension
//! (`stored_mods`, `store_mod`).
//!
//! Deviation: the browser keeps saves and mods in its own storage, not in folders.

use unbanished_formats::save::SaveMetadata;

/// A stored save: its name (the file name without `.sav`), its write time as a Windows file
/// time (100 ns since 1601) and its metadata (`None` when it does not read).
#[derive(Debug, Clone)]
pub struct Entry {
    pub name: String,
    pub date: u64,
    pub metadata: Option<SaveMetadata>,
}

/// Seconds between the Windows and Unix file-time epochs (1601 and 1970).
pub const EPOCH_DIFFERENCE: u64 = 11_644_473_600;

/// A load in progress: `poll` gives the file's bytes once read.
pub struct Read(imp::Read);

impl Read {
    pub fn poll(&mut self) -> Option<Result<Vec<u8>, String>> {
        self.0.poll()
    }
}

pub use imp::{delete, list, read, write};
#[cfg(target_arch = "wasm32")]
pub use imp::{
    init, kept_package_names, store_mod, store_packages, stored_mods, stored_packages, write_then,
};

#[cfg(not(target_arch = "wasm32"))]
mod native;
#[cfg(not(target_arch = "wasm32"))]
use native as imp;

#[cfg(target_arch = "wasm32")]
mod web;
#[cfg(target_arch = "wasm32")]
use web as imp;
