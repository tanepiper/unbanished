//! Filling the video page's lists and the key list from the settings.

use super::*;

impl OptionsDialog {
    /// `FUN_140079610`: the renderer list, then the adapter list and its dependants.
    pub(super) fn fill_video(&mut self, ctx: &mut Ctx) {
        let names: Vec<String> = ctx
            .settings
            .display
            .interfaces
            .iter()
            .map(|i| i.name.clone())
            .collect();
        self.renderer_items = (0..names.len() as i32).collect();
        fill_combo(ctx.ui, self.el.renderer, names);
        set_combo(ctx.ui, self.el.renderer, self.video.interface);
        self.fill_adapters(ctx);
    }

    /// `FUN_140079510`: the adapters of the chosen renderer, then modes, antialiasing, filters.
    pub(super) fn fill_adapters(&mut self, ctx: &mut Ctx) {
        let names: Vec<String> = ctx
            .settings
            .display
            .interfaces
            .get(self.video.interface.max(0) as usize)
            .map(|i| i.adapters.iter().map(|a| a.name.clone()).collect())
            .unwrap_or_default();
        self.adapter_items = (0..names.len() as i32).collect();
        fill_combo(ctx.ui, self.el.adapter, names);
        set_combo(ctx.ui, self.el.adapter, self.video.adapter);
        self.fill_resolutions(ctx);
        self.fill_antialias(ctx);
        self.fill_filters(ctx);
    }

    /// `FUN_140079390`: the adapter's modes as `%ix%i`, then the refresh rates.
    pub(super) fn fill_resolutions(&mut self, ctx: &mut Ctx) {
        let modes = ctx
            .settings
            .display
            .modes(self.video.interface, self.video.adapter);
        let items = modes
            .iter()
            .map(|m| format!("{}x{}", m.width, m.height))
            .collect();
        let selected = modes
            .iter()
            .position(|m| m.width == self.video.width && m.height == self.video.height)
            .map_or(-1, |i| i as i32);
        self.resolution_items = (0..modes.len() as i32).collect();
        fill_combo(ctx.ui, self.el.resolution, items);
        set_combo(ctx.ui, self.el.resolution, selected);
        self.fill_refresh(ctx);
    }

    /// `FUN_1400791f0`: the chosen mode's refresh rates as `n/d` (rounded down).
    pub(super) fn fill_refresh(&mut self, ctx: &mut Ctx) {
        let modes = ctx
            .settings
            .display
            .modes(self.video.interface, self.video.adapter);
        let refresh = modes
            .iter()
            .find(|m| m.width == self.video.width && m.height == self.video.height)
            .map(|m| m.refresh.clone())
            .unwrap_or_default();
        let items = refresh
            .iter()
            .map(|&(n, d)| (n / d.max(1)).to_string())
            .collect();
        let selected = refresh
            .iter()
            .position(|&r| r == self.video.refresh)
            .map_or(-1, |i| i as i32);
        self.refresh_items = (0..refresh.len() as i32).collect();
        fill_combo(ctx.ui, self.el.refresh, items);
        set_combo(ctx.ui, self.el.refresh, selected);
    }

    /// `FUN_140078fe0`: "None", then "%ix MSAA" for each supported sample count.
    pub(super) fn fill_antialias(&mut self, ctx: &mut Ctx) {
        let none = table_text(ctx.ui, "multiSample", 0);
        let msaa = table_text(ctx.ui, "multiSample", 1);
        let mut items = Vec::new();
        self.antialias_items.clear();
        let mut selected = -1;
        for (n, &supported) in ctx.settings.display.multisample.iter().enumerate() {
            if !supported {
                continue;
            }
            items.push(if n == 0 {
                none.clone()
            } else {
                format!("{}x {msaa}", n + 1)
            });
            if n as i32 == self.video.multisample {
                selected = self.antialias_items.len() as i32;
            }
            self.antialias_items.push(n as i32);
        }
        fill_combo(ctx.ui, self.el.antialias, items);
        set_combo(ctx.ui, self.el.antialias, selected);
    }

    /// `FUN_140078cd0`: Bilinear (−1), Trilinear (0), then Anisotropic 1x, 2x, 4x … up to the
    /// device's largest anisotropy.
    pub(super) fn fill_filters(&mut self, ctx: &mut Ctx) {
        let max = ctx.settings.display.max_anisotropy;
        let aniso = table_text(ctx.ui, "textureFilter", 2);
        let mut items = vec![
            table_text(ctx.ui, "textureFilter", 0),
            table_text(ctx.ui, "textureFilter", 1),
        ];
        self.filter_items = vec![-1, 0];
        if max > 0 {
            items.push(format!("{aniso} 1x"));
            self.filter_items.push(1);
        }
        let mut n = 2;
        while n <= max {
            items.push(format!("{aniso} {n}x"));
            self.filter_items.push(n);
            n += 2;
        }
        let selected = match self.video.filter {
            -1 => 0,
            0 => 1,
            1 => 2,
            f => (f >> 1) + 2,
        };
        fill_combo(ctx.ui, self.el.filter, items);
        set_combo(ctx.ui, self.el.filter, selected);
    }

    /// `FUN_140078a50`: each row's action name and key buttons.
    pub(super) fn refresh_keys(&self, ctx: &mut Ctx) {
        for (row, buttons) in self.el.keys.iter().enumerate() {
            let id = FIRST_KEY + row;
            let name = action_name(ctx.ui, id);
            if let Some(text) = self.name_text(ctx.ui, row) {
                ctx.ui.set_text(text, &name);
            }
            for (slot, &button) in buttons.iter().enumerate() {
                let binding = ctx.settings.keymap.binding(id, slot);
                let text = key_name(ctx.ui, binding);
                if let Some(t) = ctx.ui.find(button, "textInputKey") {
                    ctx.ui.set_text(t, &text);
                }
            }
        }
    }

    /// The `textInputName` of key-list row `row`.
    pub(super) fn name_text(&self, ui: &Ui, row: usize) -> Option<NodeId> {
        let list = ui.find(self.root, "inputList")?;
        let cells = match &ui.node(list).kind {
            Kind::Control(Control::Table { cells, .. }) => cells.clone(),
            _ => return None,
        };
        let group = cells
            .iter()
            .find(|&&(col, r, _)| col == 0 && r == row)
            .map(|c| c.2)?;
        ui.find(group, "textInputName")
    }
}
