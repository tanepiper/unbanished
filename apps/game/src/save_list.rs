//! The saves on disk (`IO::Persistence`, Runtime) and the list the Save and Load dialogs show
//! (`Save::SaveLoad::UpdateList` `0x1800c4710`). Native builds: the files are in
//! [`crate::paths::save_dir`].
//!
//! `UpdateList` asks `Persistence` for every `*.sav` file, makes one [`SaveInfo`] each and, for
//! the Save dialog (operation argument −2), first a slot for a new save showing the save sprite
//! (`+0x40` −1) and none for the autosave (`GetAutoSaveName` "Auto", which only quick save
//! writes). The files are sorted newest first (`qsort` with `0x1800c6620`: the 64-bit file time
//! at `+0x30`, descending), then each one's metadata is read (`LoadMetaData`). A file that does
//! not read keeps an empty name, status 0 and a black image of the save sprite's size.
#![cfg(not(target_arch = "wasm32"))]

use banished_formats::save::{Save, SavedMod};

/// `Save::SaveLoad::SaveInfo` (0x58 bytes).
#[derive(Debug, Clone)]
pub struct SaveInfo {
    /// `+0x38`: the file's name without `.sav`; `None` for the new-save slot (`+0x40` −1).
    pub file: Option<String>,
    /// `+0x10`: the metadata's name.
    pub name: String,
    /// `+0x30`: the file's write time, as a Windows file time (100 ns since 1601, UTC).
    pub date: u64,
    /// `+0x18`: the preview, `width × height` blue, green, red, alpha texels.
    pub width: i32,
    pub height: i32,
    pub image: Vec<u8>,
    /// `+0x44`: the metadata's extension value; 2 for a save that reads (the dialogs load only
    /// those), 0 for one that does not.
    pub status: i32,
    /// `+0x48`: the mods it was saved with.
    pub mods: Vec<SavedMod>,
}

/// `SaveLoad::GetAutoSaveName`.
pub const AUTO_SAVE: &str = "Auto";

/// Seconds between the Unix and Windows file-time epochs (1970 and 1601).
const EPOCH_DIFFERENCE: u64 = 11_644_473_600;

fn file_time(t: std::time::SystemTime) -> u64 {
    let d = t.duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
    (d.as_secs() + EPOCH_DIFFERENCE) * 10_000_000 + d.subsec_nanos() as u64 / 100
}

/// The saves for the Save dialog (`saving`, with the new-save slot and without the autosave)
/// or the Load dialog.
pub fn list(saving: bool) -> Vec<SaveInfo> {
    let (w, h) = crate::saved::PREVIEW_SIZE;
    let blank = |file: Option<String>, date| SaveInfo {
        file,
        name: String::new(),
        date,
        width: w as i32,
        height: h as i32,
        image: vec![0; (w * h * 4) as usize],
        status: 0,
        mods: Vec::new(),
    };
    let mut files: Vec<(String, u64, std::path::PathBuf)> = crate::paths::save_dir()
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
            let stem = path.file_stem()?.to_str()?.to_string();
            let date = e.metadata().and_then(|m| m.modified()).map_or(0, file_time);
            Some((stem, date, path))
        })
        .filter(|(stem, _, _)| !(saving && stem == AUTO_SAVE))
        .collect();
    files.sort_by(|a, b| b.1.cmp(&a.1));
    let mut out = Vec::new();
    if saving {
        out.push(SaveInfo {
            status: 2,
            ..blank(None, 0)
        });
    }
    for (stem, date, path) in files {
        let metadata = std::fs::read(&path)
            .map_err(|e| banished_formats::Error::Format(e.to_string()))
            .and_then(|b| Save::read_metadata(&b));
        out.push(match metadata {
            Ok(m) => SaveInfo {
                file: Some(stem),
                name: m.name,
                date,
                width: m.width,
                height: m.height,
                image: m.image,
                status: m.value,
                mods: m.mods,
            },
            Err(e) => {
                log::warn!("{}: {e}", path.display());
                blank(Some(stem), date)
            }
        });
    }
    out
}

/// `SaveLoad::GenerateSaveName`: the first `Save%i` no listed save has.
pub fn generate_name(list: &[SaveInfo]) -> String {
    (0..)
        .map(|i| format!("Save{i}"))
        .find(|n| !list.iter().any(|s| s.file.as_deref() == Some(n.as_str())))
        .unwrap()
}

/// `DateTime::AsString` (`0x1800c7d40`): the local date and time, `"%s %s"` of
/// `GetDateFormatW(DATE_SHORTDATE)` and `GetTimeFormatW(TIME_NOSECONDS)`. Deviation: the
/// user's locale formats are not read; the port writes the en-US defaults (`M/d/yyyy h:mm tt`).
pub fn date_string(date: u64) -> String {
    let unix = (date / 10_000_000) as i64 - EPOCH_DIFFERENCE as i64;
    let Ok(t) = jiff::Timestamp::from_second(unix) else {
        return String::new();
    };
    let z = t.to_zoned(jiff::tz::TimeZone::system());
    let hour = z.hour();
    let h12 = if hour % 12 == 0 { 12 } else { hour % 12 };
    let ampm = if hour < 12 { "AM" } else { "PM" };
    format!(
        "{}/{}/{} {}:{:02} {}",
        z.month(),
        z.day(),
        z.year(),
        h12,
        z.minute(),
        ampm
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_saves_take_the_first_free_number() {
        let info = |f: &str| SaveInfo {
            file: Some(f.into()),
            name: String::new(),
            date: 0,
            width: 0,
            height: 0,
            image: Vec::new(),
            status: 2,
            mods: Vec::new(),
        };
        assert_eq!(generate_name(&[]), "Save0");
        assert_eq!(
            generate_name(&[info("Save0"), info("Save2"), info("Save1")]),
            "Save3"
        );
        assert_eq!(generate_name(&[info("Save1")]), "Save0");
    }

    /// 1 January 2000 00:00 UTC as a file time.
    #[test]
    fn file_times_count_from_1601() {
        let t = std::time::UNIX_EPOCH + std::time::Duration::from_secs(946_684_800);
        assert_eq!(file_time(t), 125_911_584_000_000_000);
    }
}
