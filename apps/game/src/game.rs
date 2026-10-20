//! The running game: its resources, its loop chain, and the input and drawing that every
//! platform shares (the windowed game, captures, the browser).

use super::*;
/// The running game: shared resources plus the loop chain.
pub struct Game {
    pub(crate) archive: Rc<Archive>,
    /// The game's own packages, before the mods (`Archive::remove_after`).
    pub(crate) base_packages: usize,
    /// The resources loaded at startup (`formats::registry`).
    pub(crate) registry: Rc<unbanished_formats::registry::Registry>,
    /// The mods (`IO::Modification`) and the folder they were found in.
    pub(crate) mods: mods::Modification,
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    pub(crate) pack_dir: Option<std::path::PathBuf>,
    /// A reload was asked for, and the UI's sheets to upload again after one.
    pub(crate) reload_mods: Option<mods::Reload>,
    pub(crate) ui_reset: bool,
    pub(crate) ui: Ui,
    pub(crate) audio: audio::sound::Audio,
    pub(crate) global: Option<global::GlobalData>,
    pub(crate) rng: Random,
    pub(crate) main: MainLoop,
    /// A save's frame and minimap being read back (`SaveLoad::Draw`).
    pub(crate) save_capture: Option<gfx::render::PendingCapture>,
    /// A screenshot being read back (`FUN_14009b270`).
    pub(crate) screenshot: Option<gfx::render::PendingCapture>,
    /// `Application::IsPaused`: the window lost focus (`Application +0x11`, cleared on
    /// WM_KILLFOCUS, set on WM_SETFOCUS).
    pub(crate) app_paused: bool,
    /// The Options dialog's configurations (`IO::Persistence`).
    pub(crate) settings: settings::Settings,
    /// The display's scale factor (1.0 for captures). Deviation: the UI is drawn at
    /// `os_scale × UIScale / 100` so a high-density display does not shrink it; the original
    /// knows only `UIScale` (`UserInterface +0xdc`).
    pub(crate) os_scale: f32,
    /// The pointer in window pixels, once it has moved.
    pub(crate) pointer: Option<(i32, i32)>,
    /// The software pointers (`FUN_1400a0e00`'s table, `Global +0x28`): Arrow, Hand,
    /// Hourglass, Move, Query and Place, each its sheet and hotspot once loaded.
    pub(crate) cursors: Vec<Option<(unbanished_ui::SpriteRef, [i32; 2])>>,
    /// A capture's cinematic camera this frame: eye, target and vertical field of view
    /// ([`view::cinematic`]). Deviation: not in the original; drawing only, set by captures.
    pub(crate) cinematic: Option<(glam::Vec3, glam::Vec3, f32)>,
}

/// `SetupFileSystem` and `LoadResources`: the enabled mods join the packages (from the last
/// in the list to the first), then the game's resources and each mod's resource list load.
pub(crate) fn load_mods(
    archive: &mut Archive,
    mods: &mut mods::Modification,
) -> unbanished_formats::registry::Registry {
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
    let roots = unbanished_formats::registry::roots(&mods.enabled_files());
    unbanished_formats::registry::Registry::build(archive, &roots)
}

/// `Cursor/{name}.rsc:cursor`, in cursor index order.
pub(crate) const CURSORS: [&str; 6] = ["Arrow", "Hand", "Hourglass", "Move", "Query", "Place"];

/// The startup chain built in `FUN_140042850`: a loader loop for "Game/Global" (`FUN_14009f280`),
/// the Shining Rock splash (at least 2 s), the loading screen (at least 0.5 s), then the menu.
pub(crate) fn boot_chain() -> MainLoop {
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
pub(crate) fn reload_chain(next: Box<dyn loops::GameLoop>) -> MainLoop {
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

pub(crate) fn clock_seed() -> u32 {
    // Scripted capture override: fix the separate engine/menu stream (town names, menu scene)
    // as well as the world seed. Normal System::Random() still seeds from the clock.
    if let Ok(seed) = unbanished_formats::env::var("SEED")
        .unwrap_or_default()
        .parse()
    {
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
    pub(crate) fn apply_settings(&mut self) {
        let a = self.settings.audio;
        self.audio.scene.volumes = unbanished_audio::scene::Volumes {
            ui: a.ui,
            music: a.music,
            ambient: a.ambient,
            effect: a.effect,
        };
    }

    /// "Use Software Mouse" (`Input::Configuration +10`): the pointer is drawn by the game, at
    /// its image's own size, instead of by the system (`UserInterface::SetCursor` hands the
    /// cursor's sheet image and hotspot to the device).
    pub(crate) fn cursor_quad(&mut self) -> Option<unbanished_ui::Quad> {
        let (x, y) = self.pointer?;
        if self.cursors.is_empty() {
            self.cursors = CURSORS
                .iter()
                .map(|name| {
                    let data = self
                        .archive
                        .read_ref(&format!("Cursor/{name}.rsc:cursor"))
                        .ok()?;
                    let c = unbanished_formats::ui::cursor(&data).ok()?;
                    let sheet_ref = c.sprite_sheet?.replace('\\', "/");
                    let sheet = self.ui.res.sheet(&sheet_ref)?;
                    let loaded = &self.ui.res.sheets[sheet].sheet;
                    let sprite = unbanished_ui::SpriteRef {
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
        Some(unbanished_ui::Quad {
            texture: unbanished_ui::Texture::Sheet(sprite.sheet),
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

    pub(crate) fn with_ctx<R>(&mut self, f: impl FnOnce(&mut MainLoop, &mut Ctx) -> R) -> R {
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

    pub(crate) fn input_root(&self) -> Option<usize> {
        self.main.current().screen().map(|s| s.input_root())
    }

    pub(crate) fn raw(&mut self, input: RawInput) {
        self.with_ctx(|main, ctx| main.raw(ctx, input));
    }

    pub(crate) fn deliver_events(&mut self) {
        self.with_ctx(|main, ctx| main.events(ctx));
        self.ui_sounds();
    }

    /// `UserInterface::Play` requests from the UI (button down, click, drop).
    pub(crate) fn ui_sounds(&mut self) {
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
    pub(crate) fn pointer_move_ui(&mut self, x: i32, y: i32) {
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
    pub fn key(&mut self, key: Option<unbanished_ui::Key>, text: Option<&str>, code: Option<u8>) {
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
        self.press_node(id);
        true
    }

    /// Click an element through the normal pointer path: move to its centre (after the last
    /// layout), press and release.
    pub fn press_node(&mut self, id: unbanished_ui::NodeId) {
        let [x0, y0, x1, y1] = self.ui.node(id).rect;
        self.pointer_move_ui((x0 + x1) / 2, (y0 + y1) / 2);
        self.pointer_button(true);
        self.pointer_button(false);
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
            unbanished_ui::Kind::List(data) => {
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
    pub(crate) fn add_mods(&mut self, found: Vec<(String, Vec<u8>)>) {
        let count = found.len();
        if count > 0 {
            self.mods.add(found, &self.settings.persistence);
            log::info!("added {count} mods");
        }
    }

    /// `FUN_140072110`'s reload: the mods as the list has them now. `SaveList`, then the
    /// packages close and load again (`SetupFileSystem`), the game's resources reload
    /// (`Game/Global`) and `next` starts behind the loading screen.
    pub(crate) fn reload(&mut self, next: Box<dyn loops::GameLoop>) {
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
pub(crate) fn scale_quads(quads: &mut [unbanished_ui::Quad], s: f32) {
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

pub(crate) fn ui_key(key: &Key, shift: bool) -> Option<unbanished_ui::Key> {
    use unbanished_ui::Key as K;
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
