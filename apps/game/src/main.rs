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
//! Escape for the pause menu.

mod achievement_ui;
mod achievements;
mod ambience;
mod building_ui;
mod create_bar;
mod entity_ui;
mod event_notify;
mod gameplay;
mod global;
mod highlight;
mod keymap;
mod loops;
mod menu;
mod models;
mod music;
mod options;
mod pause_menu;
mod placement_tool;
mod removal_tool;
mod render;
mod scenario_ui;
mod scene;
mod scene_assets;
mod scene_gfx;
mod select_resource_ui;
mod selection;
mod selection_overlay;
mod settings;
mod shadow;
mod sound;
mod start;
mod status_icons;
mod toolbar;
mod town_hall_ui;
mod trade_ui;
mod utility;
#[cfg(target_arch = "wasm32")]
mod web;
mod world;

use std::rc::Rc;
use std::sync::Arc;

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
    audio: sound::Audio,
    global: Option<global::GlobalData>,
    rng: Random,
    main: MainLoop,
    /// `Application::IsPaused`: the window lost focus (`Application +0x11`, cleared on
    /// WM_KILLFOCUS, set on WM_SETFOCUS).
    app_paused: bool,
    /// The Options dialog's configurations (`IO::Persistence`).
    settings: settings::Settings,
    /// The display's scale factor (1.0 for captures). Deviation: the UI is drawn at
    /// `os_scale × UIScale / 100` so a high-density display does not shrink it; the original
    /// knows only `UIScale` (`UserInterface +0xdc`).
    os_scale: f32,
    /// The pointer in window pixels, once it has moved.
    pointer: Option<(i32, i32)>,
    /// The software pointer (`Cursor/Arrow.rsc:cursor`): its sheet and hotspot.
    cursor: Option<(banished_ui::SpriteRef, [i32; 2])>,
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
    // Scripted capture override: fix the separate engine/menu stream (town names, menu scene)
    // as well as the world seed. Normal System::Random() still seeds from the clock.
    if let Ok(seed) = std::env::var("BANISHED_SEED").unwrap_or_default().parse() {
        return seed;
    }
    let ms = web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    ms as u32
}

impl Game {
    /// A game using the player's stored settings.
    pub fn new(packages: Vec<(String, Vec<u8>)>) -> Result<Game, String> {
        Self::with_settings(packages, settings::Settings::load())
    }

    pub fn with_settings(
        packages: Vec<(String, Vec<u8>)>,
        settings: settings::Settings,
    ) -> Result<Game, String> {
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
            audio: sound::Audio::default(),
            global: None,
            // `System::Random()` seeds from the clock.
            rng: Random::new(clock_seed()),
            main: boot_chain(),
            app_paused: false,
            settings,
            os_scale: 1.0,
            pointer: None,
            cursor: None,
        })
        .map(|mut g: Game| {
            g.apply_settings();
            g
        })
    }

    /// The configurations' effects inside the game: the volumes (`Audio::Configuration`).
    fn apply_settings(&mut self) {
        let a = self.settings.audio;
        self.audio.scene.volumes = banished_audio::scene::Volumes {
            ui: a.ui,
            music: a.music,
            ambient: a.ambient,
            effect: a.effect,
        };
    }

    /// "Use Software Mouse" (`Input::Configuration +10`): the pointer is drawn by the game, at
    /// its image's own size, instead of by the system (`UserInterface::SetCursor` hands the
    /// cursor's sheet image and hotspot to the device). Not ported: the other cursors (Hand,
    /// Place, Move, …) the original switches between by `UserInterface::GetMouseCursorIndex`;
    /// only the arrow is drawn.
    fn cursor_quad(&mut self) -> Option<banished_ui::Quad> {
        let (x, y) = self.pointer?;
        if self.cursor.is_none() {
            let data = self.archive.read_ref("Cursor/Arrow.rsc:cursor").ok()?;
            let c = banished_formats::ui::cursor(&data).ok()?;
            let sheet_ref = c.sprite_sheet?.replace('\\', "/");
            let sheet = self.ui.res.sheet(&sheet_ref)?;
            let loaded = &self.ui.res.sheets[sheet].sheet;
            let sprite = banished_ui::SpriteRef {
                sheet,
                width: loaded.width,
                height: loaded.height,
                uv: loaded
                    .sprites
                    .first()
                    .map_or([0.0, 0.0, 1.0, 1.0], |s| s.uv),
            };
            self.cursor = Some((sprite, c.hotspot));
        }
        let (sprite, [hx, hy]) = self.cursor?;
        let (x, y) = ((x - hx) as f32, (y - hy) as f32);
        Some(banished_ui::Quad {
            texture: banished_ui::Texture::Sheet(sprite.sheet),
            rect: [x, y, x + sprite.width as f32, y + sprite.height as f32],
            uv: sprite.uv,
            color: [255; 4],
            clip: None,
            rotation: 0.0,
        })
    }

    /// The display's scale factor (`Window::scale_factor`).
    pub fn set_os_scale(&mut self, scale: f32) {
        self.os_scale = if scale.is_finite() && scale > 0.0 {
            scale
        } else {
            1.0
        };
    }

    /// Physical pixels per UI unit (`UserInterface::UIScaled`).
    pub fn ui_scale(&self) -> f32 {
        (self.os_scale * self.settings.game.ui_scale as f32 / 100.0).max(0.25)
    }

    /// The UI's size in its own units for a window of `size` physical pixels
    /// (`UserInterface::UIUnscaled`).
    pub fn logical(&self, size: (u32, u32)) -> (u32, u32) {
        let s = self.ui_scale();
        (
            (size.0 as f32 / s).round() as u32,
            (size.1 as f32 / s).round() as u32,
        )
    }

    fn with_ctx<R>(&mut self, f: impl FnOnce(&mut MainLoop, &mut Ctx) -> R) -> R {
        let mut ctx = Ctx {
            archive: &self.archive,
            ui: &mut self.ui,
            audio: &mut self.audio,
            global: &mut self.global,
            rng: &mut self.rng,
            app_paused: self.app_paused,
            settings: &mut self.settings,
        };
        f(&mut self.main, &mut ctx)
    }

    pub fn update(&mut self, dt: f32) {
        self.with_ctx(|main, ctx| main.update(ctx, dt));
        // The Options dialog changes the volumes while its sliders move.
        self.apply_settings();
    }

    fn input_root(&self) -> Option<usize> {
        self.main.current().screen().map(|s| s.input_root())
    }

    fn raw(&mut self, input: RawInput) {
        self.with_ctx(|main, ctx| main.raw(ctx, input));
    }

    fn deliver_events(&mut self) {
        self.with_ctx(|main, ctx| main.events(ctx));
        self.ui_sounds();
    }

    /// `UserInterface::Play` requests from the UI (button down, click, drop).
    fn ui_sounds(&mut self) {
        for slot in std::mem::take(&mut self.ui.sounds) {
            self.audio.play_ui(&self.archive, slot);
        }
    }

    /// The pointer at window pixel `(x, y)`.
    pub fn pointer_move(&mut self, x: i32, y: i32) {
        self.pointer = Some((x, y));
        let s = self.ui_scale();
        self.pointer_move_ui((x as f32 / s) as i32, (y as f32 / s) as i32);
    }

    /// The pointer at UI position `(x, y)`.
    fn pointer_move_ui(&mut self, x: i32, y: i32) {
        if let Some(root) = self.input_root() {
            self.ui.pointer_move(x, y, root);
        }
        self.raw(RawInput::Cursor(x, y));
    }

    pub fn pointer_button(&mut self, down: bool) -> bool {
        self.audio.unlock();
        if let Some(root) = self.input_root() {
            let captured = self.ui.pointer_over_interactive();
            if let Some(event) = self.ui.pointer_button(down, root) {
                self.with_ctx(|main, ctx| main.inject(ctx, event));
            }
            self.deliver_events();
            if down && !captured {
                self.raw(RawInput::Click);
            } else if !down {
                self.raw(RawInput::Release);
            }
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

    /// A key pressed: `key` for the UI (focus, edit boxes, shortcuts), `code` the engine's
    /// keyboard code for the key map.
    pub fn key(&mut self, key: Option<banished_ui::Key>, text: Option<&str>, code: Option<u8>) {
        self.audio.unlock();
        let handled = match (self.input_root(), key) {
            (Some(root), Some(key)) => {
                let handled = self.ui.key(root, key);
                if !handled {
                    if let Some(text) = text.filter(|t| t.chars().all(|c| !c.is_control())) {
                        self.ui.text_input(text);
                    }
                }
                self.deliver_events();
                handled || self.ui.text_edit_focused()
            }
            _ => false,
        };
        if handled {
            return;
        }
        let Some(code) = code else { return };
        let actions: Vec<usize> = self
            .settings
            .keymap
            .actions_for((keymap::KEYBOARD, code))
            .collect();
        for id in actions {
            use keymap::id;
            let raw = match id {
                id::CANCEL => RawInput::Escape,
                id => RawInput::Action(id),
            };
            self.raw(raw);
        }
    }

    /// A key or mouse button released (`Input::Device::Released`), for the key-binding box.
    pub fn released(&mut self, binding: keymap::Binding) {
        self.raw(RawInput::Released(binding));
        let actions: Vec<usize> = self.settings.keymap.actions_for(binding).collect();
        for id in actions {
            self.raw(RawInput::ActionReleased(id));
        }
    }

    /// Click the named control through the normal pointer path (scripted runs): lay out the
    /// screen at `size`, move to the control's centre, press and release.
    pub fn press(&mut self, name: &str, size: (u32, u32)) -> bool {
        let size = self.logical(size);
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
        self.pointer_move_ui((x0 + x1) / 2, (y0 + y1) / 2);
        self.pointer_button(true);
        self.pointer_button(false);
        true
    }

    /// Scripted runs: select the combo item whose text is `item` (case-insensitive), as a
    /// player would, so captures can reproduce New Game settings chosen in the original.
    /// Run a loop script command (captures).
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    pub fn script(&mut self, command: &str) -> bool {
        self.with_ctx(|main, ctx| main.current_mut().script(ctx, command))
    }

    pub fn select_combo(&mut self, name: &str, item: &str, size: (u32, u32)) -> bool {
        let size = self.logical(size);
        let Some(screen) = self.main.current().screen() else {
            return false;
        };
        screen.layout(&mut self.ui, size);
        let Some(combo) = screen
            .dialogs
            .iter()
            .rev()
            .find_map(|d| self.ui.find(d.root, name))
        else {
            return false;
        };
        let Some(list) = crate::menu::combo_list(&self.ui, combo) else {
            return false;
        };
        let index = match &self.ui.node(list).kind {
            banished_ui::Kind::List(data) => {
                data.items.iter().position(|t| t.eq_ignore_ascii_case(item))
            }
            _ => None,
        };
        let Some(index) = index else { return false };
        crate::menu::select_combo(&mut self.ui, combo, index);
        true
    }

    pub fn quit_requested(&self) -> bool {
        self.main.quit
    }

    /// Lay out the current loop's UI and build the frame.
    pub fn draw(&mut self, renderer: &mut Renderer, capture: bool) -> Option<Vec<u8>> {
        let size = renderer.size();
        let logical = self.logical(size);
        if let Some(screen) = self.main.current().screen() {
            screen.layout(&mut self.ui, logical);
        }
        let mut quads = self
            .main
            .current()
            .screen()
            .map(|s| self.ui.draw(s.root))
            .unwrap_or_default();
        scale_quads(&mut quads, self.ui_scale());
        quads.extend(loops::fade_quad(self.main.fade.level, size));
        if self.settings.input.mouse_display {
            quads.extend(self.cursor_quad());
        }
        renderer.set_ui_time(self.main.elapsed);
        let g = &self.settings.game;
        renderer.icon_options = status_icons::IconOptions {
            opacity: g.status_icon_opacity,
            enabled: g.status_icon_enable,
            scale: g.status_icon_scale,
        };
        let (terrain, clear) = match self.main.current().view() {
            View::World(view) => {
                renderer.set_scene(view);
                renderer.update_minimap(view);
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

/// UI units to window pixels (`UserInterface::UIScaled` at draw time).
fn scale_quads(quads: &mut [banished_ui::Quad], s: f32) {
    if s == 1.0 {
        return;
    }
    for q in quads {
        q.rect = q.rect.map(|v| v * s);
        if let Some(c) = &mut q.clip {
            *c = c.map(|v| (v as f32 * s).round() as i32);
        }
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

/// The window's fullscreen state for a video configuration: exclusive when a monitor mode
/// matches the chosen resolution and refresh rate, otherwise borderless on the current monitor.
#[cfg(not(target_arch = "wasm32"))]
fn fullscreen(window: &Window, video: &settings::VideoConfig) -> Option<winit::window::Fullscreen> {
    use winit::window::Fullscreen;
    if !video.fullscreen {
        return None;
    }
    let monitor = window.current_monitor();
    let mode = monitor.as_ref().and_then(|m| {
        m.video_modes().find(|v| {
            v.size().width as i32 == video.width
                && v.size().height as i32 == video.height
                && v.refresh_rate_millihertz() as i64 * video.refresh_d.max(1) as i64
                    == video.refresh_n as i64 * 1000
        })
    });
    Some(match mode {
        Some(m) => Fullscreen::Exclusive(m),
        None => Fullscreen::Borderless(monitor),
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
    /// The stored settings until the game takes them over.
    settings: Option<settings::Settings>,
    /// The settings revision whose video and input configuration the window shows.
    applied: u32,
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
            settings: Some(settings::Settings::load()),
            applied: 0,
        }
    }

    fn settings(&self) -> Option<&settings::Settings> {
        self.game
            .as_ref()
            .map(|g| &g.settings)
            .or(self.settings.as_ref())
    }

    fn settings_mut(&mut self) -> Option<&mut settings::Settings> {
        match &mut self.game {
            Some(g) => Some(&mut g.settings),
            None => self.settings.as_mut(),
        }
    }

    /// Build a renderer for the window with the configured interface and adapter
    /// (`Video::Device::Startup`); it arrives as `UserEvent::Renderer`.
    fn start_renderer(&self) {
        let (Some(window), Some(settings)) = (self.window.clone(), self.settings()) else {
            return;
        };
        let video = render::VideoSettings::from(&settings.video);
        let proxy = self.proxy.clone();
        let init = async move {
            let renderer = Renderer::new(window, video).await;
            let _ = proxy.send_event(UserEvent::Renderer(Box::new(renderer)));
        };
        #[cfg(not(target_arch = "wasm32"))]
        pollster::block_on(init);
        #[cfg(target_arch = "wasm32")]
        wasm_bindgen_futures::spawn_local(init);
    }

    /// The display modes of the window's monitor, largest first (`Adapter::FindDisplayMode`
    /// lists). The browser offers only the canvas size.
    fn display_modes(&self) -> Vec<settings::DisplayMode> {
        let mut modes: Vec<settings::DisplayMode> = Vec::new();
        let Some(window) = &self.window else {
            return modes;
        };
        if let Some(monitor) = window.current_monitor() {
            for m in monitor.video_modes() {
                let (w, h) = (m.size().width as i32, m.size().height as i32);
                let rate = (m.refresh_rate_millihertz() as i32, 1000);
                match modes.iter_mut().find(|d| d.width == w && d.height == h) {
                    Some(d) if !d.refresh.contains(&rate) => d.refresh.push(rate),
                    Some(_) => {}
                    None => modes.push(settings::DisplayMode {
                        width: w,
                        height: h,
                        refresh: vec![rate],
                    }),
                }
            }
        }
        if modes.is_empty() {
            let size = window.inner_size();
            let rate = window
                .current_monitor()
                .and_then(|m| m.refresh_rate_millihertz())
                .map_or((60, 1), |r| (r as i32, 1000));
            modes.push(settings::DisplayMode {
                width: size.width as i32,
                height: size.height as i32,
                refresh: vec![rate],
            });
        }
        modes.sort_by_key(|m| (m.width, m.height));
        for m in &mut modes {
            m.refresh.sort_by_key(|&(n, d)| n / d.max(1));
        }
        modes
    }

    /// `Video::Device::ApplyChangedSettings` and `Input::Device::EnableMouseClipping` for the
    /// current settings: fullscreen, window size, present mode, cursor clipping; a new renderer
    /// when the interface, adapter, filtering or shadow resolution changed.
    fn apply_settings(&mut self) {
        let (Some(window), Some(settings)) = (self.window.clone(), self.settings()) else {
            return;
        };
        let video = settings.video.clone();
        let clip = settings.input.mouse_clipping;
        let mouse_display = settings.input.mouse_display;
        self.applied = settings.revision;
        // Deviation: the browser keeps its canvas; Fullscreen and the resolution apply natively.
        #[cfg(not(target_arch = "wasm32"))]
        {
            window.set_fullscreen(fullscreen(&window, &video));
            if !video.fullscreen && video.window_width > 0 && video.window_height > 0 {
                let size = winit::dpi::PhysicalSize::new(
                    video.window_width as u32,
                    video.window_height as u32,
                );
                if window.inner_size() != size {
                    let _ = window.request_inner_size(size);
                }
            }
        }
        // Software mouse: the game draws the pointer, so the system's is hidden.
        window.set_cursor_visible(!mouse_display);
        use winit::window::CursorGrabMode;
        let grab = if clip {
            window
                .set_cursor_grab(CursorGrabMode::Confined)
                .or_else(|_| window.set_cursor_grab(CursorGrabMode::Locked))
        } else {
            window.set_cursor_grab(CursorGrabMode::None)
        };
        if let Err(e) = grab {
            log::warn!("mouse clipping: {e}");
        }
        let next = render::VideoSettings::from(&video);
        let restart = self
            .renderer
            .as_ref()
            .is_some_and(|r| r.video.needs_restart(&next));
        if restart {
            log::info!("video settings changed: restarting the renderer");
            self.renderer = None;
            self.start_renderer();
        } else if let Some(r) = &mut self.renderer {
            r.apply_video(next);
        }
    }

    fn try_start(&mut self) {
        if self.renderer.is_none() {
            return;
        }
        let Some(packages) = self.pending_packages.take() else {
            return;
        };
        let settings = self
            .settings
            .take()
            .unwrap_or_else(settings::Settings::load);
        match Game::with_settings(packages, settings) {
            Ok(mut game) => {
                if let Some(w) = &self.window {
                    game.set_os_scale(w.scale_factor() as f32);
                }
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
            let video = self.settings().map(|s| s.video.clone()).unwrap_or_default();
            attrs = if video.window_width > 0 && video.window_height > 0 {
                attrs.with_inner_size(winit::dpi::PhysicalSize::new(
                    video.window_width as u32,
                    video.window_height as u32,
                ))
            } else {
                attrs.with_inner_size(winit::dpi::LogicalSize::new(1280.0, 800.0))
            };
        }
        #[cfg(target_arch = "wasm32")]
        {
            use winit::platform::web::WindowAttributesExtWebSys;
            attrs = attrs.with_canvas(Some(web::canvas()));
        }
        let window = Arc::new(event_loop.create_window(attrs).expect("create window"));
        window.set_ime_allowed(true);
        self.window = Some(window);
        self.start_renderer();
    }

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::Renderer(mut renderer) => {
                if let Some(w) = &self.window {
                    let size = w.inner_size();
                    renderer.resize(size.width, size.height);
                }
                log::info!("GPU: {}", renderer.backend);
                let modes = self.display_modes();
                for interface in &mut renderer.display.interfaces {
                    for adapter in &mut interface.adapters {
                        adapter.modes = modes.clone();
                    }
                }
                renderer.display.desktop = self
                    .window
                    .as_ref()
                    .and_then(|w| w.current_monitor())
                    .map(|m| (m.size().width as i32, m.size().height as i32));
                if let Some(s) = self.settings_mut() {
                    s.display = renderer.display.clone();
                }
                let first = self.renderer.is_none() && self.game.is_none();
                self.renderer = Some(*renderer);
                if first {
                    self.apply_settings();
                }
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
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                if let Some(game) = &mut self.game {
                    game.set_os_scale(scale_factor as f32);
                }
            }
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
                if game.settings.revision != self.applied {
                    self.apply_settings();
                }
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
            WindowEvent::Focused(focused) => {
                if !focused {
                    self.drag_button = None;
                }
                if let Some(game) = &mut self.game {
                    game.app_paused = !focused;
                }
            }
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
                // After the UI has seen the release, so a click on the binding box's buttons
                // wins over binding the button.
                if let (false, Some(code)) = (down, keymap::mouse_code(button)) {
                    game.released((keymap::MOUSE, code));
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
            WindowEvent::KeyboardInput { event, .. } => {
                let Some(game) = &mut self.game else { return };
                let code = match event.physical_key {
                    winit::keyboard::PhysicalKey::Code(c) => keymap::keyboard_code(&c),
                    _ => None,
                };
                if event.state == ElementState::Pressed {
                    let key = ui_key(&event.logical_key, self.shift);
                    game.key(key, event.text.as_deref(), code);
                } else if let Some(code) = code {
                    game.released((keymap::KEYBOARD, code));
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
        // Captures ignore the player's stored settings; `--ui-scale PERCENT` sets UIScale.
        let mut settings = crate::settings::Settings::default();
        if let Some(v) = args
            .windows(2)
            .find(|w| w[0] == "--ui-scale")
            .and_then(|w| w[1].parse().ok())
        {
            settings.game.ui_scale = v;
        }
        // `--setting KEY=VALUE` sets a stored setting by its original name (`InputMouseDisplay=1`,
        // `VideoReflectionDetail=2`, `InputForward=297`, …).
        let stored: Vec<&str> = args
            .windows(2)
            .filter(|w| w[0] == "--setting")
            .map(|w| w[1].as_str())
            .collect();
        if !stored.is_empty() {
            settings.persistence.parse(&stored.join("\n"));
            let p = &settings.persistence;
            settings.video = crate::settings::VideoConfig::load(p);
            settings.audio = crate::settings::AudioConfig::load(p);
            settings.input = crate::settings::InputConfig::load(p);
            settings.game = crate::settings::GameOptions::load(p);
            settings.keymap = crate::keymap::KeyMap::load(p);
        }
        let mut game =
            Game::with_settings(read_packages(&data_dir(args)), settings).expect("game data");
        game.audio.set_master_volume(0.0);
        // `--capture-size WxH` matches a reference frame's resolution (default 1280x800).
        let size = args
            .windows(2)
            .find(|w| w[0] == "--capture-size")
            .and_then(|w| {
                let (a, b) = w[1].split_once('x')?;
                Some((a.parse().ok()?, b.parse().ok()?))
            })
            .unwrap_or((1280, 800));
        let mut renderer = pollster::block_on(Renderer::headless_with(
            size.0,
            size.1,
            render::VideoSettings::from(&game.settings.video),
        ));
        // The Options dialog lists the capture device with one mode, the capture size.
        game.settings.display = renderer.display.clone();
        for adapter in game
            .settings
            .display
            .interfaces
            .iter_mut()
            .flat_map(|i| i.adapters.iter_mut())
        {
            adapter.modes = vec![settings::DisplayMode {
                width: size.0 as i32,
                height: size.1 as i32,
                refresh: vec![(60, 1)],
            }];
        }
        // `--press NAME@SECONDS` clicks a control at a given time.
        let mut presses: Vec<(String, f32)> = args
            .windows(2)
            .filter(|w| w[0] == "--press")
            .filter_map(|w| {
                let (name, at) = w[1].split_once('@')?;
                Some((name.to_string(), at.parse().ok()?))
            })
            .collect();
        // `--move X,Y@SECONDS` moves the cursor; `--click-at X,Y@SECONDS` also presses and
        // releases the left button there.
        let mut pointer: Vec<(bool, i32, i32, f32)> = args
            .windows(2)
            .filter(|w| w[0] == "--move" || w[0] == "--click-at")
            .filter_map(|w| {
                let (xy, at) = w[1].split_once('@')?;
                let (x, y) = xy.split_once(',')?;
                Some((
                    w[0] == "--click-at",
                    x.parse().ok()?,
                    y.parse().ok()?,
                    at.parse().ok()?,
                ))
            })
            .collect();
        // `--wheel STEPS@SECONDS` scrolls the camera.
        let mut wheels: Vec<(f32, f32)> = args
            .windows(2)
            .filter(|w| w[0] == "--wheel")
            .filter_map(|w| {
                let (steps, at) = w[1].split_once('@')?;
                Some((steps.parse().ok()?, at.parse().ok()?))
            })
            .collect();
        // `--combo NAME=ITEM@SECONDS` selects a combo item by its text (New Game settings).
        let mut combos: Vec<(String, String, f32)> = args
            .windows(2)
            .filter(|w| w[0] == "--combo")
            .filter_map(|w| {
                let (sel, at) = w[1].rsplit_once('@')?;
                let (name, item) = sel.split_once('=')?;
                Some((name.to_string(), item.to_string(), at.parse().ok()?))
            })
            .collect();
        // `--event KIND[:CELL]@SECONDS` posts an event (event ribbon and log captures);
        // `--script "COMMAND@SECONDS"` runs a loop script command (`open TEMPLATE`).
        let mut events: Vec<(String, f32)> = args
            .windows(2)
            .filter_map(|w| {
                let (cmd, at) = w[1].rsplit_once('@')?;
                let cmd = match w[0].as_str() {
                    "--event" => format!("event {}", cmd.replace(':', " ")),
                    "--script" => cmd.to_string(),
                    _ => return None,
                };
                Some((cmd, at.parse().ok()?))
            })
            .collect();
        let dt = 1.0 / 30.0;
        let mut t = 0.0;
        while t < seconds {
            game.update(dt);
            t += dt;
            combos.retain(|(name, item, at)| {
                if t < *at {
                    return true;
                }
                if !game.select_combo(name, item, size) {
                    log::warn!("--combo {name}={item}: not found at {t:.2} s");
                }
                false
            });
            events.retain(|(command, at)| {
                if t < *at {
                    return true;
                }
                if !game.script(command) {
                    log::warn!("--event {command}: not in a game at {t:.2} s");
                }
                false
            });
            wheels.retain(|&(steps, at)| {
                if t < at {
                    return true;
                }
                game.wheel(steps);
                false
            });
            pointer.retain(|&(click, x, y, at)| {
                if t < at {
                    return true;
                }
                let logical = game.logical(size);
                if let Some(screen) = game.main.current().screen() {
                    screen.layout(&mut game.ui, logical);
                }
                game.pointer_move(x, y);
                if click {
                    game.pointer_button(true);
                    game.pointer_button(false);
                }
                false
            });
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
