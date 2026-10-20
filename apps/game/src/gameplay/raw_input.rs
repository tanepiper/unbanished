//! Raw input that the interface did not take (`GameLoop::raw`): keys, the mouse on the map, the
//! wheel and dragging.

use super::*;

impl GameplayLoop {
    /// `GameLoop::raw`: input the interface did not take.
    pub(super) fn raw_input(&mut self, ctx: &mut Ctx, input: RawInput) -> Next {
        match input {
            RawInput::Action(id) if id < keys::COUNT => self.held[id] = true,
            RawInput::ActionReleased(id) if id < keys::COUNT => self.held[id] = false,
            RawInput::Cursor(..) => self.cursor_seen = true,
            _ => {}
        }
        // The Mods dialog: Escape is buttonCancel's action 8.
        if let (Some(d), Some(screen)) = (self.mods_dialog.as_mut(), self.screen.as_mut()) {
            if let RawInput::Escape = input {
                d.cancel(ctx.ui, screen, ctx.mods, &ctx.settings.persistence);
                self.mods_dialog = None;
            }
            return Next::Stay;
        }
        // The Save and Load dialog: Escape is buttonCancel's action 8.
        if let (Some(d), Some(screen)) = (self.save_dialog.as_mut(), self.screen.as_mut()) {
            if let RawInput::Escape = input {
                let outcome = d.escape(ctx.ui, screen);
                if let Some(next) = self.save_outcome(ctx, outcome) {
                    return next;
                }
            }
            return Next::Stay;
        }
        // While the pause menu is open the controller does not run; Escape is buttonResume's
        // action 8.
        if let (Some(options), Some(screen)) = (&mut self.options, &mut self.screen) {
            let outcome = match input {
                RawInput::Released(binding) => {
                    options.released(ctx, screen, binding);
                    crate::ui::options::Outcome::Stay
                }
                RawInput::Escape => options.escape(ctx, screen),
                RawInput::Cursor(x, y) => {
                    self.cursor = (x, y);
                    crate::ui::options::Outcome::Stay
                }
                _ => crate::ui::options::Outcome::Stay,
            };
            if outcome == crate::ui::options::Outcome::Closed {
                self.options = None;
            }
            return Next::Stay;
        }
        if self.pause_menu.is_some() {
            match input {
                RawInput::Escape => self.close_pause_menu(ctx),
                RawInput::Cursor(x, y) => self.cursor = (x, y),
                _ => {}
            }
            return Next::Stay;
        }
        if self.drag_window(ctx, &input) {
            return Next::Stay;
        }
        match input {
            RawInput::Cursor(x, y) => {
                self.cursor = (x, y);
                self.fill_tooltip(ctx);
                if self.placement.is_some() {
                    self.refresh_placement(ctx);
                }
                if self.removal.is_some() {
                    self.removal_input(ctx, DragInput::Move);
                }
                return Next::Stay;
            }
            RawInput::Release => {
                if self.removal.is_some() {
                    self.removal_input(ctx, DragInput::Release);
                }
                return Next::Stay;
            }
            // StructureLeft / StructureRight turn the RotationSetting through 0..3.
            // (FencedTool `FUN_1400e20b0` turns the same setting for its shelter.)
            RawInput::Action(keys::STRUCTURE_LEFT)
                if self.placement.is_some() || self.removal.is_some() =>
            {
                self.rotation = (self.rotation + 3) % 4;
                self.refresh_placement(ctx);
                return Next::Stay;
            }
            RawInput::Action(keys::STRUCTURE_RIGHT)
                if self.placement.is_some() || self.removal.is_some() =>
            {
                self.rotation = (self.rotation + 1) % 4;
                self.refresh_placement(ctx);
                return Next::Stay;
            }
            RawInput::Action(keys::STRUCTURE_TYPE) => {
                if let Some(tool) = &mut self.placement {
                    tool.next_variant();
                    self.refresh_placement(ctx);
                }
                return Next::Stay;
            }
            RawInput::Click if self.removal.is_some() => {
                self.removal_input(ctx, DragInput::Press);
                return Next::Stay;
            }
            RawInput::Click => {
                if let (Some(tool), Some(_)) = (self.placement.as_ref(), self.town.as_ref()) {
                    // `FUN_140102b20`: under a RestrictPlayer setup, only on its cells, turned.
                    let allowed =
                        Self::restrict_allows(self.restrict.as_ref(), tool, self.rotation);
                    // `FUN_1401055c0` / `FUN_140105610`: thunk on placement, error otherwise.
                    let placement = tool.placement(self.rotation).filter(|_| allowed);
                    // In lockstep the placement waits for its slot; it was checked here, and
                    // every computer places it.
                    let placed = match placement {
                        Some(p) => match self.execute(ctx.archive, PlayerCommand::Place(p)) {
                            Outcome::Placed(id) => Some(Some(id)),
                            Outcome::Queued => Some(None),
                            _ => None,
                        },
                        None => None,
                    };
                    let Some(tool) = self.placement.as_mut() else {
                        return Next::Stay;
                    };
                    if placed.is_some() {
                        tool.placed();
                    }
                    let sound = match placed {
                        Some(id) => {
                            log::info!("placed {} as entity {id:?}", tool.reference);
                            crate::audio::sound::global::PLACEMENT_THUNK
                        }
                        None => {
                            log::info!("{}: cannot build here", tool.reference);
                            crate::audio::sound::global::PLACEMENT_ERROR
                        }
                    };
                    ctx.audio
                        .play(ctx.archive, sound, crate::audio::sound::sound_type::UI);
                    self.refresh_placement(ctx);
                } else if let Some(target) = self.map_click(ctx) {
                    // MapUI: move the camera (`FUN_14006d1b0`).
                    if let Some(SceneView {
                        camera: SceneCamera::Map(c),
                        ..
                    }) = &mut self.view
                    {
                        c.target = target;
                    }
                } else if self.path_tool.is_some() {
                    self.path_click(ctx);
                } else {
                    self.select_at_cursor(ctx);
                    self.position_entity_window(ctx);
                }
                return Next::Stay;
            }
            // `FUN_140104bb0`: MouseRight (action 6) first lets the tool cancel what it is doing
            // (DragTool `+0x48`, a drag in progress), else ends the tool; not while a
            // RestrictPlayer setup holds it.
            RawInput::Action(keys::MOUSE_RIGHT) => {
                let restricted = self.restrict.as_ref().is_some_and(|r| !r.place.is_empty());
                let tool =
                    self.placement.is_some() || self.removal.is_some() || self.path_tool.is_some();
                if tool && !restricted && !self.removal.as_mut().is_some_and(|t| t.cancel()) {
                    self.end_placement(ctx);
                    if let Some(toolbar) = &self.toolbar {
                        toolbar.clear_build(ctx.ui);
                    }
                }
                return Next::Stay;
            }
            // DragTool `+0x48` (`FUN_140101b10`): Escape first stops a drag in progress.
            RawInput::Escape if self.removal.as_mut().is_some_and(|t| t.cancel()) => {
                return Next::Stay;
            }
            // `FUN_140104bb0`: the tool stays while a RestrictPlayer setup holds.
            RawInput::Escape if self.restrict.as_ref().is_some_and(|r| !r.place.is_empty()) => {
                return Next::Stay;
            }
            // `FUN_140069bb0`: Escape closes the Follow dialog.
            RawInput::Escape if self.follow.is_some() => {
                self.close_follow(ctx);
                return Next::Stay;
            }
            RawInput::Escape
                if self.placement.is_some()
                    || self.removal.is_some()
                    || self.path_tool.is_some() =>
            {
                self.end_placement(ctx);
                if let Some(toolbar) = &self.toolbar {
                    toolbar.clear_build(ctx.ui);
                }
                return Next::Stay;
            }
            // `FUN_14009b850`: the screenshot actions.
            RawInput::Action(keys::SCREENSHOT_NO_UI) => {
                self.screenshot = Some(false);
                return Next::Stay;
            }
            RawInput::Action(keys::SCREENSHOT) => {
                self.screenshot = Some(true);
                return Next::Stay;
            }
            RawInput::Action(keys::TOGGLE_PAUSE) => {
                self.execute(ctx.archive, PlayerCommand::SetPaused(!self.paused));
                self.update_hud(ctx.ui);
                return Next::Stay;
            }
            // Toolbar0..8, Tool0..9, Camera0..4: the shown button holding that key.
            RawInput::Action(a @ keys::TOOLBAR0..=keys::CAMERA4) => {
                let (Some(screen), Some(toolbar)) = (self.screen.as_mut(), self.toolbar.as_mut())
                else {
                    return Next::Stay;
                };
                if let Some((node, action)) = toolbar.press_action(ctx.ui, screen, a as i32) {
                    // A key press counts as pressing its button for the tutorial's goals.
                    if let (Some(t), true) = (self.tutorial.as_mut(), ctx.ui.node(node).usable()) {
                        t.pressed.push(node);
                    }
                    if let Some(action) = action {
                        self.run_action(ctx, action);
                    }
                }
                self.update_hud(ctx.ui);
                return Next::Stay;
            }
            _ => {}
        }
        let Some(SceneView {
            camera: SceneCamera::Map(camera),
            ..
        }) = &mut self.view
        else {
            return Next::Stay;
        };
        match input {
            RawInput::Drag(dx, dy) => camera.rotate(dx, dy),
            RawInput::Wheel(steps) => camera.zoom(steps),
            RawInput::Action(keys::ZOOM_IN) => camera.zoom(1.0),
            RawInput::Action(keys::ZOOM_OUT) => camera.zoom(-1.0),
            RawInput::Action(keys::PITCH_UP) => camera.pitch(1.0),
            RawInput::Action(keys::PITCH_DOWN) => camera.pitch(-1.0),
            // Toolbar Options (kind 3) holds the fixed action 8, Escape.
            RawInput::Escape => self.open_pause_menu(ctx),
            _ => {}
        }
        Next::Stay
    }
}
