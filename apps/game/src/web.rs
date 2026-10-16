//! Browser glue: canvas, status panel, and loading the player's `.pkg` files (fetched from the dev
//! server with `?data=` or chosen with the file picker) and mods (`.pkm`, chosen with the picker
//! or dropped on the page at any time, then kept in the browser's storage).
use crate::UserEvent;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use winit::event_loop::EventLoopProxy;

fn document() -> web_sys::Document {
    web_sys::window().unwrap().document().unwrap()
}

pub fn canvas() -> web_sys::HtmlCanvasElement {
    document()
        .get_element_by_id("game")
        .unwrap()
        .dyn_into()
        .unwrap()
}

/// Show loading progress in the HTML panel; hide the panel once the game is running.
pub fn set_status(text: &str, loaded: bool) {
    if let Some(el) = document().get_element_by_id("status") {
        el.set_text_content(Some(text));
    }
    if loaded {
        if let Some(panel) = document().get_element_by_id("panel") {
            let _ = panel.set_attribute("hidden", "");
        }
    }
}

/// `?data=<base>`: dev-server base URL for the git-ignored `game-data/` folder.
pub fn data_base() -> Option<String> {
    let search = web_sys::window()?.location().search().ok()?;
    web_sys::UrlSearchParams::new_with_str(&search)
        .ok()?
        .get("data")
}

pub fn load_from_query(proxy: EventLoopProxy<UserEvent>) {
    let Some(base) = data_base() else { return };
    wasm_bindgen_futures::spawn_local(async move {
        let mut packages = Vec::new();
        for name in ["data1.pkg", "patch1.pkg", "patch0.pkg", "data0.pkg"] {
            set_status(&format!("Fetching {name}…"), false);
            let url = format!("{base}original/WinData/{name}");
            let window = web_sys::window().unwrap();
            let result = async {
                let resp: web_sys::Response =
                    wasm_bindgen_futures::JsFuture::from(window.fetch_with_str(&url))
                        .await?
                        .dyn_into()?;
                if !resp.ok() {
                    return Err(JsValue::from_str(&format!("HTTP {}", resp.status())));
                }
                let buf = wasm_bindgen_futures::JsFuture::from(resp.array_buffer()?).await?;
                Ok::<_, JsValue>(js_sys::Uint8Array::new(&buf).to_vec())
            }
            .await;
            match result {
                Ok(bytes) => packages.push((name.to_string(), bytes)),
                Err(e) => log::warn!("{url}: {e:?}"),
            }
        }
        if packages.is_empty() {
            set_status(
                "Could not fetch game data. Select your .pkg files to continue.",
                false,
            );
            return;
        }
        let _ = proxy.send_event(UserEvent::Packages(packages));
    });
}

/// The stored mods, once read (`UserEvent::Mods`, empty when there are none).
pub fn load_stored_mods(proxy: EventLoopProxy<UserEvent>) {
    crate::save_store::stored_mods(move |mods| {
        if !mods.is_empty() {
            log::info!("{} mods in browser storage", mods.len());
        }
        let _ = proxy.send_event(UserEvent::Mods(mods));
    });
}

/// `name` without `.pkm` when it names a mod.
fn mod_name(name: &str) -> Option<String> {
    let lower = name.to_ascii_lowercase();
    lower
        .ends_with(".pkm")
        .then(|| name[..name.len() - 4].to_string())
}

/// Read, store and hand over the mods among `files`.
fn add_mods(files: Vec<web_sys::File>, proxy: EventLoopProxy<UserEvent>) {
    let files: Vec<(String, web_sys::File)> = files
        .into_iter()
        .filter_map(|f| Some((mod_name(&f.name())?, f)))
        .collect();
    if files.is_empty() {
        return;
    }
    wasm_bindgen_futures::spawn_local(async move {
        let mut mods = Vec::new();
        for (name, file) in files {
            match wasm_bindgen_futures::JsFuture::from(file.array_buffer()).await {
                Ok(buf) => {
                    let bytes = js_sys::Uint8Array::new(&buf).to_vec();
                    crate::save_store::store_mod(&name, &bytes);
                    mods.push((name, bytes));
                }
                Err(e) => log::error!("{}: {e:?}", file.name()),
            }
        }
        let _ = proxy.send_event(UserEvent::Mods(mods));
    });
}

/// Mods dropped on the page join the list (the browser's way of copying them into `WinData`).
pub fn install_drop(proxy: EventLoopProxy<UserEvent>) {
    let window = web_sys::window().unwrap();
    let over = Closure::<dyn FnMut(web_sys::DragEvent)>::new(|e: web_sys::DragEvent| {
        e.prevent_default();
    });
    let drop = Closure::<dyn FnMut(web_sys::DragEvent)>::new(move |e: web_sys::DragEvent| {
        e.prevent_default();
        let Some(list) = e.data_transfer().and_then(|d| d.files()) else {
            return;
        };
        let files = (0..list.length()).filter_map(|i| list.get(i)).collect();
        add_mods(files, proxy.clone());
    });
    let _ = window.add_event_listener_with_callback("dragover", over.as_ref().unchecked_ref());
    let _ = window.add_event_listener_with_callback("drop", drop.as_ref().unchecked_ref());
    over.forget();
    drop.forget();
}

/// Reads `.pkg` files chosen in the `#packages` input and hands them to the app; `.pkm` files
/// chosen with them are mods.
pub fn install_file_picker(proxy: EventLoopProxy<UserEvent>) {
    let input: web_sys::HtmlInputElement = document()
        .get_element_by_id("packages")
        .unwrap()
        .dyn_into()
        .unwrap();
    let target = input.clone();
    let on_change = Closure::<dyn FnMut()>::new(move || {
        let Some(files) = target.files() else { return };
        // Cancelling the chooser must leave the running game alone.
        if files.length() == 0 {
            return;
        }
        let all: Vec<web_sys::File> = (0..files.length()).filter_map(|i| files.get(i)).collect();
        add_mods(all.clone(), proxy.clone());
        let files: Vec<web_sys::File> = all
            .into_iter()
            .filter(|f| f.name().to_lowercase().ends_with(".pkg"))
            .collect();
        // Permit selecting the same files again after a read or parse error.
        target.set_value("");
        if files.is_empty() {
            set_status(
                "Select one or more .pkg files from your WinData folder.",
                false,
            );
            return;
        }
        target.set_disabled(true);
        let input = target.clone();
        let proxy = proxy.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let mut packages = Vec::new();
            for file in files {
                set_status(&format!("Loading {}…", file.name()), false);
                match wasm_bindgen_futures::JsFuture::from(file.array_buffer()).await {
                    Ok(buf) => packages.push((file.name(), js_sys::Uint8Array::new(&buf).to_vec())),
                    Err(e) => {
                        log::error!("{}: {e:?}", file.name());
                        set_status(
                            &format!("Could not read {}. Select the files again.", file.name()),
                            false,
                        );
                        input.set_disabled(false);
                        return;
                    }
                }
            }
            input.set_disabled(false);
            let _ = proxy.send_event(UserEvent::Packages(packages));
        });
    });
    input
        .add_event_listener_with_callback("change", on_change.as_ref().unchecked_ref())
        .unwrap();
    on_change.forget();
}
