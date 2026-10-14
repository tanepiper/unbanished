//! Banished, reimplemented. Boots like the original: global data, the Shining Rock splash, the
//! loading screen, then the main menu with its music. New Game enters the generated world with an
//! interactive HUD, calendar, time controls and camera navigation.
//!
//! Native: `cargo run -p banished-app --release -- [path/to/WinData]`
//! Web:    `tools/dev-web.sh --app game --release`, then open `/?data=/game-data/`.
//! Captures (headless PNG):
//!   `banished --boot-capture OUT.png SECONDS` runs the boot sequence for SECONDS of game time;
//!   `banished --capture OUT.png [--seed N --terrain Valleys|Mountains --size 0|1|2 --month M
//!    --view overview|ground]` renders a generated map.
//!
//! On the map: left/right drag to rotate, middle drag to pan, wheel to zoom, WASD/arrows to move,
//! Escape for the main menu.

mod gameplay;
mod global;
mod loops;
mod menu;
mod models;
mod render;
mod scene;
mod scene_assets;
mod scene_gfx;
mod toolbar;
mod utility;
#[cfg(target_arch = "wasm32")]
mod web;
mod world;

use std::rc::Rc;
use std::sync::Arc;

use banished_audio::Player;
use banished_formats::pkg::{Archive, Package};
use banished_game::random::Random;
use banished_ui::Ui;
use web_time::Instant;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

use loops::{Ctx, DialogLoaderLoop, LoaderLoop, MainLoop, RawInput, View};
use render::{Frame, Renderer};

pub enum UserEvent {
    Renderer(Box<Renderer>),
    Packages(Vec<(String, Vec<u8>)>),
}

const CLEAR: wgpu::Color = wgpu::Color {
    r: 0.0,
    g: 0.0,
    b: 0.0,
    a: 1.0,
};

/// The running game: shared resources plus the loop chain.
pub struct Game {
    archive: Rc<Archive>,
    ui: Ui,
    audio: Player,
    global: Option<global::GlobalData>,
    rng: Random,
    main: MainLoop,
}

/// The startup chain built in `FUN_140042850`: a loader loop for "Game/Global" (`FUN_14009f280`),
/// the Shining Rock splash (at least 2 s), the loading screen (at least 0.5 s), then the menu.
fn boot_chain() -> MainLoop {
    let menu = Box::new(menu::MenuLoop::new());
    let loading = Box::new(DialogLoaderLoop::new(
        "Dialog/Loading.rsc:resource".into(),
        0.5,
        menu,
    ));
    let splash = Box::new(DialogLoaderLoop::new(
        "Dialog/ShiningRock.rsc:resource".into(),
        2.0,
        loading,
    ));
    MainLoop::new(Box::new(LoaderLoop::new(
        "Game/Global",
        |ctx: &mut Ctx| match global::GlobalData::load(ctx.archive) {
            Ok(g) => *ctx.global = Some(g),
            Err(e) => log::error!("global game data: {e}"),
        },
        splash,
    )))
}

fn clock_seed() -> u32 {
    let ms = web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    ms as u32
}

impl Game {
    pub fn new(packages: Vec<(String, Vec<u8>)>) -> Result<Game, String> {
        let mut archive = Archive::default();
        for (name, bytes) in packages {
            archive.add(Package::parse(bytes).map_err(|e| format!("{name}: {e}"))?);
        }
        if archive.is_empty() {
            return Err("no packages".into());
        }
        let archive = Rc::new(archive);
        Ok(Game {
            ui: Ui::new(archive.clone()),
            archive,
            audio: Player::default(),
            global: None,
            // `System::Random()` seeds from the clock.
            rng: Random::new(clock_seed()),
            main: boot_chain(),
        })
    }

    fn with_ctx<R>(&mut self, f: impl FnOnce(&mut MainLoop, &mut Ctx) -> R) -> R {
        let mut ctx = Ctx {
            archive: &self.archive,
            ui: &mut self.ui,
            audio: &mut self.audio,
            global: &mut self.global,
            rng: &mut self.rng,
        };
        f(&mut self.main, &mut ctx)
    }

    pub fn update(&mut self, dt: f32) {
        self.with_ctx(|main, ctx| main.update(ctx, dt));
    }

    fn input_root(&self) -> Option<usize> {
        self.main.current().screen().map(|s| s.input_root())
    }

    fn raw(&mut self, input: RawInput) {
        self.with_ctx(|main, ctx| main.raw(ctx, input));
    }

    fn deliver_events(&mut self) {
        self.with_ctx(|main, ctx| main.events(ctx));
    }

    pub fn pointer_move(&mut self, x: i32, y: i32) {
        if let Some(root) = self.input_root() {
            self.ui.pointer_move(x, y, root);
        }
    }

    pub fn pointer_button(&mut self, down: bool) -> bool {
        self.audio.unlock();
        if let Some(root) = self.input_root() {
            let captured = self.ui.pointer_over_interactive();
            if let Some(event) = self.ui.pointer_button(down, root) {
                self.with_ctx(|main, ctx| main.inject(ctx, event));
            }
            self.deliver_events();
            captured
        } else {
            false
        }
    }

    pub fn wheel(&mut self, steps: f32) {
        if self.input_root().is_some() && self.ui.wheel((steps * 40.0) as i32) {
            self.deliver_events();
        } else {
            self.raw(RawInput::Wheel(steps));
        }
    }

    pub fn drag(&mut self, dx: f32, dy: f32) {
        self.raw(RawInput::Drag(dx, dy));
    }

    pub fn key(&mut self, key: banished_ui::Key, text: Option<&str>) {
        self.audio.unlock();
        let Some(root) = self.input_root() else {
            use banished_ui::Key as K;
            let raw = match key {
                K::Escape => RawInput::Escape,
                K::Up | K::Character('w') => RawInput::Pan(1.0, 0.0),
                K::Down | K::Character('s') => RawInput::Pan(-1.0, 0.0),
                K::Left | K::Character('a') => RawInput::Pan(0.0, -1.0),
                K::Right | K::Character('d') => RawInput::Pan(0.0, 1.0),
                _ => return,
            };
            return self.raw(raw);
        };
        let handled = self.ui.key(root, key);
        if !handled {
            if let Some(text) = text.filter(|t| t.chars().all(|c| !c.is_control())) {
                self.ui.text_input(text);
            }
        }
        self.deliver_events();
        if !handled && !self.ui.text_edit_focused() {
            use banished_ui::Key as K;
            let raw = match key {
                K::Escape => Some(RawInput::Escape),
                K::Up => Some(RawInput::Pan(1.0, 0.0)),
                K::Down => Some(RawInput::Pan(-1.0, 0.0)),
                K::Left => Some(RawInput::Pan(0.0, -1.0)),
                K::Right => Some(RawInput::Pan(0.0, 1.0)),
                K::Character('w') => Some(RawInput::Pan(1.0, 0.0)),
                K::Character('s') => Some(RawInput::Pan(-1.0, 0.0)),
                K::Character('a') => Some(RawInput::Pan(0.0, -1.0)),
                K::Character('d') => Some(RawInput::Pan(0.0, 1.0)),
                _ => None,
            };
            if let Some(raw) = raw {
                self.raw(raw);
            }
        }
    }

    /// Click the named control through the normal pointer path (scripted runs): lay out the
    /// screen at `size`, move to the control's centre, press and release.
    pub fn press(&mut self, name: &str, size: (u32, u32)) -> bool {
        let Some(screen) = self.main.current().screen() else {
            return false;
        };
        screen.layout(&mut self.ui, size);
        let found = screen
            .dialogs
            .iter()
            .rev()
            .find_map(|d| self.ui.find(d.root, name));
        let Some(id) = found else { return false };
        let [x0, y0, x1, y1] = self.ui.node(id).rect;
        self.pointer_move((x0 + x1) / 2, (y0 + y1) / 2);
        self.pointer_button(true);
        self.pointer_button(false);
        true
    }

    pub fn quit_requested(&self) -> bool {
        self.main.quit
    }

    /// Lay out the current loop's UI and build the frame.
    pub fn draw(&mut self, renderer: &mut Renderer, capture: bool) -> Option<Vec<u8>> {
        let size = renderer.size();
        if let Some(screen) = self.main.current().screen() {
            screen.layout(&mut self.ui, size);
        }
        let mut quads = self
            .main
            .current()
            .screen()
            .map(|s| self.ui.draw(s.root))
            .unwrap_or_default();
        quads.extend(loops::fade_quad(self.main.fade.level, size));
        renderer.set_ui_time(self.main.elapsed);
        let (terrain, clear) = match self.main.current().view() {
            View::World(view) => {
                renderer.set_scene(view);
                let fog = view.lighting.fog_color;
                (
                    Some(view.frame(renderer.aspect(), self.main.elapsed)),
                    // Until the sky mesh is drawn, clear to the fog colour.
                    wgpu::Color {
                        r: fog.x as f64,
                        g: fog.y as f64,
                        b: fog.z as f64,
                        a: 1.0,
                    },
                )
            }
            View::None => (None, CLEAR),
        };
        let frame = Frame {
            terrain,
            clear,
            ui: Some((&self.ui, &quads)),
        };
        if capture {
            #[cfg(not(target_arch = "wasm32"))]
            return Some(renderer.capture(&frame));
        }
        renderer.render(&frame);
        None
    }
}

fn ui_key(key: &Key, shift: bool) -> Option<banished_ui::Key> {
    use banished_ui::Key as K;
    Some(match key {
        Key::Named(NamedKey::Tab) => {
            if shift {
                K::BackTab
            } else {
                K::Tab
            }
        }
        Key::Named(NamedKey::Enter) => K::Enter,
        Key::Named(NamedKey::Escape) => K::Escape,
        Key::Named(NamedKey::Space) => K::Space,
        Key::Named(NamedKey::ArrowDown) => K::Down,
        Key::Named(NamedKey::ArrowUp) => K::Up,
        Key::Named(NamedKey::ArrowLeft) => K::Left,
        Key::Named(NamedKey::ArrowRight) => K::Right,
        Key::Named(NamedKey::PageDown) => K::PageDown,
        Key::Named(NamedKey::PageUp) => K::PageUp,
        Key::Named(NamedKey::Home) => K::Home,
        Key::Named(NamedKey::End) => K::End,
        Key::Named(NamedKey::Backspace) => K::Backspace,
        Key::Named(NamedKey::Delete) => K::Delete,
        Key::Character(s) => K::Character(s.chars().next()?),
        _ => return None,
    })
}

struct App {
    proxy: EventLoopProxy<UserEvent>,
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    pending_packages: Option<Vec<(String, Vec<u8>)>>,
    game: Option<Game>,
    last_frame: Option<Instant>,
    cursor: (f64, f64),
    drag_button: Option<MouseButton>,
    shift: bool,
}

impl App {
    fn new(proxy: EventLoopProxy<UserEvent>) -> App {
        App {
            proxy,
            window: None,
            renderer: None,
            pending_packages: None,
            game: None,
            last_frame: None,
            cursor: (0.0, 0.0),
            drag_button: None,
            shift: false,
        }
    }

    fn try_start(&mut self) {
        if self.renderer.is_none() {
            return;
        }
        let Some(packages) = self.pending_packages.take() else {
            return;
        };
        match Game::new(packages) {
            Ok(game) => {
                self.game = Some(game);
                set_status("", true);
            }
            Err(e) => set_status(&format!("Could not read packages: {e}"), false),
        }
    }
}

fn set_status(text: &str, loaded: bool) {
    if !text.is_empty() {
        log::info!("{text}");
    }
    #[cfg(target_arch = "wasm32")]
    web::set_status(text, loaded);
    #[cfg(not(target_arch = "wasm32"))]
    let _ = loaded;
}

impl ApplicationHandler<UserEvent> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        #[allow(unused_mut)]
        let mut attrs = Window::default_attributes().with_title("Banished");
        #[cfg(not(target_arch = "wasm32"))]
        {
            attrs = attrs.with_inner_size(winit::dpi::LogicalSize::new(1280.0, 800.0));
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
            let _ = proxy.send_event(UserEvent::Renderer(Box::new(renderer)));
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
                log::info!("GPU: {}", renderer.backend);
                self.renderer = Some(*renderer);
            }
            UserEvent::Packages(packages) => {
                set_status(&format!("Reading {} package(s)…", packages.len()), false);
                self.pending_packages = Some(packages);
            }
        }
        self.try_start();
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(s) => {
                if let Some(r) = &mut self.renderer {
                    r.resize(s.width, s.height);
                }
            }
            WindowEvent::RedrawRequested => {
                let (Some(renderer), Some(game)) = (&mut self.renderer, &mut self.game) else {
                    return;
                };
                if let Some(w) = &self.window {
                    let size = w.inner_size();
                    if (size.width, size.height) != renderer.size() {
                        renderer.resize(size.width, size.height);
                    }
                }
                let now = Instant::now();
                let dt = self
                    .last_frame
                    .map_or(0.0, |t| (now - t).as_secs_f32())
                    .min(0.1);
                self.last_frame = Some(now);
                game.update(dt);
                if game.quit_requested() {
                    event_loop.exit();
                    return;
                }
                game.draw(renderer, false);
            }
            WindowEvent::ModifiersChanged(m) => self.shift = m.state().shift_key(),
            WindowEvent::CursorMoved { position, .. } => {
                let (dx, dy) = (position.x - self.cursor.0, position.y - self.cursor.1);
                self.cursor = (position.x, position.y);
                if let Some(game) = &mut self.game {
                    game.pointer_move(position.x as i32, position.y as i32);
                    match self.drag_button {
                        Some(MouseButton::Left | MouseButton::Right) => {
                            game.drag(dx as f32, dy as f32);
                        }
                        Some(MouseButton::Middle) => {
                            game.raw(RawInput::PanDrag(dx as f32, dy as f32))
                        }
                        _ => {}
                    }
                }
            }
            WindowEvent::Focused(false) => self.drag_button = None,
            WindowEvent::MouseInput { button, state, .. } => {
                let down = state == ElementState::Pressed;
                let Some(game) = &mut self.game else { return };
                match button {
                    MouseButton::Left => {
                        // A loop switch can replace the screen beneath a stationary cursor
                        // without producing another CursorMoved event. Refresh the hit target
                        // against the current loop before deciding whether the UI captures drag.
                        game.pointer_move(self.cursor.0 as i32, self.cursor.1 as i32);
                        let captured = game.pointer_button(down);
                        if down {
                            self.drag_button = (!captured).then_some(MouseButton::Left);
                        } else if self.drag_button == Some(MouseButton::Left) {
                            self.drag_button = None;
                        }
                    }
                    MouseButton::Right | MouseButton::Middle => {
                        if down {
                            self.drag_button = Some(button);
                        } else if self.drag_button == Some(button) {
                            self.drag_button = None;
                        }
                    }
                    _ => {}
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let steps = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32 / 40.0,
                };
                if let Some(game) = &mut self.game {
                    game.wheel(steps);
                }
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                if let (Some(game), Some(key)) =
                    (&mut self.game, ui_key(&event.logical_key, self.shift))
                {
                    game.key(key, event.text.as_deref());
                }
            }
            WindowEvent::Ime(winit::event::Ime::Commit(text)) => {
                if let Some(game) = &mut self.game {
                    game.ui.text_input(&text);
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        // Fades, animations and the camera sway need continuous frames.
        if let (Some(w), Some(_)) = (&self.window, &self.game) {
            w.request_redraw();
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use super::*;

    pub fn data_dir(args: &[String]) -> std::path::PathBuf {
        args.iter()
            .find(|a| !a.starts_with("--") && std::path::Path::new(a).is_dir())
            .map(Into::into)
            .unwrap_or_else(|| {
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../game-data/original/WinData")
            })
    }

    pub fn read_packages(dir: &std::path::Path) -> Vec<(String, Vec<u8>)> {
        ["data1.pkg", "patch1.pkg", "patch0.pkg", "data0.pkg"]
            .iter()
            .filter_map(|name| match std::fs::read(dir.join(name)) {
                Ok(bytes) => Some((name.to_string(), bytes)),
                Err(e) => {
                    log::warn!("{}: {e}", dir.join(name).display());
                    None
                }
            })
            .collect()
    }

    pub fn write_png(path: &str, (w, h): (u32, u32), rgba: &[u8]) {
        let file = std::fs::File::create(path).expect("create png");
        let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header()
            .and_then(|mut wr| wr.write_image_data(rgba))
            .expect("write png");
        log::info!("wrote {path}");
    }

    /// Run the boot sequence headlessly at 30 frames per second and capture one frame.
    pub fn boot_capture(args: &[String], out: &str, seconds: f32) {
        let mut game = Game::new(read_packages(&data_dir(args))).expect("game data");
        game.audio.set_volume(0.0);
        let size = (1280, 800);
        let mut renderer = pollster::block_on(Renderer::headless(size.0, size.1));
        // `--press NAME@SECONDS` clicks a control at a given time.
        let mut presses: Vec<(String, f32)> = args
            .windows(2)
            .filter(|w| w[0] == "--press")
            .filter_map(|w| {
                let (name, at) = w[1].split_once('@')?;
                Some((name.to_string(), at.parse().ok()?))
            })
            .collect();
        let dt = 1.0 / 30.0;
        let mut t = 0.0;
        while t < seconds {
            game.update(dt);
            t += dt;
            presses.retain(|(name, at)| {
                if t < *at {
                    return true;
                }
                if !game.press(name, size) {
                    log::warn!("--press {name}: not found at {t:.2} s");
                }
                false
            });
        }
        let rgba = game.draw(&mut renderer, true).expect("capture");
        write_png(out, size, &rgba);
    }

    /// The generated-map capture from M5 (`--capture`).
    pub fn map_capture(args: &[String], out: &str) {
        use crate::scene::{Camera, SceneCamera, SceneView};
        use crate::world::{World, WorldParams};
        let mut params = WorldParams::default();
        let (mut month, mut overview) = (3usize, false);
        let value = |flag: &str| {
            args.iter()
                .position(|a| a == flag)
                .and_then(|i| args.get(i + 1))
                .cloned()
        };
        if let Some(v) = value("--seed") {
            params.seed = v.parse().expect("--seed N");
        }
        if let Some(v) = value("--terrain") {
            params.terrain = v;
        }
        if let Some(v) = value("--size") {
            params.size = v.parse().expect("--size 0|1|2");
        }
        if let Some(v) = value("--month") {
            month = v.parse::<usize>().expect("--month 0..11") % 12;
        }
        if let Some(v) = value("--view") {
            overview = v == "overview";
        }
        let mut archive = Archive::default();
        for (_, bytes) in read_packages(&data_dir(args)) {
            archive.add(Package::parse(bytes).expect("parse package"));
        }
        let world = World::generate(&archive, &params).expect("generate world");
        let mut lighting = render::Lighting::from_month(&world.environment.months[month]);
        let camera = if overview {
            // Fog distances are tuned for the in-game camera; push them out for whole-map views.
            lighting.fog_start = 1e5;
            lighting.fog_end = 2e5;
            Camera::overview(&world.terrain)
        } else {
            Camera::ground()
        };
        let size = (1280, 800);
        let mut renderer = pollster::block_on(Renderer::headless(size.0, size.1));
        let view = SceneView::new(world.terrain, SceneCamera::Map(camera), lighting);
        renderer.set_scene(&view);
        let rgba = renderer.capture(&Frame {
            terrain: Some(view.frame(renderer.aspect(), 0.0)),
            clear: wgpu::Color {
                r: 0.75,
                g: 0.75,
                b: 0.75,
                a: 1.0,
            },
            ui: None,
        });
        write_png(out, size, &rgba);
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("--boot-capture") => {
            let out = args.get(1).expect("--boot-capture OUT.png SECONDS");
            let seconds = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(5.0);
            return native::boot_capture(&args, out, seconds);
        }
        Some("--capture") => {
            let out = args.get(1).expect("--capture OUT.png [OPTIONS]");
            return native::map_capture(&args, out);
        }
        _ => {}
    }
    let packages = native::read_packages(&native::data_dir(&args));
    if packages.is_empty() {
        eprintln!("No .pkg files found. Run tools/extract-assets.sh or pass the WinData folder.");
        std::process::exit(1);
    }
    let event_loop = EventLoop::<UserEvent>::with_user_event()
        .build()
        .expect("event loop");
    let mut app = App::new(event_loop.create_proxy());
    app.pending_packages = Some(packages);
    event_loop.run_app(&mut app).expect("run app");
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
