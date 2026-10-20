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
//! On the map: middle drag to rotate, right click to cancel the tool, wheel to zoom,
//! WASD/arrows to move, Escape for the pause menu.

mod achievements;
mod audio;
mod gameplay;
mod gfx;
mod global;
mod keymap;
mod loops;
mod menu;
mod mods;
mod platform;
mod saves;
mod settings;
mod start;
mod tools;
mod ui;
mod view;

use std::rc::Rc;
use std::sync::Arc;

use banished_formats::pkg::{Archive, Package};
use banished_game::random::Random;
use banished_ui::Ui;
use web_time::Instant;

/// The browser's resolution choices: parts of the page's size the scene is drawn at.
#[cfg(target_arch = "wasm32")]
const WEB_SCENE_SCALES: [f32; 3] = [1.0, 0.75, 0.5];
/// The browser's frame-rate limits, in frames per second.
#[cfg(target_arch = "wasm32")]
const WEB_FRAME_RATES: [i32; 4] = [30, 60, 90, 120];
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

use gfx::render::{Frame, Renderer};
use loops::{Ctx, DialogLoaderLoop, LoaderLoop, MainLoop, RawInput, View};

pub enum UserEvent {
    Renderer(Box<Renderer>),
    Packages(Vec<(String, Vec<u8>)>),
    /// Mods (`(file name without .pkm, bytes)`): those stored in the browser when the page
    /// starts, then each one the player adds.
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    Mods(Vec<(String, Vec<u8>)>),
    /// A save to run for `?digest=` (`gameplay::digest`), with its update count and interval.
    #[cfg(target_arch = "wasm32")]
    Digest {
        save: Vec<u8>,
        updates: u32,
        every: u32,
    },
}

const CLEAR: wgpu::Color = wgpu::Color {
    r: 0.0,
    g: 0.0,
    b: 0.0,
    a: 1.0,
};

/// `Engine::MainLoop`'s update step and the most time it catches up (`DAT_180164818`,
/// `DAT_180164820`: 1/60 s and 1/15 s, as ticks).
const FRAME_STEP: i64 = 16_666;
const FRAME_MAX: i64 = 66_666;
// Deviation: the original also draws between steps (`MainLoop::Frame`'s `+0x38 / +0x48`
// handed to `Renderer::Draw`, models and poses blending their `InterpolatedState`); the port
// draws the last step, which differs only on displays faster than 60 Hz.

/// The running game: shared resources plus the loop chain.
pub struct Game {
    archive: Rc<Archive>,
    /// The game's own packages, before the mods (`Archive::remove_after`).
    base_packages: usize,
    /// The resources loaded at startup (`formats::registry`).
    registry: Rc<banished_formats::registry::Registry>,
    /// The mods (`IO::Modification`) and the folder they were found in.
    mods: mods::Modification,
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    pack_dir: Option<std::path::PathBuf>,
    /// A reload was asked for, and the UI's sheets to upload again after one.
    reload_mods: Option<mods::Reload>,
    ui_reset: bool,
    ui: Ui,
    audio: audio::sound::Audio,
    global: Option<global::GlobalData>,
    rng: Random,
    main: MainLoop,
    /// A save's frame and minimap being read back (`SaveLoad::Draw`).
    save_capture: Option<gfx::render::PendingCapture>,
    /// A screenshot being read back (`FUN_14009b270`).
    screenshot: Option<gfx::render::PendingCapture>,
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
    /// The software pointers (`FUN_1400a0e00`'s table, `Global +0x28`): Arrow, Hand,
    /// Hourglass, Move, Query and Place, each its sheet and hotspot once loaded.
    cursors: Vec<Option<(banished_ui::SpriteRef, [i32; 2])>>,
    /// A capture's cinematic camera this frame: eye, target and vertical field of view
    /// ([`view::cinematic`]). Deviation: not in the original; drawing only, set by captures.
    cinematic: Option<(glam::Vec3, glam::Vec3, f32)>,
}

/// `SetupFileSystem` and `LoadResources`: the enabled mods join the packages (from the last
/// in the list to the first), then the game's resources and each mod's resource list load.
fn load_mods(
    archive: &mut Archive,
    mods: &mut mods::Modification,
) -> banished_formats::registry::Registry {
    mods.dirty = false;
    archive.user_packs = mods.load_order().len();
    for m in mods.load_order() {
        match Package::parse(m.bytes.as_ref().clone()) {
            Ok(p) => {
                log::info!("mod {} ({}.pkm): {} files", m.name, m.file, p.entries.len());
                archive.add_mod(p);
            }
            Err(e) => log::warn!("{}.pkm: {e}", m.file),
        }
    }
    let roots = banished_formats::registry::roots(&mods.enabled_files());
    banished_formats::registry::Registry::build(archive, &roots)
}

/// `Cursor/{name}.rsc:cursor`, in cursor index order.
const CURSORS: [&str; 6] = ["Arrow", "Hand", "Hourglass", "Move", "Query", "Place"];

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

/// After a mod reload: the game's resources again ("Game/Global"), the loading screen, then
/// `next` (`FUN_1400b9c60` from the menu, `FUN_1400b9ec0` in a game).
fn reload_chain(next: Box<dyn loops::GameLoop>) -> MainLoop {
    let loading = Box::new(DialogLoaderLoop::new(
        "Dialog/Loading.rsc:resource".into(),
        0.5,
        next,
    ));
    MainLoop::new(Box::new(LoaderLoop::new(
        "Game/Global",
        |ctx: &mut Ctx| match global::GlobalData::load(ctx.archive) {
            Ok(g) => *ctx.global = Some(g),
            Err(e) => log::error!("global game data: {e}"),
        },
        loading,
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
        Self::with_mods(packages, Vec::new(), None, settings)
    }

    /// The game with the mods found (`(file without .pkm, bytes)`) in `pack_dir`: the
    /// enabled ones are loaded (`Modification::RefreshList`, `SetupFileSystem`).
    pub fn with_mods(
        packages: Vec<(String, Vec<u8>)>,
        found: Vec<(String, Vec<u8>)>,
        pack_dir: Option<std::path::PathBuf>,
        settings: settings::Settings,
    ) -> Result<Game, String> {
        let mut archive = Archive::default();
        for (name, bytes) in packages {
            archive.add(Package::parse(bytes).map_err(|e| format!("{name}: {e}"))?);
        }
        if archive.is_empty() {
            return Err("no packages".into());
        }
        let base_packages = archive.package_count();
        let mut mods = mods::Modification::refresh(found, &settings.persistence);
        let registry = Rc::new(load_mods(&mut archive, &mut mods));
        mods::set_registry(registry.clone());
        let archive = Rc::new(archive);
        Ok(Game {
            base_packages,
            registry,
            mods,
            pack_dir,
            reload_mods: None,
            ui_reset: false,
            ui: Ui::new(archive.clone()),
            archive,
            audio: audio::sound::Audio::default(),
            global: None,
            // `System::Random()` seeds from the clock.
            rng: Random::new(clock_seed()),
            main: boot_chain(),
            save_capture: None,
            cinematic: None,
            screenshot: None,
            app_paused: false,
            settings,
            os_scale: 1.0,
            pointer: None,
            cursors: Vec::new(),
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
    /// cursor's sheet image and hotspot to the device).
    fn cursor_quad(&mut self) -> Option<banished_ui::Quad> {
        let (x, y) = self.pointer?;
        if self.cursors.is_empty() {
            self.cursors = CURSORS
                .iter()
                .map(|name| {
                    let data = self
                        .archive
                        .read_ref(&format!("Cursor/{name}.rsc:cursor"))
                        .ok()?;
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
                    Some((sprite, c.hotspot))
                })
                .collect();
        }
        // The loop's cursor this frame (`UserInterface::SetCursor`), the arrow if it is missing.
        let index = self.main.current().cursor(&self.ui, self.ui.pointer());
        let cursor = self.cursors.get(index).copied().flatten();
        let cursor = cursor.or_else(|| self.cursors.first().copied().flatten());
        let (sprite, [hx, hy]) = cursor?;
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
            registry: &self.registry,
            mods: &mut self.mods,
            reload_mods: &mut self.reload_mods,
            pack_dir: self.pack_dir.as_deref(),
        };
        f(&mut self.main, &mut ctx)
    }

    pub fn update(&mut self, dt: f32) {
        self.with_ctx(|main, ctx| main.update(ctx, dt));
        match self.reload_mods.take() {
            Some(mods::Reload::Menu) => self.reload(Box::new(menu::MenuLoop::new())),
            Some(mods::Reload::Game(bytes)) => match saves::saved::SavedGame::parse(&bytes) {
                Ok(saved) => self.reload(Box::new(gameplay::GameplayLoop::from_save(saved))),
                Err(e) => {
                    log::error!("reload: the game saved to memory does not read back: {e}");
                    self.reload(Box::new(menu::MenuLoop::new()));
                }
            },
            None => {}
        }
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
            } else if down {
                self.raw(RawInput::UiPress);
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

    /// A mouse button other than the left pressed: its key-map actions.
    pub fn pressed_mouse(&mut self, code: u8) {
        let actions: Vec<usize> = self
            .settings
            .keymap
            .actions_for((keymap::MOUSE, code))
            .collect();
        for id in actions {
            self.raw(RawInput::Action(id));
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

    /// Click the named edit, empty it and type `text` (scripted runs).
    pub fn type_into(&mut self, name: &str, text: &str, size: (u32, u32)) -> bool {
        if !self.press(name, size) {
            return false;
        }
        let Some(edit) = self.ui.focused() else {
            return false;
        };
        self.ui.set_text(edit, "");
        self.ui.text_input(text)
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
    /// Mods added while the game runs (dropped in the browser): the list is read again with
    /// them, replacing any of the same name. They take effect after a reload (the Mods dialog
    /// asks for one when an enabled mod changed).
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    fn add_mods(&mut self, found: Vec<(String, Vec<u8>)>) {
        let count = found.len();
        if count > 0 {
            self.mods.add(found, &self.settings.persistence);
            log::info!("added {count} mods");
        }
    }

    /// `FUN_140072110`'s reload: the mods as the list has them now. `SaveList`, then the
    /// packages close and load again (`SetupFileSystem`), the game's resources reload
    /// (`Game/Global`) and `next` starts behind the loading screen.
    fn reload(&mut self, next: Box<dyn loops::GameLoop>) {
        self.mods.save_list(&mut self.settings.persistence);
        self.settings.save();
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(dir) = &self.pack_dir {
            self.mods = mods::Modification::refresh(mods::find(dir), &self.settings.persistence);
        }
        // Everything holding the packages lets go of them first.
        self.main = MainLoop::new(Box::new(menu::MenuLoop::new()));
        self.global = None;
        let empty = Rc::new(Archive::default());
        self.ui = Ui::new(empty.clone());
        let held = std::mem::replace(&mut self.archive, empty);
        let mut archive = match Rc::try_unwrap(held) {
            Ok(a) => a,
            Err(shared) => {
                log::error!("the packages are still in use; the mods are not reloaded");
                self.archive = shared;
                self.ui = Ui::new(self.archive.clone());
                return;
            }
        };
        archive.remove_after(self.base_packages);
        self.registry = Rc::new(load_mods(&mut archive, &mut self.mods));
        mods::set_registry(self.registry.clone());
        self.archive = Rc::new(archive);
        self.ui = Ui::new(self.archive.clone());
        self.cursors.clear();
        self.ui_reset = true;
        self.main = reload_chain(next);
        log::info!("reloaded with {} mods", self.mods.enabled_files().len());
    }

    pub fn draw(&mut self, renderer: &mut Renderer, capture: bool) -> Option<Vec<u8>> {
        if std::mem::take(&mut self.ui_reset) {
            renderer.reset_ui_sheets();
        }
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
        renderer.icon_options = gfx::status_icons::IconOptions {
            opacity: g.status_icon_opacity,
            enabled: g.status_icon_enable,
            scale: g.status_icon_scale,
        };
        let (terrain, clear) = match self.main.current().view() {
            View::World(view) => {
                renderer.set_scene(view);
                renderer.update_minimap(view);
                let fog = view.lighting.fog_color;
                let aspect = renderer.aspect();
                (
                    Some(match self.cinematic {
                        Some((eye, target, fov)) => {
                            view.frame_from(eye, target, fov, aspect, self.main.elapsed)
                        }
                        None => view.frame(aspect, self.main.elapsed),
                    }),
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
        let mut frame = Frame {
            terrain,
            clear,
            ui: None,
        };
        // A save's preview: the scene alone, cropped and scaled, read back over the next frames
        // with the minimap (`SaveLoad::Draw`).
        if frame.terrain.is_some()
            && self.save_capture.is_none()
            && self.main.current().wants_save_preview()
        {
            self.save_capture = Some(renderer.begin_capture(&frame));
        }
        if let Some(pending) = &self.save_capture {
            if let Some((rgba, minimap)) = renderer.poll_capture(pending) {
                self.save_capture = None;
                let (w, h) = renderer.size();
                let image = if rgba.len() == (w * h * 4) as usize {
                    crate::saves::saved::preview(&rgba, w, h)
                } else {
                    let (pw, ph) = crate::saves::saved::PREVIEW_SIZE;
                    vec![0; (pw * ph * 4) as usize]
                };
                self.main.current_mut().save_preview(image, minimap);
            }
        }
        // `FUN_14009b4e0`: the screenshot without the UI after the scene, with it after the UI.
        let shot = if self.screenshot.is_none() && frame.terrain.is_some() {
            self.main.current_mut().take_screenshot()
        } else {
            None
        };
        if shot == Some(false) {
            self.screenshot = Some(renderer.begin_capture(&frame));
        }
        frame.ui = Some((&self.ui, &quads));
        if shot == Some(true) {
            self.screenshot = Some(renderer.begin_capture(&frame));
        }
        if let Some(pending) = &self.screenshot {
            if let Some((rgba, _)) = renderer.poll_capture(pending) {
                self.screenshot = None;
                let (w, h) = renderer.size();
                if rgba.len() == (w * h * 4) as usize {
                    crate::gfx::screenshot::save(&rgba, w, h);
                }
            }
        }
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
        // Whole device pixels: the corner and the size are rounded, so a bitmap glyph or image
        // starts on a pixel and equal ones get equal widths. Unrounded, a fractional scale
        // (a pixel ratio of 2.625 on some tablets) put most glyphs between pixels, and the
        // linear filtering smeared them (issue #12).
        let [x0, y0, x1, y1] = q.rect;
        let (left, top) = ((x0 * s).round(), (y0 * s).round());
        q.rect = [
            left,
            top,
            left + ((x1 - x0) * s).round(),
            top + ((y1 - y0) * s).round(),
        ];
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
    /// The mods found beside them (`*.pkm`) and that folder.
    pending_mods: Option<(Vec<(String, Vec<u8>)>, Option<std::path::PathBuf>)>,
    /// `?digest=`: the save, updates and interval, waiting for the game to start.
    #[cfg(target_arch = "wasm32")]
    pending_digest: Option<(Vec<u8>, u32, u32)>,
    /// `?net=`: a networked test's joining player, once the game has started.
    #[cfg(target_arch = "wasm32")]
    net_session: Option<gameplay::network::JoinSession>,
    game: Option<Game>,
    last_frame: Option<Instant>,
    /// `Engine::MainLoop +0x38`: real time not yet stepped (ticks).
    accumulated: i64,
    cursor: (f64, f64),
    drag_button: Option<MouseButton>,
    shift: bool,
    /// The stored settings until the game takes them over.
    settings: Option<settings::Settings>,
    /// The settings revision whose video and input configuration the window shows.
    applied: u32,
    /// The browser's frame-rate limit (the Options refresh rate there): frames that come
    /// sooner after the last are skipped.
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    frame_interval: Option<std::time::Duration>,
}

impl App {
    fn new(proxy: EventLoopProxy<UserEvent>) -> App {
        App {
            proxy,
            window: None,
            renderer: None,
            pending_packages: None,
            pending_mods: None,
            #[cfg(target_arch = "wasm32")]
            pending_digest: None,
            #[cfg(target_arch = "wasm32")]
            net_session: None,
            game: None,
            last_frame: None,
            accumulated: 0,
            cursor: (0.0, 0.0),
            drag_button: None,
            shift: false,
            settings: Some(settings::Settings::load()),
            applied: 0,
            frame_interval: None,
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
        let video = gfx::render::VideoSettings::from(&settings.video);
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
    /// lists). Deviation: the browser offers the page's size at 100, 75 and 50 % (the scene
    /// is drawn at that size and scaled to the page, `Renderer::set_scene_scale`) and frame-rate
    /// limits of 30 to 120 per second (issue #15).
    fn display_modes(&self) -> Vec<settings::DisplayMode> {
        let mut modes: Vec<settings::DisplayMode> = Vec::new();
        let Some(window) = &self.window else {
            return modes;
        };
        #[cfg(target_arch = "wasm32")]
        {
            let size = window.inner_size();
            for part in WEB_SCENE_SCALES {
                modes.push(settings::DisplayMode {
                    width: (size.width as f32 * part).round() as i32,
                    height: (size.height as f32 * part).round() as i32,
                    refresh: WEB_FRAME_RATES.iter().map(|&r| (r, 1)).collect(),
                });
            }
            modes.sort_by_key(|m| (m.width, m.height));
        }
        #[cfg(not(target_arch = "wasm32"))]
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
        // The browser opens the game in its page, never full screen, so there the setting
        // starts unchecked until the player changes it in Options (issue #14).
        #[cfg(target_arch = "wasm32")]
        if self.applied == 0 {
            if let Some(s) = self.settings_mut() {
                s.video.fullscreen = false;
            }
        }
        let (Some(window), Some(settings)) = (self.window.clone(), self.settings()) else {
            return;
        };
        let video = settings.video.clone();
        let clip = settings.input.mouse_clipping;
        let mouse_display = settings.input.mouse_display;
        self.applied = settings.revision;
        // In the browser, the resolution is the scene's share of the page (snapped to one of
        // the offered parts), and the refresh rate a frame-rate limit.
        #[cfg(target_arch = "wasm32")]
        {
            let page = window.inner_size().width.max(1) as f32;
            let part = if video.width > 0 {
                let r = video.width as f32 / page;
                WEB_SCENE_SCALES
                    .into_iter()
                    .min_by(|a, b| (a - r).abs().total_cmp(&(b - r).abs()))
                    .unwrap_or(1.0)
            } else {
                1.0
            };
            if let Some(renderer) = &mut self.renderer {
                renderer.set_scene_scale(part);
            }
            let rate = video.refresh_n as f32 / video.refresh_d.max(1) as f32;
            self.frame_interval =
                (rate >= 1.0).then(|| std::time::Duration::from_secs_f32(0.97 / rate));
        }
        // Deviation: in the browser, Fullscreen asks the browser to show the page full screen
        // (winit waits for the next click or key if the browser needs one) instead of
        // changing the display mode.
        #[cfg(target_arch = "wasm32")]
        if video.fullscreen != window.fullscreen().is_some() {
            window.set_fullscreen(
                video
                    .fullscreen
                    .then_some(winit::window::Fullscreen::Borderless(None)),
            );
        }
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
        let next = gfx::render::VideoSettings::from(&video);
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
        // In the browser the stored mods are read first.
        #[cfg(target_arch = "wasm32")]
        if self.pending_mods.is_none() {
            return;
        }
        let Some(packages) = self.pending_packages.take() else {
            return;
        };
        let settings = self
            .settings
            .take()
            .unwrap_or_else(settings::Settings::load);
        let (found, pack_dir) = self.pending_mods.take().unwrap_or_default();
        match Game::with_mods(packages, found, pack_dir, settings) {
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
    platform::web::set_status(text, loaded);
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
            attrs = attrs.with_canvas(Some(platform::web::canvas()));
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
            #[cfg(target_arch = "wasm32")]
            UserEvent::Digest {
                save,
                updates,
                every,
            } => self.pending_digest = Some((save, updates, every)),
            UserEvent::Mods(found) => match &mut self.game {
                Some(game) => game.add_mods(found),
                None => {
                    let (list, _) = self.pending_mods.get_or_insert_with(Default::default);
                    for (name, bytes) in found {
                        list.retain(|(n, _)| n != &name);
                        list.push((name, bytes));
                    }
                }
            },
        }
        self.try_start();
        #[cfg(target_arch = "wasm32")]
        if let (Some(game), Some((save, updates, every))) = (&mut self.game, &self.pending_digest) {
            let text = match gameplay::digest::run(game, save, *updates, *every) {
                Ok(digests) => digests.iter().map(|d| format!("{d}\n")).collect(),
                Err(e) => format!("digest: {e}"),
            };
            self.pending_digest = None;
            platform::web::show_digest(&text);
        }
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
                // The browser's frame-rate limit: skip this frame if it came too soon.
                #[cfg(target_arch = "wasm32")]
                if let (Some(last), Some(min)) = (self.last_frame, self.frame_interval) {
                    if Instant::now() - last < min {
                        return;
                    }
                }
                // `?net=`: the networked test's player runs a few milliseconds each frame.
                #[cfg(target_arch = "wasm32")]
                {
                    if self.net_session.is_none() {
                        if let Some((relay, room, updates)) = platform::web::net_request() {
                            self.net_session =
                                Some(gameplay::network::JoinSession::new(&relay, &room, updates));
                        }
                    }
                    if let Some(s) = &mut self.net_session {
                        if !s.done {
                            s.step(game, std::time::Duration::from_millis(30));
                            let mut text = s.status.clone();
                            for l in &s.lines {
                                text.push('\n');
                                text.push_str(l);
                            }
                            platform::web::set_digest_text(&text);
                        }
                    }
                }
                platform::perf::frame_begin();
                let now = Instant::now();
                // `MainLoop::Run` updates once before its first frame.
                if self.last_frame.is_none() {
                    game.update(banished_game::scheduler::seconds(FRAME_STEP));
                }
                let elapsed = self.last_frame.map_or(0, |t| (now - t).as_micros() as i64);
                self.last_frame = Some(now);
                // `Engine::MainLoop::Frame` (Runtime `0x1800df940`): real time builds up and the
                // loop updates in whole steps of 1/60 s; more than 1/15 s waiting is cut to one
                // step (the constructor's `+0x48` and `+0x50`).
                self.accumulated += elapsed;
                if FRAME_MAX < self.accumulated {
                    self.accumulated = FRAME_STEP;
                }
                while FRAME_STEP <= self.accumulated {
                    let _p = platform::perf::scope("update");
                    game.update(banished_game::scheduler::seconds(FRAME_STEP));
                    self.accumulated -= FRAME_STEP;
                    if game.quit_requested() {
                        break;
                    }
                }
                if game.quit_requested() {
                    event_loop.exit();
                    return;
                }
                {
                    let _p = platform::perf::scope("draw");
                    game.draw(renderer, false);
                }
                if game.settings.revision != self.applied {
                    self.apply_settings();
                }
                platform::perf::frame_end();
            }
            WindowEvent::ModifiersChanged(m) => self.shift = m.state().shift_key(),
            WindowEvent::CursorMoved { position, .. } => {
                let (dx, dy) = (position.x - self.cursor.0, position.y - self.cursor.1);
                self.cursor = (position.x, position.y);
                if let Some(game) = &mut self.game {
                    game.pointer_move(position.x as i32, position.y as i32);
                    // `FUN_140104390`: the camera turns by the mouse's Δx while the button bound
                    // to `MouseMiddle` (action 7, the middle button by default) is held.
                    let rotates = self
                        .drag_button
                        .and_then(keymap::mouse_code)
                        .is_some_and(|c| {
                            game.settings
                                .keymap
                                .is(keymap::id::MOUSE_MIDDLE, (keymap::MOUSE, c))
                        });
                    if rotates {
                        game.drag(dx as f32, dy as f32);
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
                    _ => {
                        if down {
                            self.drag_button = Some(button);
                        } else if self.drag_button == Some(button) {
                            self.drag_button = None;
                        }
                    }
                }
                // The key map's actions for the button (`KeyMap::Pressed`), outside the UI:
                // `MouseRight` (action 6) cancels the tool (`FUN_140104bb0`).
                if let (true, false, Some(code)) = (
                    down,
                    button == MouseButton::Left,
                    keymap::mouse_code(button),
                ) {
                    game.pressed_mouse(code);
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
        crate::platform::paths::data_dir(args)
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
        let mut game = {
            let dir = data_dir(args);
            let found = crate::mods::find(&dir);
            Game::with_mods(read_packages(&dir), found, Some(dir), settings).expect("game data")
        };
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
            gfx::render::VideoSettings::from(&game.settings.video),
        ));
        renderer.blocking_readback = true;
        // `--scene-scale PART` draws the 3D scene at that part of the frame's size (the browser's
        // resolution setting), to check it without a browser.
        if let Some(part) = args
            .windows(2)
            .find(|w| w[0] == "--scene-scale")
            .and_then(|w| w[1].parse::<f32>().ok())
        {
            renderer.set_scene_scale(part);
        }
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
        // `--text NAME=TEXT@SECONDS` clicks the named edit, empties it and types TEXT.
        let mut texts: Vec<(String, String, f32)> = args
            .windows(2)
            .filter(|w| w[0] == "--text")
            .filter_map(|w| {
                let (what, at) = w[1].rsplit_once('@')?;
                let (name, text) = what.split_once('=')?;
                Some((name.to_string(), text.to_string(), at.parse().ok()?))
            })
            .collect();
        // `--move X,Y@SECONDS` moves the cursor; `--click-at X,Y@SECONDS` also presses and
        // releases the left button there; `--mouse-down` and `--mouse-up` move there and press
        // or release it, for drags (with `--move` between).
        let mut pointer: Vec<(&str, i32, i32, f32)> = args
            .windows(2)
            .filter(|w| {
                matches!(
                    w[0].as_str(),
                    "--move" | "--click-at" | "--mouse-down" | "--mouse-up"
                )
            })
            .filter_map(|w| {
                let (xy, at) = w[1].split_once('@')?;
                let (x, y) = xy.split_once(',')?;
                Some((
                    w[0].as_str(),
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
        // `--hold ACTION@START-END` holds the default key of a key-map action (`RotateLeft`,
        // `Forward`, `PitchUp`, …) from START to END seconds: camera moves in captures.
        let mut holds: Vec<(u8, f32, f32, bool)> = args
            .windows(2)
            .filter(|w| w[0] == "--hold")
            .filter_map(|w| {
                let (name, span) = w[1].split_once('@')?;
                let (start, end) = span.split_once('-')?;
                let code = keymap::default_key(name);
                if code.is_none() {
                    log::warn!("--hold {name}: no such keyboard action");
                }
                Some((code?, start.parse().ok()?, end.parse().ok()?, false))
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
        // `--camera`, `--camera-follow` and their settings: the cinematic camera (drawing only).
        let mut cinematic = crate::view::cinematic::Cinematic::from_args(args);
        let dt = 1.0 / 30.0;
        let mut t = 0.0;
        // `--realtime`: frames keep to the clock (a capture that talks to a relay).
        let realtime = args
            .iter()
            .any(|a| a == "--realtime")
            .then(std::time::Instant::now);
        // Frames left to draw for a scripted screenshot's readback.
        let mut shot_frames = 0;
        while t < seconds {
            if let Some(start) = realtime {
                let ahead = std::time::Duration::from_secs_f32(t).saturating_sub(start.elapsed());
                std::thread::sleep(ahead);
            }
            // A 1/30 s frame is two of `MainLoop::Frame`'s 1/60 s steps.
            let step = banished_game::scheduler::seconds(FRAME_STEP);
            platform::perf::frame_begin();
            for _ in 0..2 {
                let _p = platform::perf::scope("update");
                game.update(step);
            }
            t += dt;
            if let Some(c) = cinematic.as_mut() {
                game.cinematic = c.step(t, dt, game.main.current(), &game.archive);
            }
            // A save waits for a drawn frame's preview (headless, nothing else is drawn). With
            // the profiler on, every frame is drawn, as in the windowed game.
            if game.main.current().wants_save_preview()
                || shot_frames > 0
                || platform::perf::enabled()
            {
                let _p = platform::perf::scope("draw");
                game.draw(&mut renderer, false);
                shot_frames -= (shot_frames > 0) as i32;
            }
            platform::perf::frame_end();
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
                if command.starts_with("screenshot") {
                    shot_frames = 5;
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
            holds.retain_mut(|(code, start, end, down)| {
                if !*down && t >= *start {
                    game.key(None, None, Some(*code));
                    *down = true;
                }
                if *down && t >= *end {
                    game.released((keymap::KEYBOARD, *code));
                    return false;
                }
                true
            });
            pointer.retain(|&(kind, x, y, at)| {
                if t < at {
                    return true;
                }
                let logical = game.logical(size);
                if let Some(screen) = game.main.current().screen() {
                    screen.layout(&mut game.ui, logical);
                }
                game.pointer_move(x, y);
                match kind {
                    "--click-at" => {
                        game.pointer_button(true);
                        game.pointer_button(false);
                    }
                    "--mouse-down" => {
                        game.pointer_button(true);
                    }
                    "--mouse-up" => {
                        game.pointer_button(false);
                    }
                    _ => {}
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
            texts.retain(|(name, text, at)| {
                if t < *at {
                    return true;
                }
                if !game.type_into(name, text, size) {
                    log::warn!("--text {name}: not typed at {t:.2} s");
                }
                false
            });
        }
        let rgba = game.draw(&mut renderer, true).expect("capture");
        write_png(out, size, &rgba);
    }

    /// `--sim-digest SAVE [UPDATES] [EVERY]`: runs a save without a window at speed 10 and
    /// prints a digest of the game state every EVERY updates (`gameplay::digest`), to compare
    /// with the browser build's `?digest=`.
    pub fn sim_digest(args: &[String]) {
        let save = args.get(1).expect("--sim-digest SAVE [UPDATES] [EVERY]");
        let updates = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(1200);
        let every = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(60);
        let bytes = std::fs::read(save).expect("read the save");
        let dir = data_dir(args);
        let found = crate::mods::find(&dir);
        let mut game = Game::with_mods(
            read_packages(&dir),
            found,
            Some(dir),
            crate::settings::Settings::default(),
        )
        .expect("game data");
        game.audio.set_master_volume(0.0);
        let start = std::time::Instant::now();
        match crate::gameplay::digest::run(&mut game, &bytes, updates, every) {
            Ok(digests) => {
                for d in digests {
                    println!("{d}");
                }
                log::info!(
                    "{updates} updates in {:.1} s",
                    start.elapsed().as_secs_f32()
                );
            }
            Err(e) => {
                eprintln!("--sim-digest: {e}");
                std::process::exit(1);
            }
        }
    }

    /// `--replay SAVE RECORDING [UPDATES] [EVERY]`: replays a recording (`--record`) from its
    /// save without a window and compares the game state with the recorded digests
    /// (`gameplay::recording`). Exits with 1 when they differ.
    pub fn replay(args: &[String]) {
        let (Some(save), Some(recording)) = (args.get(1), args.get(2)) else {
            eprintln!("--replay SAVE RECORDING [UPDATES] [EVERY]");
            std::process::exit(2);
        };
        let updates = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(0);
        let every = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(0);
        let bytes = std::fs::read(save).expect("read the save");
        let text = std::fs::read_to_string(recording).expect("read the recording");
        let dir = data_dir(args);
        let found = crate::mods::find(&dir);
        let mut game = Game::with_mods(
            read_packages(&dir),
            found,
            Some(dir),
            crate::settings::Settings::default(),
        )
        .expect("game data");
        game.audio.set_master_volume(0.0);
        let start = std::time::Instant::now();
        match crate::gameplay::recording::replay(&mut game, &bytes, &text, updates, every) {
            Ok(r) => {
                for d in &r.digests {
                    println!("{d}");
                }
                log::info!("replayed in {:.1} s", start.elapsed().as_secs_f32());
                match r.mismatch {
                    Some((recorded, replayed)) => {
                        println!("DIFFERENT from the recording");
                        println!("  recorded {recorded}");
                        println!("  replayed {replayed}");
                        std::process::exit(1);
                    }
                    None => println!("identical: {} recorded digests matched", r.compared),
                }
            }
            Err(e) => {
                eprintln!("--replay: {e}");
                std::process::exit(2);
            }
        }
    }

    /// `--lockstep-test SAVE RECORDING [PLAYERS] [DELAY] [JITTER] [STALL] [SEED] [CHECK]
    /// [CORRUPT]`: plays a recording's commands, split between PLAYERS (default 3), in lockstep
    /// games joined by a simulated network (`gameplay::lockstep::simulate`), and compares their
    /// digests. The host checks digests every CHECK updates (default 600, 0 none) and resyncs
    /// the players from its save when they differ; CORRUPT `PLAYER:UPDATE` scrambles one
    /// player's game there, to test that. Exits with 1 when the players disagree at the end.
    pub fn lockstep_test(args: &[String]) {
        let (Some(save), Some(recording)) = (args.get(1), args.get(2)) else {
            eprintln!(
                "--lockstep-test SAVE RECORDING [PLAYERS] [DELAY] [JITTER] [STALL] [SEED] [CHECK] [CORRUPT]"
            );
            std::process::exit(2);
        };
        let number =
            |i: usize, default: u64| args.get(i).and_then(|s| s.parse().ok()).unwrap_or(default);
        let sim = crate::gameplay::network::Simulation {
            players: number(3, 3) as u8,
            delay: number(4, 6),
            jitter: number(5, 8),
            stall: number(6, 20),
            every: 60,
            check: number(8, 600),
            corrupt: args.get(9).and_then(|s| {
                let (p, u) = s.split_once(':')?;
                Some((p.parse().ok()?, u.parse().ok()?))
            }),
            seed: number(7, 1),
        };
        let bytes = std::fs::read(save).expect("read the save");
        let text = std::fs::read_to_string(recording).expect("read the recording");
        let dir = data_dir(args);
        let found = crate::mods::find(&dir);
        let mut game = Game::with_mods(
            read_packages(&dir),
            found,
            Some(dir),
            crate::settings::Settings::default(),
        )
        .expect("game data");
        game.audio.set_master_volume(0.0);
        let start = std::time::Instant::now();
        match crate::gameplay::network::simulate(&mut game, &bytes, &text, sim, 0) {
            Ok(r) => {
                for d in &r.digests[0] {
                    println!("{d}");
                }
                log::info!("simulated in {:.1} s", start.elapsed().as_secs_f32());
                println!(
                    "{sim:?}: {} ticks, players at most {} updates apart",
                    r.ticks, r.lead
                );
                println!(
                    "desyncs found at updates {:?}, resyncs at {:?}",
                    r.desyncs, r.resyncs
                );
                if r.recorded > 0 {
                    match r.recorded_mismatch {
                        Some((a, b)) => println!("DIFFERENT from the recording: {a} / {b}"),
                        None => println!("{} recorded digests matched", r.recorded),
                    }
                }
                match r.mismatch {
                    Some((p, a, b)) => {
                        println!("PLAYERS DISAGREE: player {p}");
                        println!("  player 0 {a}");
                        println!("  player {p} {b}");
                        std::process::exit(1);
                    }
                    None if r.recorded_mismatch.is_some() => std::process::exit(1),
                    None => println!(
                        "identical: {} players agree on {} digests",
                        sim.players,
                        r.digests[0].len()
                    ),
                }
            }
            Err(e) => {
                eprintln!("--lockstep-test: {e}");
                std::process::exit(2);
            }
        }
    }

    /// `--net-test RELAY host|join ROOM SAVE RECORDING PLAYERS [DELAY] [CHECK] [CORRUPT]`: one
    /// player of a networked lockstep game through a relay (`banished-relay`). The host opens
    /// ROOM, waits for PLAYERS, and starts the game from SAVE (joining players get it from the
    /// host and ignore SAVE); each plays its share of RECORDING's commands and prints its
    /// digests. CORRUPT `UPDATE` scrambles this player's game there. tools/compat/network.sh runs
    /// several and compares them.
    pub fn net_test(args: &[String]) {
        use crate::gameplay::network::{NetPeer, RelayLink, Script, Shared};
        let usage =
            "--net-test RELAY host|join ROOM SAVE RECORDING PLAYERS [DELAY] [CHECK] [CORRUPT]";
        let (Some(relay), Some(role), Some(room), Some(save), Some(recording), Some(players)) = (
            args.get(1),
            args.get(2),
            args.get(3),
            args.get(4),
            args.get(5),
            args.get(6).and_then(|s| s.parse::<u8>().ok()),
        ) else {
            eprintln!("{usage}");
            std::process::exit(2);
        };
        let number =
            |i: usize, default: u64| args.get(i).and_then(|s| s.parse().ok()).unwrap_or(default);
        let (delay, check) = (number(7, 6), number(8, 120));
        let corrupt = args.get(9).and_then(|s| s.parse::<u64>().ok());
        let host = role == "host";
        let text = std::fs::read_to_string(recording).expect("read the recording");
        let dir = data_dir(args);
        let found = crate::mods::find(&dir);
        let mut game = Game::with_mods(
            read_packages(&dir),
            found,
            Some(dir),
            crate::settings::Settings::default(),
        )
        .expect("game data");
        game.audio.set_master_volume(0.0);
        let link = std::rc::Rc::new(std::cell::RefCell::new(RelayLink::new(
            banished_net::Connection::connect(relay),
        )));
        let request = match host {
            true => banished_net::ToRelay::Host {
                room: Some(room.clone()),
            },
            false => banished_net::ToRelay::Join { room: room.clone() },
        };
        link.borrow_mut().request(request);
        // Who this player is, and (the host) everybody there.
        let wait = |what: &str, done: &mut dyn FnMut(&banished_net::FromRelay) -> bool| {
            let start = std::time::Instant::now();
            loop {
                let _ = crate::gameplay::network::Link::poll(&mut *link.borrow_mut());
                let events: Vec<_> = link.borrow_mut().events.drain(..).collect();
                for e in &events {
                    if let banished_net::FromRelay::Error { message } = e {
                        eprintln!("relay: {message}");
                        std::process::exit(2);
                    }
                    if done(e) {
                        return;
                    }
                }
                if let banished_net::State::Closed(e) = link.borrow().state() {
                    eprintln!("relay: {e}");
                    std::process::exit(2);
                }
                if start.elapsed().as_secs() > 120 {
                    eprintln!("waited too long for {what}");
                    std::process::exit(2);
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        };
        let mut me = 0u8;
        wait("the room", &mut |e| match e {
            banished_net::FromRelay::Hosted { .. } => true,
            banished_net::FromRelay::Joined { player, .. } => {
                me = *player;
                true
            }
            _ => false,
        });
        log::info!("player {me} in room {room}");
        let mut peer = NetPeer::new(me, Box::new(Shared(link.clone())), check);
        let mut script = Script::new(&text, players as usize, delay, false).expect("the recording");
        let last = script.last + delay + 1;
        let start = std::time::Instant::now();
        let outcome = game.with_ctx(|_, ctx| -> Result<Vec<String>, String> {
            if host {
                let mut joined = 1;
                wait("the players", &mut |e| {
                    if let banished_net::FromRelay::PeerJoined { .. } = e {
                        joined += 1;
                    }
                    joined >= players
                });
                let bytes = std::fs::read(save).map_err(|e| e.to_string())?;
                peer.host_start(ctx, bytes, players, delay)?;
            }
            let mut lines = Vec::new();
            let mut corrupt = corrupt;
            loop {
                let arrived = peer.poll(ctx)?;
                let Some(g) = peer.game.as_mut() else {
                    if !arrived {
                        std::thread::sleep(std::time::Duration::from_millis(2));
                    }
                    continue;
                };
                if g.updates() > last {
                    break;
                }
                script.give(me as usize, g, ctx);
                let Some(update) = peer.begin(ctx) else {
                    if !arrived {
                        std::thread::sleep(std::time::Duration::from_micros(500));
                    }
                    continue;
                };
                if corrupt.is_some_and(|at| update >= at) {
                    peer.game.as_mut().unwrap().scramble();
                    corrupt = None;
                }
                peer.boundary(ctx, update)?;
                if update % 60 == 0 {
                    // Printed at once, so that a run cut short can be compared too; after a
                    // resync an update can come again, and its last line counts.
                    let d = peer.game.as_ref().unwrap().state_digest(ctx, update)?;
                    println!("{d}");
                    lines.push(d.to_string());
                }
                peer.update(ctx);
            }
            // Let the last messages go out before the connection closes.
            let linger = std::time::Instant::now();
            while linger.elapsed().as_millis() < 1500 {
                peer.poll(ctx)?;
                peer.begin(ctx);
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            Ok(lines)
        });
        match outcome {
            Ok(_) => {
                println!("desyncs {:?} resyncs {:?}", peer.desyncs(), peer.resyncs());
                log::info!(
                    "player {me}: {} updates in {:.1} s",
                    last,
                    start.elapsed().as_secs_f32()
                );
            }
            Err(e) => {
                eprintln!("--net-test: {e}");
                std::process::exit(2);
            }
        }
    }

    /// The generated-map capture from M5 (`--capture`).
    pub fn map_capture(args: &[String], out: &str) {
        use crate::view::scene::{Camera, SceneCamera, SceneView};
        use crate::view::world::{World, WorldParams};
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
        let mut lighting = gfx::render::Lighting::from_month(&world.environment.months[month]);
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
    if matches!(args.first().map(String::as_str), Some("--version" | "-V")) {
        println!("{}", platform::version::NAME_VERSION);
        return;
    }
    log::info!("{}", platform::version::NAME_VERSION);
    // `--record FILE [--record-every N]`: record the next game loaded from a save.
    if let Some(path) = args.windows(2).find(|w| w[0] == "--record").map(|w| &w[1]) {
        let every = args
            .windows(2)
            .find(|w| w[0] == "--record-every")
            .and_then(|w| w[1].parse().ok())
            .unwrap_or(0);
        gameplay::recording::request(path.into(), every);
    }
    match args.first().map(String::as_str) {
        Some("--boot-capture") => {
            let out = args.get(1).expect("--boot-capture OUT.png SECONDS");
            let seconds = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(5.0);
            return native::boot_capture(&args, out, seconds);
        }
        Some("--sim-digest") => return native::sim_digest(&args),
        Some("--replay") => return native::replay(&args),
        Some("--lockstep-test") => return native::lockstep_test(&args),
        Some("--net-test") => return native::net_test(&args),
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
    let dir = native::data_dir(&args);
    app.pending_mods = Some((mods::find(&dir), Some(dir)));
    event_loop.run_app(&mut app).expect("run app");
}

#[cfg(target_arch = "wasm32")]
fn main() {
    console_error_panic_hook::set_once();
    let _ = console_log::init_with_level(log::Level::Info);
    log::info!("{}", platform::version::NAME_VERSION);
    platform::web::show_version(platform::version::NAME_VERSION);
    let event_loop = EventLoop::<UserEvent>::with_user_event()
        .build()
        .expect("event loop");
    let proxy = event_loop.create_proxy();
    platform::web::install_file_picker(proxy.clone());
    platform::web::load_from_query(proxy.clone());
    platform::web::load_digest_save(proxy.clone());
    // The saves in browser storage (their metadata, for the Save and Load dialogs).
    saves::save_store::init();
    platform::web::load_stored_mods(proxy.clone());
    platform::web::install_drop(proxy.clone());
    use winit::platform::web::EventLoopExtWebSys;
    event_loop.spawn_app(App::new(proxy));
}
