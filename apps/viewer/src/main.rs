//! Banished model viewer.
//!
//! Native: `cargo run -p banished-viewer --release [-- path/to/WinData]`
//! Web:    `tools/dev-web.sh`, then choose your `WinData/*.pkg` files, or open `/?data=/game-data/`
//!         to load them from the local dev server (never deploy that proxy).
//!
//! Capture: `cargo run -p banished-viewer --release -- --capture OUT_DIR [FILTER ...]`
//!          renders every mesh whose name contains a filter to OUT_DIR/*.png (no window).
//!
//! Controls: drag to orbit, wheel to zoom, Left/Right to change model (Shift = 10).

mod assets;
mod render;

use std::sync::Arc;

use glam::{Mat4, Vec3};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy};
use winit::keyboard::{Key, ModifiersState, NamedKey};
use winit::window::{Window, WindowId};

use assets::Assets;
use render::{GpuMesh, Renderer};

pub enum UserEvent {
    Renderer(Renderer),
    Packages(Vec<(String, Vec<u8>)>),
}

struct Camera {
    yaw: f32,
    pitch: f32,
    distance: f32,
    target: Vec3,
}

impl Camera {
    fn eye(&self) -> Vec3 {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        self.target + Vec3::new(cy * cp, sy * cp, sp) * self.distance
    }

    fn view_proj(&self, aspect: f32) -> Mat4 {
        use glam::camera::rh::{proj::directx::perspective, view::look_at_mat4};
        let proj = perspective(
            45f32.to_radians(),
            aspect,
            self.distance * 0.01,
            self.distance * 20.0,
        );
        proj * look_at_mat4(self.eye(), self.target, Vec3::Z)
    }
}

struct App {
    proxy: EventLoopProxy<UserEvent>,
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    assets: Option<Assets>,
    pending_packages: Option<Vec<(String, Vec<u8>)>>,
    index: usize,
    mesh: Option<GpuMesh>,
    camera: Camera,
    dragging: bool,
    last_cursor: Option<(f64, f64)>,
    modifiers: ModifiersState,
}

impl App {
    fn new(proxy: EventLoopProxy<UserEvent>) -> Self {
        App {
            proxy,
            window: None,
            renderer: None,
            assets: None,
            pending_packages: None,
            index: 0,
            mesh: None,
            camera: Camera {
                yaw: -0.8,
                pitch: 0.45,
                distance: 3.0,
                target: Vec3::ZERO,
            },
            dragging: false,
            last_cursor: None,
            modifiers: ModifiersState::empty(),
        }
    }

    fn try_load_assets(&mut self) {
        if self.renderer.is_none() {
            return;
        }
        let Some(packages) = self.pending_packages.take() else {
            return;
        };
        match Assets::from_packages(packages) {
            Ok(assets) => {
                self.assets = Some(assets);
                self.index = 0;
                self.show_mesh();
            }
            Err(e) => self.set_status(&format!("Could not read packages: {e}")),
        }
    }

    fn show_mesh(&mut self) {
        let (Some(renderer), Some(assets)) = (&self.renderer, &self.assets) else {
            return;
        };
        if assets.meshes.is_empty() {
            self.set_status("No meshes found in the selected packages.");
            return;
        }
        let name = assets.meshes[self.index].clone();
        match renderer.load_mesh(assets, &name) {
            Ok(mesh) => {
                self.camera.target = mesh.center;
                self.camera.distance = mesh.radius * 2.6;
                self.mesh = Some(mesh);
                let status = format!("[{}/{}] {name}", self.index + 1, assets.meshes.len());
                self.set_status(&status);
            }
            Err(e) => {
                self.mesh = None;
                self.set_status(&format!("{name}: {e}"));
            }
        }
        self.request_redraw();
    }

    fn step(&mut self, delta: isize) {
        let Some(assets) = &self.assets else { return };
        let n = assets.meshes.len() as isize;
        if n > 0 {
            self.index = (self.index as isize + delta).rem_euclid(n) as usize;
            self.show_mesh();
        }
    }

    fn set_status(&self, text: &str) {
        log::info!("{text}");
        if let Some(w) = &self.window {
            w.set_title(&format!("Banished viewer — {text}"));
        }
        #[cfg(target_arch = "wasm32")]
        web::set_status(text);
    }

    fn request_redraw(&self) {
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
        let mut attrs = Window::default_attributes().with_title("Banished viewer");
        #[cfg(target_arch = "wasm32")]
        {
            use winit::platform::web::WindowAttributesExtWebSys;
            attrs = attrs.with_canvas(Some(web::canvas()));
        }
        let window = Arc::new(event_loop.create_window(attrs).expect("create window"));
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
            UserEvent::Renderer(renderer) => {
                self.set_status(&format!("GPU: {}", renderer.backend));
                self.renderer = Some(renderer);
                if let Some(w) = &self.window {
                    let size = w.inner_size();
                    self.renderer
                        .as_mut()
                        .unwrap()
                        .resize(size.width, size.height);
                }
                self.try_load_assets();
            }
            UserEvent::Packages(packages) => {
                self.set_status(&format!("Reading {} package(s)…", packages.len()));
                self.pending_packages = Some(packages);
                self.try_load_assets();
            }
        }
        self.request_redraw();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(r) = &mut self.renderer {
                    r.resize(size.width, size.height);
                }
                self.request_redraw();
            }
            WindowEvent::RedrawRequested => {
                if let Some(r) = &self.renderer {
                    r.render(
                        self.mesh.as_ref(),
                        self.camera.view_proj(r.aspect()),
                        self.camera.eye(),
                    );
                }
            }
            WindowEvent::ModifiersChanged(m) => self.modifiers = m.state(),
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                let step = if self.modifiers.shift_key() { 10 } else { 1 };
                match event.logical_key {
                    Key::Named(NamedKey::ArrowRight) => self.step(step),
                    Key::Named(NamedKey::ArrowLeft) => self.step(-step),
                    _ => {}
                }
            }
            WindowEvent::MouseInput {
                button: MouseButton::Left,
                state,
                ..
            } => {
                self.dragging = state == ElementState::Pressed;
            }
            WindowEvent::CursorMoved { position, .. } => {
                if let (true, Some((x, y))) = (self.dragging, self.last_cursor) {
                    self.camera.yaw -= (position.x - x) as f32 * 0.01;
                    self.camera.pitch =
                        (self.camera.pitch + (position.y - y) as f32 * 0.01).clamp(-1.5, 1.5);
                    self.request_redraw();
                }
                self.last_cursor = Some((position.x, position.y));
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let amount = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32 / 40.0,
                };
                self.camera.distance *= (1.0 - amount * 0.1).clamp(0.5, 1.5);
                self.request_redraw();
            }
            _ => {}
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn default_data_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../game-data/original/WinData")
}

/// Headless: render matching meshes to PNG files.
#[cfg(not(target_arch = "wasm32"))]
fn capture(out_dir: &std::path::Path, filters: &[String]) {
    const SIZE: u32 = 512;
    let assets =
        Assets::from_packages(assets::load_dir(&default_data_dir()).expect("read packages"))
            .expect("parse packages");
    let renderer = pollster::block_on(Renderer::headless(SIZE, SIZE));
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
        let camera = Camera {
            yaw: -0.8,
            pitch: 0.45,
            distance: mesh.radius * 2.6,
            target: mesh.center,
        };
        let rgba = renderer.capture(Some(&mesh), camera.view_proj(1.0), camera.eye());
        let file = out_dir.join(name.trim_end_matches(".crs").replace('\\', "_") + ".png");
        let mut enc = png::Encoder::new(
            std::fs::File::create(&file).expect("create png"),
            SIZE,
            SIZE,
        );
        enc.set_color(png::ColorType::Rgba);
        enc.write_header()
            .and_then(|mut w| w.write_image_data(&rgba))
            .expect("write png");
        written += 1;
    }
    log::info!("wrote {written} captures to {}", out_dir.display());
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("--capture") {
        let out = args.get(1).expect("--capture OUT_DIR [FILTER ...]");
        capture(std::path::Path::new(out), &args[2..]);
        return;
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
        Ok(_) | Err(_) => log::error!(
            "No .pkg files in {}. Run tools/extract-assets.sh or pass the WinData folder.",
            dir.display()
        ),
    }

    let mut app = App::new(proxy);
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
    event_loop.spawn_app(App::new(proxy));
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

    pub fn set_status(text: &str) {
        if let Some(el) = document().get_element_by_id("status") {
            el.set_text_content(Some(text));
        }
    }

    /// Dev mode: `?data=<base url>` fetches the four packages from that base (e.g. the trunk proxy).
    pub fn load_from_query(proxy: EventLoopProxy<UserEvent>) {
        let search = web_sys::window()
            .unwrap()
            .location()
            .search()
            .unwrap_or_default();
        let Ok(params) = web_sys::UrlSearchParams::new_with_str(&search) else {
            return;
        };
        let Some(base) = params.get("data") else {
            return;
        };
        wasm_bindgen_futures::spawn_local(async move {
            let mut packages = Vec::new();
            for name in ["data1.pkg", "patch1.pkg", "patch0.pkg", "data0.pkg"] {
                set_status(&format!("Fetching {name}…"));
                let url = format!("{base}{name}");
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
            let files: Vec<web_sys::File> =
                (0..files.length()).filter_map(|i| files.get(i)).collect();
            let proxy = proxy.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let mut packages = Vec::new();
                for file in files
                    .into_iter()
                    .filter(|f| f.name().to_lowercase().ends_with(".pkg"))
                {
                    set_status(&format!("Loading {}…", file.name()));
                    match wasm_bindgen_futures::JsFuture::from(file.array_buffer()).await {
                        Ok(buf) => {
                            packages.push((file.name(), js_sys::Uint8Array::new(&buf).to_vec()))
                        }
                        Err(e) => log::error!("{}: {e:?}", file.name()),
                    }
                }
                let _ = proxy.send_event(UserEvent::Packages(packages));
            });
        });
        input
            .add_event_listener_with_callback("change", on_change.as_ref().unchecked_ref())
            .unwrap();
        on_change.forget();
    }
}
