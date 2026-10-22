//! The window's events, as winit hands them to the application.

use super::*;

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
