//! Banished asset browser: models, textures, audio and dialogs, drawn with the reimplemented game UI.
//!
//! Native:     `cargo run -p banished-viewer --release [-- path/to/WinData]`
//! Web:        `tools/dev-web.sh --release`, then open `/?data=/game-data/` or pick `.pkg` files.
//! Screenshot: `banished-viewer --screenshot FILE.png [MODE] [FILTER]` renders the tool headlessly
//!             (MODE is models|textures|audio|dialogs; FILTER picks the first matching entry).
//! Capture:    `banished-viewer --capture OUT_DIR [FILTER ...]` renders meshes alone to PNGs.
//!
//! Controls: click modes and list entries, Up/Down to change entry, drag to orbit, wheel to zoom.

mod assets;
mod audio;
mod render;
mod tool;
mod ui_render;

use std::sync::Arc;

use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

use assets::Assets;
use render::Renderer;
use tool::{Mode, Tool};

pub enum UserEvent {
    Renderer(Renderer),
    Packages(Vec<(String, Vec<u8>)>),
}

struct App {
    proxy: EventLoopProxy<UserEvent>,
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    tool: Option<Tool>,
    pending_packages: Option<Vec<(String, Vec<u8>)>>,
    shift: bool,
}

fn audio_player() -> audio::Player {
    audio::Player::default()
}

impl App {
    fn try_load_assets(&mut self) {
        let Some(renderer) = &mut self.renderer else {
            return;
        };
        let Some(packages) = self.pending_packages.take() else {
            return;
        };
        match Assets::from_packages(packages) {
            Ok(assets) => {
                let mut tool = Tool::new(assets, audio_player());
                tool.set_mode(renderer, Mode::Models);
                self.tool = Some(tool);
                self.set_status("");
            }
            Err(e) => self.set_status(&format!("Could not read packages: {e}")),
        }
    }

    fn set_status(&self, text: &str) {
        if !text.is_empty() {
            log::info!("{text}");
        }
        #[cfg(target_arch = "wasm32")]
        web::set_status(text, self.tool.is_some());
    }

    fn redraw(&self) {
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }
}

impl ApplicationHandler<UserEvent> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        #[allow(unused_mut)]
        let mut attrs = Window::default_attributes().with_title("Banished asset browser");
        #[cfg(not(target_arch = "wasm32"))]
        {
            attrs = attrs.with_inner_size(winit::dpi::LogicalSize::new(1400.0, 860.0));
        }
        #[cfg(target_arch = "wasm32")]
        {
            use winit::platform::web::WindowAttributesExtWebSys;
            attrs = attrs.with_canvas(Some(web::canvas()));
        }
        let window = Arc::new(event_loop.create_window(attrs).expect("create window"));
        window.set_ime_allowed(true);
        self.window = Some(window.clone());
        let proxy = self.proxy.clone();
        let init = async move {
            let renderer = Renderer::new(window).await;
            let _ = proxy.send_event(UserEvent::Renderer(renderer));
        };
        #[cfg(not(target_arch = "wasm32"))]
        pollster::block_on(init);
        #[cfg(target_arch = "wasm32")]
        wasm_bindgen_futures::spawn_local(init);
    }

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::Renderer(mut renderer) => {
                if let Some(w) = &self.window {
                    let size = w.inner_size();
                    renderer.resize(size.width, size.height);
                }
                self.set_status(&format!("GPU: {}", renderer.backend));
                self.renderer = Some(renderer);
            }
            UserEvent::Packages(packages) => {
                self.set_status(&format!("Reading {} package(s)…", packages.len()));
                self.pending_packages = Some(packages);
            }
        }
        self.try_load_assets();
        self.redraw();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let (Some(renderer), Some(tool)) = (&mut self.renderer, &mut self.tool) else {
            match event {
                WindowEvent::CloseRequested => event_loop.exit(),
                WindowEvent::Resized(s) => {
                    if let Some(r) = &mut self.renderer {
                        r.resize(s.width, s.height);
                    }
                }
                _ => {}
            }
            return;
        };
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => renderer.resize(size.width, size.height),
            WindowEvent::RedrawRequested => {
                // Resize events can arrive before the (async) renderer exists; sync here.
                if let Some(w) = &self.window {
                    let size = w.inner_size();
                    if (size.width, size.height) != renderer.size() {
                        renderer.resize(size.width, size.height);
                    }
                }
                tool.layout(renderer.size());
                let quads = tool.quads();
                renderer.render(tool.frame(&quads));
                return;
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                use banished_ui::Key as UiKey;
                let key = match &event.logical_key {
                    Key::Named(NamedKey::Tab) => Some(if self.shift {
                        UiKey::BackTab
                    } else {
                        UiKey::Tab
                    }),
                    Key::Named(NamedKey::Enter) => Some(UiKey::Enter),
                    Key::Named(NamedKey::Escape) => Some(UiKey::Escape),
                    Key::Named(NamedKey::Space) => Some(UiKey::Space),
                    Key::Named(NamedKey::ArrowDown) => Some(UiKey::Down),
                    Key::Named(NamedKey::ArrowUp) => Some(UiKey::Up),
                    Key::Named(NamedKey::ArrowLeft) => Some(UiKey::Left),
                    Key::Named(NamedKey::ArrowRight) => Some(UiKey::Right),
                    Key::Named(NamedKey::PageDown) => Some(UiKey::PageDown),
                    Key::Named(NamedKey::PageUp) => Some(UiKey::PageUp),
                    Key::Named(NamedKey::Home) => Some(UiKey::Home),
                    Key::Named(NamedKey::End) => Some(UiKey::End),
                    Key::Named(NamedKey::Backspace) => Some(UiKey::Backspace),
                    Key::Named(NamedKey::Delete) => Some(UiKey::Delete),
                    Key::Character(s) => s.chars().next().map(UiKey::Character),
                    _ => None,
                };
                if let Some(key) = key {
                    tool.key(renderer, key, event.text.as_deref());
                }
            }
            WindowEvent::Ime(winit::event::Ime::Commit(text)) => tool.text_input(renderer, &text),
            WindowEvent::ModifiersChanged(modifiers) => self.shift = modifiers.state().shift_key(),
            WindowEvent::Focused(false) => tool.ui.clear_input(),
            WindowEvent::CursorLeft { .. } => tool.pointer_move(renderer, -1, -1),
            WindowEvent::MouseInput {
                button: MouseButton::Left,
                state,
                ..
            } => {
                tool.pointer_button(renderer, state == ElementState::Pressed);
            }
            WindowEvent::CursorMoved { position, .. } => {
                tool.pointer_move(renderer, position.x as i32, position.y as i32)
            }
            WindowEvent::MouseWheel { delta, .. } => tool.wheel(match delta {
                MouseScrollDelta::LineDelta(_, y) => y,
                MouseScrollDelta::PixelDelta(p) => p.y as f32 / 40.0,
            }),
            _ => return,
        }
        self.redraw();
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn default_data_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../game-data/original/WinData")
}

#[cfg(not(target_arch = "wasm32"))]
fn write_png(path: &std::path::Path, (w, h): (u32, u32), rgba: &[u8]) {
    let mut enc = png::Encoder::new(std::fs::File::create(path).expect("create png"), w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.write_header()
        .and_then(|mut wr| wr.write_image_data(rgba))
        .expect("write png");
}

/// Headless: render the whole tool UI to a PNG.
#[cfg(not(target_arch = "wasm32"))]
fn screenshot(file: &std::path::Path, mode: Option<&str>, filter: Option<&str>) {
    let assets =
        Assets::from_packages(assets::load_dir(&default_data_dir()).expect("read packages"))
            .expect("parse packages");
    let mut renderer = pollster::block_on(Renderer::headless(1400, 860));
    let mut tool = Tool::new(assets, audio_player());
    let mode = mode.and_then(Mode::parse).unwrap_or(Mode::Models);
    tool.set_mode(&mut renderer, mode);
    if let Some(f) = filter {
        let f = f.to_lowercase();
        let items = match mode {
            Mode::Models => &tool.assets.meshes,
            Mode::Textures => &tool.assets.textures,
            Mode::Audio => &tool.assets.sounds,
            Mode::Dialogs => &tool.assets.dialogs,
        };
        if let Some(i) = items.iter().position(|n| n.to_lowercase().contains(&f)) {
            tool.select(&mut renderer, i);
        }
    }
    tool.layout(renderer.size());
    let quads = tool.quads();
    let rgba = renderer.capture(tool.frame(&quads));
    write_png(file, renderer.size(), &rgba);
    log::info!("wrote {}", file.display());
}

/// Headless: render matching meshes alone to PNG files.
#[cfg(not(target_arch = "wasm32"))]
fn capture(out_dir: &std::path::Path, filters: &[String]) {
    const SIZE: u32 = 512;
    let assets =
        Assets::from_packages(assets::load_dir(&default_data_dir()).expect("read packages"))
            .expect("parse packages");
    let mut renderer = pollster::block_on(Renderer::headless(SIZE, SIZE));
    std::fs::create_dir_all(out_dir).expect("create output dir");
    let mut written = 0;
    for name in &assets.meshes {
        let lower = name.to_lowercase();
        if !filters.is_empty() && !filters.iter().any(|f| lower.contains(&f.to_lowercase())) {
            continue;
        }
        let mesh = match renderer.load_mesh(&assets, name) {
            Ok(m) => m,
            Err(e) => {
                log::error!("{name}: {e}");
                continue;
            }
        };
        let camera = tool::Camera {
            yaw: -0.8,
            pitch: 0.45,
            distance: mesh.radius * 2.6,
            target: mesh.center,
        };
        let rgba = renderer.capture(render::Frame {
            clear: render::CLEAR_SKY,
            mesh: Some(&mesh),
            view_proj: camera.view_proj(1.0),
            eye: camera.eye(),
            viewport: None,
            ui: None,
            textures: true,
            vertices_only: false,
        });
        let file = out_dir.join(name.trim_end_matches(".crs").replace('\\', "_") + ".png");
        write_png(&file, (SIZE, SIZE), &rgba);
        written += 1;
    }
    log::info!("wrote {written} captures to {}", out_dir.display());
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("--capture") => {
            let out = args.get(1).expect("--capture OUT_DIR [FILTER ...]");
            return capture(std::path::Path::new(out), &args[2..]);
        }
        Some("--screenshot") => {
            let out = args.get(1).expect("--screenshot FILE [MODE] [FILTER]");
            return screenshot(
                std::path::Path::new(out),
                args.get(2).map(String::as_str),
                args.get(3).map(String::as_str),
            );
        }
        _ => {}
    }
    let event_loop = EventLoop::<UserEvent>::with_user_event()
        .build()
        .expect("event loop");
    let proxy = event_loop.create_proxy();
    let dir = args
        .first()
        .map(std::path::PathBuf::from)
        .unwrap_or_else(default_data_dir);
    match assets::load_dir(&dir) {
        Ok(p) if !p.is_empty() => {
            let _ = proxy.send_event(UserEvent::Packages(p));
        }
        _ => log::error!(
            "No .pkg files in {}. Run tools/extract-assets.sh or pass the WinData folder.",
            dir.display()
        ),
    }
    let mut app = App {
        proxy,
        window: None,
        renderer: None,
        tool: None,
        pending_packages: None,
        shift: false,
    };
    event_loop.run_app(&mut app).expect("run");
}

#[cfg(target_arch = "wasm32")]
fn main() {
    console_error_panic_hook::set_once();
    let _ = console_log::init_with_level(log::Level::Info);
    let event_loop = EventLoop::<UserEvent>::with_user_event()
        .build()
        .expect("event loop");
    let proxy = event_loop.create_proxy();
    web::install_file_picker(proxy.clone());
    web::load_from_query(proxy.clone());
    use winit::platform::web::EventLoopExtWebSys;
    event_loop.spawn_app(App {
        proxy,
        window: None,
        renderer: None,
        tool: None,
        pending_packages: None,
        shift: false,
    });
}

#[cfg(target_arch = "wasm32")]
mod web {
    use super::UserEvent;
    use wasm_bindgen::prelude::*;
    use wasm_bindgen::JsCast;
    use winit::event_loop::EventLoopProxy;

    fn document() -> web_sys::Document {
        web_sys::window().unwrap().document().unwrap()
    }

    pub fn canvas() -> web_sys::HtmlCanvasElement {
        document()
            .get_element_by_id("viewer")
            .unwrap()
            .dyn_into()
            .unwrap()
    }

    /// Show loading progress in the HTML panel; hide the panel once the tool is running.
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

    /// Reads `.pkg` files chosen in the `#packages` input and hands them to the app.
    pub fn install_file_picker(proxy: EventLoopProxy<UserEvent>) {
        let input: web_sys::HtmlInputElement = document()
            .get_element_by_id("packages")
            .unwrap()
            .dyn_into()
            .unwrap();
        let target = input.clone();
        let on_change = Closure::<dyn FnMut()>::new(move || {
            let Some(files) = target.files() else { return };
            // Cancelling the chooser must leave the current viewer alone.
            if files.length() == 0 {
                return;
            }
            let files: Vec<web_sys::File> = (0..files.length())
                .filter_map(|i| files.get(i))
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
                        Ok(buf) => {
                            packages.push((file.name(), js_sys::Uint8Array::new(&buf).to_vec()))
                        }
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
}
