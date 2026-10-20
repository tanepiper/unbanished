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

/// Show the engine version (`version::NAME_VERSION`) under the status line of the loading panel.
pub fn show_version(text: &str) {
    if let Some(el) = document().get_element_by_id("version") {
        el.set_text_content(Some(text));
    }
}

/// `?data=<base>`: dev-server base URL for the git-ignored `game-data/` folder.
///
/// Only honoured when the page itself is served from this machine (`tools/dev-web.sh`). On the
/// published site the player must pick their own `.pkg` files, so a link cannot make the page
/// load game data from another server.
pub fn data_base() -> Option<String> {
    local_param("data")
}

/// A query parameter of a page served from this machine; none on any other host.
fn local_param(name: &str) -> Option<String> {
    let location = web_sys::window()?.location();
    let host = location.hostname().ok()?;
    if !matches!(host.as_str(), "localhost" | "127.0.0.1" | "[::1]" | "::1") {
        return None;
    }
    let search = location.search().ok()?;
    web_sys::UrlSearchParams::new_with_str(&search)
        .ok()?
        .get(name)
}

/// `?digest=<save URL>&updates=N&every=M` (local pages only, as `?data=`): fetch a save to run
/// without drawing, at speed 10, and show the simulation's digests (`gameplay::digest`), to
/// compare with `banished --sim-digest` (docs/CROSS-PLATFORM.md).
pub fn load_digest_save(proxy: EventLoopProxy<UserEvent>) {
    let Some(url) = local_param("digest") else {
        return;
    };
    let number = |name: &str, default: u32| {
        local_param(name)
            .and_then(|v| v.parse().ok())
            .unwrap_or(default)
    };
    let (updates, every) = (number("updates", 1200), number("every", 60));
    wasm_bindgen_futures::spawn_local(async move {
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
            Ok(save) => {
                let _ = proxy.send_event(UserEvent::Digest {
                    save,
                    updates,
                    every,
                });
            }
            Err(e) => show_digest(&format!("{url}: {e:?}")),
        }
    });
}

/// `?net=RELAY&room=CODE&updates=N` (local pages only): join a relay room as a player of a
/// networked test (`gameplay::network::JoinSession`).
pub fn net_request() -> Option<(String, String, u64)> {
    Some((
        local_param("net")?,
        local_param("room")?,
        local_param("updates")?.parse().ok()?,
    ))
}

/// The digests (or an error) in a `<pre id="digest">` at the end of the page, and the log.
pub fn show_digest(text: &str) {
    log::info!("digest:\n{text}");
    set_digest_text(text);
}

/// The text of the `<pre id="digest">`, without logging it (it changes every frame).
pub fn set_digest_text(text: &str) {
    let doc = document();
    let pre = doc.get_element_by_id("digest").unwrap_or_else(|| {
        let pre = doc.create_element("pre").unwrap();
        pre.set_id("digest");
        let _ = doc.body().unwrap().append_child(&pre);
        pre
    });
    pre.set_text_content(Some(text));
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

thread_local! {
    /// Whether game files are kept in this browser (the loading screen's Start button).
    static KEPT: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Calls `f` when the element `id` is clicked.
fn on_click(id: &str, f: impl FnMut() + 'static) {
    let Some(el) = document().get_element_by_id(id) else {
        return;
    };
    let click = Closure::<dyn FnMut()>::new(f);
    let _ = el.add_event_listener_with_callback("click", click.as_ref().unchecked_ref());
    click.forget();
}

/// Show or hide the element `id`.
fn show(id: &str, visible: bool) {
    if let Some(el) = document().get_element_by_id(id) {
        let _ = if visible {
            el.remove_attribute("hidden")
        } else {
            el.set_attribute("hidden", "")
        };
    }
}

/// The game files kept from an earlier visit (#17). The start page then shows Start the game
/// and the manager instead of the steps (`body.has-kept`; its Help button shows the steps, for
/// other files or more mods). Not with `?data=`, which loads the files from the development
/// server instead.
pub fn load_kept_packages(proxy: EventLoopProxy<UserEvent>) {
    if data_base().is_some() {
        return;
    }
    on_click("start-kept", move || {
        let proxy = proxy.clone();
        if let Some(b) = document().get_element_by_id("start-kept") {
            let _ = b.set_attribute("disabled", "");
        }
        set_status("Loading the game files kept in this browser…", false);
        crate::saves::save_store::stored_packages(move |packages| {
            if packages.is_empty() {
                set_status(
                    "The kept game files could not be read. Choose them again.",
                    false,
                );
                return;
            }
            let _ = proxy.send_event(UserEvent::Packages(packages));
        });
    });
    crate::saves::save_store::kept_package_names(|names| {
        if names.is_empty() {
            return;
        }
        log::info!("{} game files kept in browser storage", names.len());
        KEPT.with(|k| k.set(true));
        show("kept", true);
        if let Some(body) = document().body() {
            body.set_class_name(&format!("{} has-kept", body.class_name()));
        }
        set_status("Ready. Press Start the game.", false);
    });
}

/// The start page's save import (the manager's "Import saves", #17): `.sav` files chosen in
/// `#import-saves` are read by the game's own save reader (which also makes the record the Load
/// dialog lists) and kept in the browser; then the page starts again with the manager open.
pub fn install_save_import() {
    let Some(input) = document()
        .get_element_by_id("import-saves")
        .and_then(|e| e.dyn_into::<web_sys::HtmlInputElement>().ok())
    else {
        return;
    };
    let target = input.clone();
    let change = Closure::<dyn FnMut()>::new(move || {
        let Some(list) = target.files() else { return };
        let files: Vec<web_sys::File> = (0..list.length()).filter_map(|i| list.get(i)).collect();
        target.set_value("");
        wasm_bindgen_futures::spawn_local(async move {
            let mut failed = Vec::new();
            for file in files {
                let name = file.name();
                let stem = name
                    .strip_suffix(".sav")
                    .or_else(|| name.strip_suffix(".SAV"))
                    .unwrap_or(&name)
                    .to_string();
                let bytes = match wasm_bindgen_futures::JsFuture::from(file.array_buffer()).await {
                    Ok(buf) => js_sys::Uint8Array::new(&buf).to_vec(),
                    Err(_) => {
                        failed.push(name);
                        continue;
                    }
                };
                if let Err(e) = crate::saves::save_store::write_then(&stem, &bytes).await {
                    log::error!("importing {name}: {e}");
                    failed.push(name);
                }
            }
            if failed.is_empty() {
                if let Some(w) = web_sys::window() {
                    let _ = w.location().reload();
                }
            } else {
                set_status(
                    &format!("Not a Banished save: {}", failed.join(", ")),
                    false,
                );
            }
        });
    });
    let _ = input.add_event_listener_with_callback("change", change.as_ref().unchecked_ref());
    change.forget();
}

/// The stored mods, once read (`UserEvent::Mods`, empty when there are none).
pub fn load_stored_mods(proxy: EventLoopProxy<UserEvent>) {
    crate::saves::save_store::stored_mods(move |mods| {
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
                    crate::saves::save_store::store_mod(&name, &bytes);
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
            // Only mods: with kept game files, that is all the player wanted.
            if KEPT.with(|k| k.get()) {
                set_status(
                    "Mods added. Press Start the game when you are ready.",
                    false,
                );
            } else {
                set_status(
                    "Select one or more .pkg files from your WinData folder.",
                    false,
                );
            }
            return;
        }
        // "Keep the game files in this browser" (#17): kept for the next visit.
        let keep = document()
            .get_element_by_id("keep")
            .and_then(|e| e.dyn_into::<web_sys::HtmlInputElement>().ok())
            .is_none_or(|k| k.checked());
        if keep {
            crate::saves::save_store::store_packages(files.clone());
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
