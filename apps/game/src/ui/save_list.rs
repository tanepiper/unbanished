//! The saves on disk (`IO::Persistence`, Runtime) and the list the Save and Load dialogs show
//! (`Save::SaveLoad::UpdateList` `0x1800c4710`), from [`crate::saves::save_store`].
//!
//! `UpdateList` asks `Persistence` for every `*.sav` file, makes one [`SaveInfo`] each and, for
//! the Save dialog (operation argument −2), first a slot for a new save showing the save sprite
//! (`+0x40` −1) and none for the autosave (`GetAutoSaveName` "Auto", which only quick save
//! writes). The files are sorted newest first (`qsort` with `0x1800c6620`: the 64-bit file time
//! at `+0x30`, descending), then each one's metadata is read (`LoadMetaData`). A file that does
//! not read keeps an empty name, status 0 and a black image of the save sprite's size.
use banished_formats::save::SavedMod;

use crate::saves::save_store::{self, EPOCH_DIFFERENCE};

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

/// The saves for the Save dialog (`saving`, with the new-save slot and without the autosave)
/// or the Load dialog.
pub fn list(saving: bool) -> Vec<SaveInfo> {
    let (w, h) = crate::saves::saved::PREVIEW_SIZE;
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
    let mut files: Vec<save_store::Entry> = save_store::list()
        .into_iter()
        .filter(|e| !(saving && e.name == AUTO_SAVE))
        .collect();
    files.sort_by(|a, b| b.date.cmp(&a.date));
    let mut out = Vec::new();
    if saving {
        out.push(SaveInfo {
            status: 2,
            ..blank(None, 0)
        });
    }
    for e in files {
        out.push(match e.metadata {
            Some(m) => SaveInfo {
                file: Some(e.name),
                name: m.name,
                date: e.date,
                width: m.width,
                height: m.height,
                image: m.image,
                status: m.value,
                mods: m.mods,
            },
            None => blank(Some(e.name), e.date),
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
/// `GetDateFormatW(DATE_SHORTDATE)` and `GetTimeFormatW(TIME_NOSECONDS)`, in the user's
/// locale. The browser formats them by its locale (`toLocaleDateString`, `toLocaleTimeString`
/// without seconds). Deviation: native builds do not read the locale's formats; they write the
/// en-US defaults (`M/d/yyyy h:mm tt`).
#[cfg(target_arch = "wasm32")]
pub fn date_string(date: u64) -> String {
    use wasm_bindgen::JsValue;
    let unix = (date / 10_000_000) as i64 - EPOCH_DIFFERENCE as i64;
    let d = js_sys::Date::new(&JsValue::from_f64(unix as f64 * 1000.0));
    // The page's language (`navigator.language`), else the browser's default.
    let locale = js_sys::Reflect::get(&js_sys::global(), &"navigator".into())
        .and_then(|n| js_sys::Reflect::get(&n, &"language".into()))
        .ok()
        .and_then(|l| l.as_string())
        .unwrap_or_else(|| "en-US".into());
    let time = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&time, &"hour".into(), &"numeric".into());
    let _ = js_sys::Reflect::set(&time, &"minute".into(), &"2-digit".into());
    format!(
        "{} {}",
        String::from(d.to_locale_date_string(&locale, &JsValue::UNDEFINED)),
        String::from(d.to_locale_time_string_with_options(&locale, &time)),
    )
}

/// See the browser's `date_string`.
#[cfg(not(target_arch = "wasm32"))]
pub fn date_string(date: u64) -> String {
    let unix = (date / 10_000_000) as i64 - EPOCH_DIFFERENCE as i64;
    let Some((year, month, day, hour, minute)) = local_time(unix) else {
        return String::new();
    };
    let h12 = if hour % 12 == 0 { 12 } else { hour % 12 };
    let ampm = if hour < 12 { "AM" } else { "PM" };
    format!("{month}/{day}/{year} {h12}:{minute:02} {ampm}")
}

/// Seconds since 1970 as the local year, month, day, hour and minute.
#[cfg(not(target_arch = "wasm32"))]
fn local_time(unix: i64) -> Option<(i32, i32, i32, i32, i32)> {
    let t = jiff::Timestamp::from_second(unix).ok()?;
    let z = t.to_zoned(jiff::tz::TimeZone::system());
    Some((
        z.year() as i32,
        z.month() as i32,
        z.day() as i32,
        z.hour() as i32,
        z.minute() as i32,
    ))
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
}
