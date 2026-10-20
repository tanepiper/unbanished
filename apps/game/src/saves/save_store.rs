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
mod imp {
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
}

#[cfg(target_arch = "wasm32")]
mod imp {
    use std::cell::RefCell;
    use std::rc::Rc;

    use unbanished_formats::save::{Save, SaveMetadata};
    use wasm_bindgen::prelude::*;
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;
    use web_sys::{IdbDatabase, IdbRequest, IdbTransactionMode};

    use super::{Entry, EPOCH_DIFFERENCE};

    const DATABASE: &str = "banished-wasm";
    const FILES: &str = "files";
    const META: &str = "meta";
    const MODS: &str = "mods";
    /// The game files (`.pkg`) the player chose, kept so they need not be chosen again (#17).
    const GAME: &str = "game";
    /// Version 2 added the mods store.
    const VERSION: u32 = 3;

    thread_local! {
        static DB: RefCell<Option<IdbDatabase>> = const { RefCell::new(None) };
        static MIRROR: RefCell<Vec<Entry>> = const { RefCell::new(Vec::new()) };
    }

    /// An IndexedDB request as a future of its result.
    async fn done(req: &IdbRequest) -> Result<JsValue, JsValue> {
        let r = req.clone();
        let promise = js_sys::Promise::new(&mut |resolve, reject| {
            let ok_req = r.clone();
            let ok = Closure::once(move |_: web_sys::Event| {
                let _ = resolve.call1(&JsValue::NULL, &ok_req.result().unwrap_or(JsValue::NULL));
            });
            let err = Closure::once(move |_: web_sys::Event| {
                let _ = reject.call1(&JsValue::NULL, &JsValue::from_str("request failed"));
            });
            r.set_onsuccess(Some(ok.as_ref().unchecked_ref()));
            r.set_onerror(Some(err.as_ref().unchecked_ref()));
            ok.forget();
            err.forget();
        });
        JsFuture::from(promise).await
    }

    /// A future that ends when `tx` has committed (`complete`), or fails when it aborts or
    /// fails. A request's `success` comes before the commit: a page that reloads then can lose
    /// the write. The handlers are set at once, so call this before the requests' awaits.
    fn committed(tx: &web_sys::IdbTransaction) -> JsFuture {
        let t = tx.clone();
        let promise = js_sys::Promise::new(&mut |resolve, reject| {
            let ok = Closure::once(move |_: web_sys::Event| {
                let _ = resolve.call0(&JsValue::NULL);
            });
            let fail = reject.clone();
            let err = Closure::once(move |_: web_sys::Event| {
                let _ = fail.call1(&JsValue::NULL, &JsValue::from_str("transaction failed"));
            });
            let abort = Closure::once(move |_: web_sys::Event| {
                let _ = reject.call1(&JsValue::NULL, &JsValue::from_str("transaction aborted"));
            });
            t.set_oncomplete(Some(ok.as_ref().unchecked_ref()));
            t.set_onerror(Some(err.as_ref().unchecked_ref()));
            t.set_onabort(Some(abort.as_ref().unchecked_ref()));
            ok.forget();
            err.forget();
            abort.forget();
        });
        JsFuture::from(promise)
    }

    async fn db() -> Result<IdbDatabase, JsValue> {
        if let Some(db) = DB.with(|d| d.borrow().clone()) {
            return Ok(db);
        }
        let factory = web_sys::window()
            .and_then(|w| w.indexed_db().ok().flatten())
            .ok_or_else(|| JsValue::from_str("no IndexedDB"))?;
        let open = factory.open_with_u32(DATABASE, VERSION)?;
        let upgrade_req = open.clone();
        let upgrade = Closure::once(move |_: web_sys::Event| {
            if let Ok(db) = upgrade_req
                .result()
                .and_then(|r| r.dyn_into::<IdbDatabase>())
            {
                for store in [FILES, META, MODS, GAME] {
                    if !db.object_store_names().contains(store) {
                        let _ = db.create_object_store(store);
                    }
                }
            }
        });
        open.set_onupgradeneeded(Some(upgrade.as_ref().unchecked_ref()));
        upgrade.forget();
        let db: IdbDatabase = done(&open).await?.dyn_into()?;
        DB.with(|d| *d.borrow_mut() = Some(db.clone()));
        Ok(db)
    }

    /// A `meta` record: the write time (milliseconds since 1970, f64) then the metadata
    /// block.
    fn meta_record(ms: f64, metadata: &[u8]) -> js_sys::Uint8Array {
        let mut v = ms.to_le_bytes().to_vec();
        v.extend_from_slice(metadata);
        js_sys::Uint8Array::from(v.as_slice())
    }

    fn file_time(ms: f64) -> u64 {
        (ms.max(0.0) * 10_000.0) as u64 + EPOCH_DIFFERENCE * 10_000_000
    }

    /// Open the database and mirror every save's metadata.
    pub fn init() {
        wasm_bindgen_futures::spawn_local(async {
            let result = async {
                let db = db().await?;
                let tx = db.transaction_with_str(META)?;
                let store = tx.object_store(META)?;
                let keys: js_sys::Array = done(&store.get_all_keys()?).await?.dyn_into()?;
                let values: js_sys::Array = done(&store.get_all()?).await?.dyn_into()?;
                let entries: Vec<Entry> = keys
                    .iter()
                    .zip(values.iter())
                    .filter_map(|(k, v)| {
                        let name = k.as_string()?;
                        let bytes = v.dyn_into::<js_sys::Uint8Array>().ok()?.to_vec();
                        let ms = f64::from_le_bytes(bytes.get(..8)?.try_into().ok()?);
                        let metadata = SaveMetadata::parse(&bytes[8..]).ok();
                        Some(Entry {
                            name,
                            date: file_time(ms),
                            metadata,
                        })
                    })
                    .collect();
                log::info!("{} saves in browser storage", entries.len());
                MIRROR.with(|m| *m.borrow_mut() = entries);
                Ok::<_, JsValue>(())
            }
            .await;
            if let Err(e) = result {
                log::error!("save storage: {e:?}");
            }
        });
    }

    /// Every stored mod, `(file name without .pkm, bytes)` by name, handed to `then` (empty
    /// when the storage cannot be read).
    pub fn stored_mods(then: impl FnOnce(Vec<(String, Vec<u8>)>) + 'static) {
        wasm_bindgen_futures::spawn_local(async move {
            let result = async {
                let db = db().await?;
                let tx = db.transaction_with_str(MODS)?;
                let store = tx.object_store(MODS)?;
                let keys: js_sys::Array = done(&store.get_all_keys()?).await?.dyn_into()?;
                let values: js_sys::Array = done(&store.get_all()?).await?.dyn_into()?;
                let mods: Vec<(String, Vec<u8>)> = keys
                    .iter()
                    .zip(values.iter())
                    .filter_map(|(k, v)| {
                        Some((
                            k.as_string()?,
                            v.dyn_into::<js_sys::Uint8Array>().ok()?.to_vec(),
                        ))
                    })
                    .collect();
                Ok::<_, JsValue>(mods)
            }
            .await;
            then(result.unwrap_or_else(|e| {
                log::error!("mod storage: {e:?}");
                Vec::new()
            }));
        });
    }

    /// Keep the chosen game files (`.pkg`) for the next visit, in place of any kept before.
    /// The browser's own file objects are stored, so the data is not copied into the game's
    /// memory; the browser is asked to keep them even when space runs low.
    pub fn store_packages(files: Vec<web_sys::File>) {
        wasm_bindgen_futures::spawn_local(async move {
            let result = async {
                let db = db().await?;
                let tx = db.transaction_with_str_and_mode(GAME, IdbTransactionMode::Readwrite)?;
                let store = tx.object_store(GAME)?;
                done(&store.clear()?).await?;
                for file in &files {
                    done(&store.put_with_key(file, &JsValue::from_str(&file.name()))?).await?;
                }
                Ok::<_, JsValue>(())
            }
            .await;
            match result {
                Ok(()) => log::info!("kept {} game files in browser storage", files.len()),
                Err(e) => log::error!("keeping the game files: {e:?}"),
            }
            if let Some(storage) = web_sys::window().map(|w| w.navigator().storage()) {
                if let Ok(promise) = storage.persist() {
                    let _ = JsFuture::from(promise).await;
                }
            }
        });
    }

    /// The kept game files, `(file name, bytes)`, handed to `then` (empty when none are kept
    /// or they cannot be read).
    pub fn stored_packages(then: impl FnOnce(Vec<(String, Vec<u8>)>) + 'static) {
        wasm_bindgen_futures::spawn_local(async move {
            let result = async {
                let db = db().await?;
                let tx = db.transaction_with_str(GAME)?;
                let store = tx.object_store(GAME)?;
                let keys: js_sys::Array = done(&store.get_all_keys()?).await?.dyn_into()?;
                let values: js_sys::Array = done(&store.get_all()?).await?.dyn_into()?;
                let mut packages = Vec::new();
                for (k, v) in keys.iter().zip(values.iter()) {
                    let (Some(name), Ok(blob)) = (k.as_string(), v.dyn_into::<web_sys::Blob>())
                    else {
                        continue;
                    };
                    let buf = JsFuture::from(blob.array_buffer()).await?;
                    packages.push((name, js_sys::Uint8Array::new(&buf).to_vec()));
                }
                Ok::<_, JsValue>(packages)
            }
            .await;
            then(result.unwrap_or_else(|e| {
                log::error!("kept game files: {e:?}");
                Vec::new()
            }));
        });
    }

    /// Keep a save as `write` does, and finish when it is stored (the start page's import).
    pub async fn write_then(name: &str, bytes: &[u8]) -> Result<(), String> {
        let ms = js_sys::Date::now();
        let save = Save::parse(bytes).map_err(|e| e.to_string())?;
        save.metadata().map_err(|e| e.to_string())?;
        let (file, meta) = (
            js_sys::Uint8Array::from(bytes),
            meta_record(ms, &save.metadata),
        );
        let result = async {
            let db = db().await?;
            let stores = js_sys::Array::of2(&FILES.into(), &META.into());
            let tx =
                db.transaction_with_str_sequence_and_mode(&stores, IdbTransactionMode::Readwrite)?;
            let key = JsValue::from_str(name);
            let finished = committed(&tx);
            done(&tx.object_store(FILES)?.put_with_key(&file, &key)?).await?;
            done(&tx.object_store(META)?.put_with_key(&meta, &key)?).await?;
            // The page reloads after an import: only return once the write is on disk.
            finished.await?;
            Ok::<_, JsValue>(())
        }
        .await;
        result.map_err(|e| format!("{e:?}"))
    }

    /// The names of the kept game files (only their names are read), handed to `then`.
    pub fn kept_package_names(then: impl FnOnce(Vec<String>) + 'static) {
        wasm_bindgen_futures::spawn_local(async move {
            let result = async {
                let db = db().await?;
                let tx = db.transaction_with_str(GAME)?;
                let keys: js_sys::Array = done(&tx.object_store(GAME)?.get_all_keys()?)
                    .await?
                    .dyn_into()?;
                Ok::<_, JsValue>(keys.iter().filter_map(|k| k.as_string()).collect())
            }
            .await;
            then(result.unwrap_or_default());
        });
    }

    /// Keep a mod (`name` without `.pkm`), replacing one of that name.
    pub fn store_mod(name: &str, bytes: &[u8]) {
        let (name, file) = (name.to_string(), js_sys::Uint8Array::from(bytes));
        wasm_bindgen_futures::spawn_local(async move {
            let result = async {
                let db = db().await?;
                let tx = db.transaction_with_str_and_mode(MODS, IdbTransactionMode::Readwrite)?;
                let key = JsValue::from_str(&name);
                done(&tx.object_store(MODS)?.put_with_key(&file, &key)?).await?;
                Ok::<_, JsValue>(())
            }
            .await;
            match result {
                Ok(()) => log::info!("stored mod {name}"),
                Err(e) => log::error!("storing mod {name}: {e:?}"),
            }
        });
    }

    pub fn list() -> Vec<Entry> {
        MIRROR.with(|m| m.borrow().clone())
    }

    pub fn write(name: &str, bytes: &[u8]) -> Result<(), String> {
        let ms = js_sys::Date::now();
        let save = Save::parse(bytes).map_err(|e| e.to_string())?;
        let metadata = save.metadata().map_err(|e| e.to_string())?;
        let entry = Entry {
            name: name.to_string(),
            date: file_time(ms),
            metadata: Some(metadata),
        };
        MIRROR.with(|m| {
            let mut m = m.borrow_mut();
            m.retain(|e| e.name != name);
            m.push(entry);
        });
        let (name, file) = (name.to_string(), js_sys::Uint8Array::from(bytes));
        let meta = meta_record(ms, &save.metadata);
        wasm_bindgen_futures::spawn_local(async move {
            let result = async {
                let db = db().await?;
                let stores = js_sys::Array::of2(&FILES.into(), &META.into());
                let tx = db.transaction_with_str_sequence_and_mode(
                    &stores,
                    IdbTransactionMode::Readwrite,
                )?;
                let key = JsValue::from_str(&name);
                done(&tx.object_store(FILES)?.put_with_key(&file, &key)?).await?;
                done(&tx.object_store(META)?.put_with_key(&meta, &key)?).await?;
                Ok::<_, JsValue>(())
            }
            .await;
            match result {
                Ok(()) => log::info!("saved {name} to browser storage"),
                Err(e) => log::error!("saving {name}: {e:?}"),
            }
        });
        Ok(())
    }

    pub fn delete(name: &str) {
        MIRROR.with(|m| m.borrow_mut().retain(|e| e.name != name));
        let name = name.to_string();
        wasm_bindgen_futures::spawn_local(async move {
            let result = async {
                let db = db().await?;
                let stores = js_sys::Array::of2(&FILES.into(), &META.into());
                let tx = db.transaction_with_str_sequence_and_mode(
                    &stores,
                    IdbTransactionMode::Readwrite,
                )?;
                let key = JsValue::from_str(&name);
                done(&tx.object_store(FILES)?.delete(&key)?).await?;
                done(&tx.object_store(META)?.delete(&key)?).await?;
                Ok::<_, JsValue>(())
            }
            .await;
            if let Err(e) = result {
                log::error!("deleting {name}: {e:?}");
            }
        });
    }

    pub struct Read(Rc<RefCell<Option<Result<Vec<u8>, String>>>>);

    impl Read {
        pub fn poll(&mut self) -> Option<Result<Vec<u8>, String>> {
            self.0.borrow_mut().take()
        }
    }

    pub fn read(name: &str) -> super::Read {
        let slot = Rc::new(RefCell::new(None));
        let out = slot.clone();
        let name = name.to_string();
        wasm_bindgen_futures::spawn_local(async move {
            let result = async {
                let db = db().await?;
                let tx = db.transaction_with_str(FILES)?;
                let value = done(&tx.object_store(FILES)?.get(&JsValue::from_str(&name))?).await?;
                let bytes: js_sys::Uint8Array = value.dyn_into()?;
                Ok::<_, JsValue>(bytes.to_vec())
            }
            .await
            .map_err(|e| format!("{name}: {e:?}"));
            *out.borrow_mut() = Some(result);
        });
        super::Read(Read(slot))
    }
}
