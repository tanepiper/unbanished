//! The windowed game: the winit application that owns the window, the renderer and the
//! [`Game`], and turns window events into the game's input.

use super::*;
/// The window's fullscreen state for a video configuration: exclusive when a monitor mode
/// matches the chosen resolution and refresh rate, otherwise borderless on the current monitor.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn fullscreen(
    window: &Window,
    video: &settings::VideoConfig,
) -> Option<winit::window::Fullscreen> {
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

pub(crate) struct App {
    pub(crate) proxy: EventLoopProxy<UserEvent>,
    pub(crate) window: Option<Arc<Window>>,
    pub(crate) renderer: Option<Renderer>,
    pub(crate) pending_packages: Option<Vec<(String, Vec<u8>)>>,
    /// The mods found beside them (`*.pkm`) and that folder.
    pub(crate) pending_mods: Option<(Vec<(String, Vec<u8>)>, Option<std::path::PathBuf>)>,
    /// `?digest=`: the save, updates and interval, waiting for the game to start.
    #[cfg(target_arch = "wasm32")]
    pub(crate) pending_digest: Option<(Vec<u8>, u32, u32)>,
    /// `?net=`: a networked test's joining player, once the game has started.
    #[cfg(target_arch = "wasm32")]
    #[cfg(feature = "multiplayer")]
    pub(crate) net_session: Option<gameplay::network::JoinSession>,
    pub(crate) game: Option<Game>,
    pub(crate) last_frame: Option<Instant>,
    /// `Engine::MainLoop +0x38`: real time not yet stepped (ticks).
    pub(crate) accumulated: i64,
    pub(crate) cursor: (f64, f64),
    pub(crate) drag_button: Option<MouseButton>,
    pub(crate) shift: bool,
    /// The stored settings until the game takes them over.
    pub(crate) settings: Option<settings::Settings>,
    /// The settings revision whose video and input configuration the window shows.
    pub(crate) applied: u32,
    /// The browser's frame-rate limit (the Options refresh rate there): frames that come
    /// sooner after the last are skipped.
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    pub(crate) frame_interval: Option<std::time::Duration>,
}

impl App {
    pub(crate) fn new(proxy: EventLoopProxy<UserEvent>) -> App {
        App {
            proxy,
            window: None,
            renderer: None,
            pending_packages: None,
            pending_mods: None,
            #[cfg(target_arch = "wasm32")]
            pending_digest: None,
            #[cfg(target_arch = "wasm32")]
            #[cfg(feature = "multiplayer")]
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

    pub(crate) fn settings(&self) -> Option<&settings::Settings> {
        self.game
            .as_ref()
            .map(|g| &g.settings)
            .or(self.settings.as_ref())
    }

    pub(crate) fn settings_mut(&mut self) -> Option<&mut settings::Settings> {
        match &mut self.game {
            Some(g) => Some(&mut g.settings),
            None => self.settings.as_mut(),
        }
    }

    /// Build a renderer for the window with the configured interface and adapter
    /// (`Video::Device::Startup`); it arrives as `UserEvent::Renderer`.
    pub(crate) fn start_renderer(&self) {
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
    pub(crate) fn display_modes(&self) -> Vec<settings::DisplayMode> {
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
    pub(crate) fn apply_settings(&mut self) {
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

    pub(crate) fn try_start(&mut self) {
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

pub(crate) fn set_status(text: &str, loaded: bool) {
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
                #[cfg(not(target_arch = "wasm32"))]
                {
                    renderer.display.desktop = self
                        .window
                        .as_ref()
                        .and_then(|w| w.current_monitor())
                        .map(|m| (m.size().width as i32, m.size().height as i32));
                }
                // Deviation: in the browser the "desktop" resolution, which an unset or unknown
                // resolution falls back to (`GetDefaultResolution`), is the page's full size: the
                // 100 % entry of the Resolution list, as the scene is drawn without a setting.
                #[cfg(target_arch = "wasm32")]
                {
                    renderer.display.desktop = self.window.as_ref().map(|w| {
                        let size = w.inner_size();
                        (size.width as i32, size.height as i32)
                    });
                }
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
                #[cfg(all(feature = "multiplayer", target_arch = "wasm32"))]
                {
                    if self.net_session.is_none() {
                        if let Some((relay, room, updates)) = platform::web::net_request() {
                            self.net_session = Some(gameplay::network::JoinSession::new(
                                &relay,
                                &room,
                                updates,
                                gameplay::network::hello(&game.archive),
                            ));
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
                    game.update(unbanished_game::scheduler::seconds(FRAME_STEP));
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
                    game.update(unbanished_game::scheduler::seconds(FRAME_STEP));
                    self.accumulated -= FRAME_STEP;
                    if game.quit_requested() {
                        break;
                    }
                }
                // Tests' probe commands (`window.unbanished.probe`), between updates and drawing.
                #[cfg(all(feature = "probe", target_arch = "wasm32"))]
                if let Some(w) = &self.window {
                    let size = w.inner_size();
                    probe::web::answer_pending(game, (size.width, size.height));
                }
                if game.quit_requested() {
                    #[cfg(target_arch = "wasm32")]
                    platform::web::exit_page();
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
