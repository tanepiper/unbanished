//! One frame of the game loop (`GameLoop::update`): input, the simulation steps, the camera,
//! windows, sound and the next loop.

use super::*;

impl GameplayLoop {
    /// `GameLoop::update`: one frame of the game.
    pub(super) fn update_frame(&mut self, ctx: &mut Ctx, dt: f32) -> Next {
        // A multiplayer game: what the other players sent, the slot before this update, the
        // turns to send and the host's checks. Held: the slot waits for another player's turn.
        let held = match self.begin_online(ctx) {
            Ok(held) => held,
            Err(next) => return next,
        };
        self.record_digest(ctx, held);
        if !held {
            self.sim.updates += 1;
        }
        if let Some(d) = self.mods_dialog.as_mut() {
            d.poll(ctx.ui, ctx.settings, ctx.mods);
        }
        if !self.entities.saved.is_empty() {
            self.store_window_settings(ctx.ui, false);
        }
        self.close_removed_windows(ctx.ui);
        {
            self.finish_save(ctx);
            // A multiplayer game is not autosaved (the host saves it for a resync).
            if !self.online() {
                self.update_autosave(ctx, dt);
            }
            if let Some(next) = self.update_save_dialog(ctx, dt) {
                return next;
            }
        }
        if ctx.ui.screen[0] > 0 && ctx.ui.screen[1] > 0 {
            self.aspect = ctx.ui.screen[0] as f32 / ctx.ui.screen[1] as f32;
        }
        self.fill_tooltip(ctx);
        if let Some(toolbar) = &mut self.toolbar {
            toolbar.set_keymap(ctx.ui, &ctx.settings.keymap);
            toolbar.relayout(ctx.ui);
        }
        // FUN_1400be4a0 runs every update: the build limits follow the town.
        self.refresh_toolbar(ctx.ui);
        // FUN_140107b70: while the application is paused (unfocused) the controller opens the
        // pause menu (game option `+0x18`, on by default) instead of running.
        if ctx.app_paused && ctx.settings.game.auto_pause && self.pause_menu.is_none() {
            self.open_pause_menu(ctx);
        }
        self.show_celsius = ctx.settings.game.show_celsius;
        if let Some(options) = &mut self.options {
            options.update();
        }
        // `UserInterface::PausesGame`: FUN_14009b850 skips the simulation and the controller.
        // Deviation: in a multiplayer game the pause menu does not hold the shared game.
        let pauses_game = match (self.replaying(), self.in_lockstep()) {
            (true, _) => self.menu_paused,
            (false, true) => false,
            (false, false) => self.pause_menu.is_some(),
        };
        if !self.replaying() && pauses_game != self.menu_paused {
            self.menu_paused = pauses_game;
            self.record_menu_pause(self.sim.updates - 1, pauses_game);
        }
        // Controller FUN_140107b70 → FUN_140104390: the camera eases toward its zoom and pitch
        // targets every frame.
        if ctx.app_paused {
            self.held = [false; keys::COUNT];
        }
        if let Some(view) = self.view.as_mut().filter(|_| !pauses_game) {
            if let SceneCamera::Map(camera) = &mut view.camera {
                // `FUN_140104390`: held rotation keys, then movement from the screen edges
                // (game option EdgeScroll) overridden by the held movement keys.
                let held = |id: usize| self.held[id];
                let turn = held(keys::ROTATE_RIGHT) as i32 - held(keys::ROTATE_LEFT) as i32;
                camera.turn(turn as f32, dt);
                let mut d = glam::Vec2::ZERO;
                let [w, h] = ctx.ui.screen;
                if ctx.settings.game.edge_scroll && self.cursor_seen && w > 0 && h > 0 {
                    let (x, y) = self.cursor;
                    if x < EDGE {
                        d.x = -1.0;
                    }
                    if w - EDGE - 1 < x && x < w {
                        d.x = 1.0;
                    }
                    if y < EDGE {
                        d.y = -1.0;
                    }
                    if h - EDGE - 1 < y && y < h {
                        d.y = 1.0;
                    }
                }
                if held(keys::LEFT) {
                    d.x = -1.0;
                }
                if held(keys::RIGHT) {
                    d.x = 1.0;
                }
                if held(keys::FORWARD) {
                    d.y = -1.0;
                }
                if held(keys::BACK) {
                    d.y = 1.0;
                }
                camera.travel(d, dt, &view.terrain);
                camera.update(&view.terrain);
            }
        }
        // PlacementTool slot +0x38 runs every frame: ghost, validity and the CreateBar.
        if self.placement.is_some() && !pauses_game {
            self.refresh_placement(ctx);
        }
        let mut models_changed = false;
        let mut iterations = 0;
        // The town moves only on an update every player runs.
        let runs = !pauses_game && !held;
        if !self.paused && runs {
            for _ in 0..SPEED_LEVELS[self.speed as usize] {
                let _p = crate::platform::perf::scope("sim.iterate");
                models_changed |= self.iterate(ctx.archive, dt);
                iterations += 1;
            }
        }
        if runs {
            self.update_goal(ctx, dt, ticks(dt * iterations as f32));
            self.update_achievements(ctx, ticks(dt * iterations as f32));
            self.force_restricted_tool(ctx);
        }
        self.refresh_tutorial_ui(ctx.ui);
        if let Some(i) = self.next_scenario.take() {
            if let Some((_, s)) = tutorial::scenarios(ctx.archive).into_iter().nth(i) {
                let game = tutorial::scenario_game(ctx.ui, ctx.archive, ctx.global.as_ref(), &s, i);
                return Next::Switch(Box::new(DialogLoaderLoop::new(
                    format!("Dialog/Loading.rsc:gameLoad{}", ctx.rng.rand_below(12)),
                    0.5,
                    Box::new(game),
                )));
            }
        }
        self.update_follow(ctx);
        {
            let _p = crate::platform::perf::scope("refresh_decals");
            self.refresh_decals(ctx.archive);
        }
        {
            let _p = crate::platform::perf::scope("refresh_particles");
            self.refresh_particles(ctx.archive, dt * iterations as f32);
        }
        {
            let _p = crate::platform::perf::scope("refresh_weather");
            self.refresh_weather(ctx, dt * iterations as f32);
        }
        // FUN_14009b850 runs this phase once after the speed loop, even at speed zero.
        if runs && self.view.is_some() && self.sim.town.is_some() {
            models_changed |= self.sim.smooth();
            self.ambience_stale |= models_changed;
            if let (true, Some(town), Some(view)) = (models_changed, &self.sim.town, &mut self.view)
            {
                if let Some(models) = &mut view.models {
                    let _p = crate::platform::perf::scope("refresh_models");
                    self.town_view
                        .as_ref()
                        .expect("the town view comes with the town")
                        .refresh_models(town, ctx.archive, models);
                }
            }
        }
        self.utility.update(ctx.ui, &self.status_values());
        if let (Some(notify), Some(screen)) = (self.notify.as_mut(), self.screen.as_mut()) {
            let width = self.toolbar.as_ref().map_or(0, |t| t.width(ctx.ui));
            if notify.update(ctx.ui, screen, &self.sim.events, dt, width) {
                // `Scene::PlayEffect(WarningEffect, 1)`.
                ctx.audio.play(
                    ctx.archive,
                    crate::audio::sound::global::WARNING,
                    crate::audio::sound::sound_type::UI,
                );
            }
        }
        if let Some(town) = &self.sim.town {
            let town_ctx = crate::ui::town_hall_ui::TownContext {
                stock_counts: &self.sim.stock_warnings.count,
                statistics: &self.sim.statistics,
            };
            self.entities.refresh_with(
                ctx.ui,
                ctx.archive,
                town,
                &self.sim.stock_warnings.limits,
                &self.params.town_name,
                Some(&town_ctx),
            );
        }
        // TradeUI's open (`FUN_140087f40`) and close (`FUN_140087ed0`): the post holds its
        // merchant while its window is open.
        let open = self.entities.trade_windows();
        if open != self.trade_held {
            if self.sim.town.is_some() {
                use unbanished_game::{commerce::trade::TradeSetting, work::professions::Command};
                let closed = self.trade_held.iter().filter(|e| !open.contains(e));
                let opened = open.iter().filter(|e| !self.trade_held.contains(e));
                let held: Vec<Command> = closed
                    .map(|&entity| (entity, false))
                    .chain(opened.map(|&entity| (entity, true)))
                    .map(|(entity, on)| Command::Trade {
                        entity,
                        setting: TradeSetting::Held(on),
                    })
                    .collect();
                for command in held {
                    self.execute(ctx.archive, PlayerCommand::Town(command));
                }
            }
            self.trade_held = open;
        }
        self.position_entity_window(ctx);
        if let (Some(town), Some(view)) = (self.sim.town.as_ref(), self.view.as_mut()) {
            let grid = self
                .sim
                .heights
                .as_ref()
                .expect("the heights come with the view")
                .grid();
            view.highlight =
                self.highlight
                    .geometry(ctx.archive, town, &grid, self.selection.selected);
            view.status_icons = self
                .town_view
                .as_ref()
                .expect("the town view comes with the town")
                .status_icons(town);
            // Repainted when entities change (MapComponent activation/removal).
            if models_changed || view.minimap_overlay.is_none() {
                let revision = view.minimap_overlay.as_ref().map_or(0, |o| o.0) + 1;
                view.minimap_overlay =
                    Some((revision, town.cells.size as u32, town.minimap_overlay()));
            }
            for r in self
                .town_view
                .as_ref()
                .expect("the town view comes with the town")
                .icon_sheet_refs(town)
            {
                if let std::collections::btree_map::Entry::Vacant(e) = view.icon_sheets.entry(r) {
                    if let Some(sheet) = crate::gfx::status_icons::load_sheet(ctx.archive, e.key())
                    {
                        e.insert(sheet);
                    }
                }
            }
        }
        self.log_events(ctx);
        // AutoSlow (settings `+0x17`): a fire, a blight, a tornado or a shortage
        // (`FUN_14009a2d0`, `FUN_14009aa50`, `FUN_14009ac20` and the disasters themselves) sets
        // the game speed (`Game +0x120 → +0x34`) back to 1.
        // Deviation: in a multiplayer game AutoSlow is always on, so the speed does not depend on
        // each player's own setting.
        let auto_slow = ctx.settings.game.auto_slow || self.in_lockstep();
        if std::mem::take(&mut self.sim.events.slow_request) && auto_slow {
            self.speed = 0;
        }
        self.move_cooldown -= dt;
        {
            let _p = crate::platform::perf::scope("audio");
            self.update_audio(ctx, dt);
        }
        let month = self.month();
        let into_month =
            (self.sim.calendar.seconds() - month as f32 * MONTH_SECONDS) / MONTH_SECONDS;
        if let (Some(view), Some(from)) =
            (self.view.as_mut(), self.sim.environment_months.get(month))
        {
            // `FUN_140098f90`: blend this month's lighting towards the next.
            let to = &self.sim.environment_months[(month + 1) % self.sim.environment_months.len()];
            view.lighting = Lighting::between_months(from, to, into_month);
            // With ShowWeather, each running effect pulls the lighting towards its own.
            if let (true, Some(env)) = (ctx.settings.game.show_weather, &self.sim.environment) {
                for (slot, p) in self.precipitation.iter().enumerate() {
                    let (Some(p), Some(Some(e))) = (p, env.effects.get(slot)) else {
                        continue;
                    };
                    view.lighting.toward_weather(&p.desc, e.intensity);
                }
            }
            if let Some(env) = &self.sim.environment {
                let s = env.season;
                view.lighting.user = glam::Vec4::new(s.snow, s.autumn, s.leaf_loss, s.rain);
            }
        }
        Next::Stay
    }
}
