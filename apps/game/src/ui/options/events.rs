//! The dialog's events: changed combos, and applying or cancelling the changes.

use super::*;

impl OptionsDialog {
    /// `FUN_1400783f0` and the message boxes' buttons.
    pub fn event(&mut self, ctx: &mut Ctx, screen: &mut Screen, event: Event) -> Outcome {
        if self.rebind.is_some() {
            if let Event::Clicked(id) = event {
                self.rebind_click(ctx, screen, id);
            }
            return Outcome::Stay;
        }
        match event {
            Event::Clicked(id) => {
                if self.is(ctx.ui, self.el.ok, id) {
                    self.apply(ctx);
                    self.close(ctx, screen);
                    return Outcome::Closed;
                }
                if self.is(ctx.ui, self.el.cancel, id) {
                    self.cancel(ctx);
                    self.close(ctx, screen);
                    return Outcome::Closed;
                }
                if self.is(ctx.ui, self.el.apply, id) {
                    self.apply(ctx);
                    return Outcome::Stay;
                }
                let key = self.el.keys.iter().enumerate().find_map(|(row, b)| {
                    (0..2)
                        .find(|&slot| ctx.ui.descendant(b[slot], id))
                        .map(|slot| (row, slot))
                });
                if let Some((row, slot)) = key {
                    self.start_rebind(ctx, screen, row, slot);
                }
            }
            Event::Selected(id, _) => self.combo_changed(ctx, id),
            Event::Changed(id) => {
                // Event 0x17: the volumes change while the slider moves.
                let a = &mut ctx.settings.audio;
                let targets = [&mut a.ui, &mut a.music, &mut a.ambient, &mut a.effect];
                for (slider, target) in self.el.volumes.iter().zip(targets) {
                    if *slider == Some(id) {
                        *target = slider_value(ctx.ui, id);
                    }
                }
            }
            _ => {}
        }
        Outcome::Stay
    }

    /// Event 0x14 on combos 4–9.
    pub(super) fn combo_changed(&mut self, ctx: &mut Ctx, id: NodeId) {
        let data = |ui: &Ui, combo: Option<NodeId>, items: &[i32]| {
            combo_selected(ui, combo).and_then(|i| items.get(i).copied())
        };
        if Some(id) == self.el.renderer {
            if let Some(d) = data(ctx.ui, self.el.renderer, &self.renderer_items) {
                self.video.interface = d;
                self.validate(ctx);
                self.fill_adapters(ctx);
            }
        } else if Some(id) == self.el.adapter {
            if let Some(d) = data(ctx.ui, self.el.adapter, &self.adapter_items) {
                self.video.adapter = d;
                self.validate(ctx);
                self.fill_antialias(ctx);
                self.fill_filters(ctx);
                self.fill_resolutions(ctx);
            }
        } else if Some(id) == self.el.resolution {
            if let Some(d) = data(ctx.ui, self.el.resolution, &self.resolution_items) {
                let modes = ctx
                    .settings
                    .display
                    .modes(self.video.interface, self.video.adapter);
                if let Some(m) = modes.get(d as usize) {
                    self.video.width = m.width;
                    self.video.height = m.height;
                }
                self.validate(ctx);
                self.fill_refresh(ctx);
            }
        } else if Some(id) == self.el.refresh {
            let mode = data(ctx.ui, self.el.resolution, &self.resolution_items);
            let rate = data(ctx.ui, self.el.refresh, &self.refresh_items);
            if let (Some(m), Some(r)) = (mode, rate) {
                let modes = ctx
                    .settings
                    .display
                    .modes(self.video.interface, self.video.adapter);
                if let Some(&refresh) = modes
                    .get(m as usize)
                    .and_then(|m| m.refresh.get(r as usize))
                {
                    self.video.refresh = refresh;
                }
                self.validate(ctx);
            }
        } else if Some(id) == self.el.antialias {
            if let Some(d) = data(ctx.ui, self.el.antialias, &self.antialias_items) {
                self.video.multisample = d;
                self.validate(ctx);
            }
        } else if Some(id) == self.el.filter {
            if let Some(d) = data(ctx.ui, self.el.filter, &self.filter_items) {
                self.video.filter = d;
                self.validate(ctx);
            }
        }
    }

    /// `FUN_140077db0`: video, input, audio and game, then write the settings.
    pub(super) fn apply(&mut self, ctx: &mut Ctx) {
        self.apply_video(ctx);
        // `FUN_140077830`.
        ctx.settings.input.mouse_clipping = check(ctx.ui, self.el.mouse_clip);
        ctx.settings.input.mouse_display = check(ctx.ui, self.el.software_mouse);
        // `FUN_1400778f0`.
        let ui = &*ctx.ui;
        let g = &mut ctx.settings.game;
        g.ui_scale = slider(ui, self.el.ui_scale) as i32;
        g.autosave_interval = combo_selected(ui, self.el.autosave).map_or(0, |i| i as i32);
        g.status_icon_enable = check(ui, self.el.status_icon);
        g.status_icon_opacity = slider(ui, self.el.icon_opacity) as i32;
        g.status_icon_scale = slider(ui, self.el.icon_scale) as i32;
        g.edge_scroll = check(ui, self.el.edge_scroll);
        g.show_celsius = check(ui, self.el.celsius);
        g.show_weather = check(ui, self.el.weather);
        g.auto_slow = check(ui, self.el.autoslow);
        g.auto_pause = check(ui, self.el.autopause);
        g.last_option = active_tab(ui, self.el.tabs);
        ctx.settings.save();
        ctx.settings.revision = ctx.settings.revision.wrapping_add(1);
    }

    /// `FUN_1400772e0`: take the edited video configuration. A resolution change also sets
    /// the window size. The device restart or `ApplyChangedSettings` happens in the platform
    /// layer when it sees the new revision.
    pub(super) fn apply_video(&mut self, ctx: &mut Ctx) {
        let ui = &*ctx.ui;
        let e = self.video;
        let v = &mut ctx.settings.video;
        if e.width != v.width || e.height != v.height {
            v.window_width = e.width;
            v.window_height = e.height;
        }
        v.interface = e.interface;
        v.adapter = e.adapter;
        v.width = e.width;
        v.height = e.height;
        v.refresh_n = e.refresh.0;
        v.refresh_d = e.refresh.1;
        v.multisample = e.multisample;
        v.texture_filter = e.filter;
        v.fullscreen = check(ui, self.el.fullscreen);
        v.vsync = check(ui, self.el.vsync);
        v.shadow_scalar =
            combo_selected(ui, self.el.shadow_resolution).map_or(v.shadow_scalar, |i| i as i32);
        v.shader_detail =
            combo_selected(ui, self.el.shadow_quality).map_or(v.shader_detail, |i| i as i32);
        v.reflection_detail =
            combo_selected(ui, self.el.reflection).map_or(v.reflection_detail, |i| i as i32);
    }

    /// `FUN_140077d10`: re-read the stored input, key map, audio and game settings (the tab
    /// shown is kept).
    pub(super) fn cancel(&mut self, ctx: &mut Ctx) {
        let p = &ctx.settings.persistence;
        ctx.settings.input = InputConfig::load(p);
        ctx.settings.keymap = KeyMap::load(p);
        ctx.settings.audio = AudioConfig::load(p);
        let tab = active_tab(ctx.ui, self.el.tabs);
        ctx.settings.game = GameOptions::load(p);
        ctx.settings.game.last_option = tab;
        ctx.settings.revision = ctx.settings.revision.wrapping_add(1);
    }
}
