//! The Options dialog (`Dialog/Options.rsc`, controller vtable `0x1401632e0`), opened from the
//! main menu and the pause menu (`MainMenuDialog` element id 4 → `FUN_1400b8ee0`).
//!
//! - Bind (`FUN_1400797a0`): buttons Ok 10, Cancel 11 (action 8, Escape), Apply 12; the four
//!   volume sliders (ids 0–3, 0–100) from the audio configuration; the video combos (ids 4–9)
//!   from a working copy of the video configuration (`+0x190..+0x1ac`); the shadow and
//!   reflection combos select their configured index; the input checks; the key list
//!   (`FUN_140078a50`); the game tab (autosave combo from `StringTable:autoSave`, UI scale
//!   75–150, icon opacity 0–100, icon scale 25–200) and the last tab shown.
//! - Events (`FUN_1400783f0`): a click on 10/11/12 applies and closes, reverts and closes, or
//!   applies; a click on a key button (ids `13 + 2·row + slot`) rebinds it (`FUN_140077e70`);
//!   a combo selection (0x14) updates the working video configuration and refills the
//!   dependent combos; a slider change (0x17) sets that volume at once.
//! - Apply (`FUN_140077db0`): video (`FUN_1400772e0`), input (`FUN_140077830`), audio, game
//!   (`FUN_1400778f0`), then `Persistence::WriteSettings`.
//! - Cancel (`FUN_140077d10`): re-read the input, key map, audio and game settings.
//!
//! Deviation: the Renderer combo lists wgpu backends (Vulkan, Metal, DirectX 12, OpenGL, WebGPU)
//! where the original lists its DirectX 9 and 11 interfaces. The autosave interval drives
//! `crate::autosave` and auto slow-down the disaster slow-down.
//! See docs/OPTIONS.md.

use banished_ui::{Control, Event, Kind, NodeId, Ui};

use crate::keymap::{self, Binding, KeyMap};
use crate::loops::{Ctx, Screen};
use crate::menu::{combo_list, select_combo};
use crate::settings::{AudioConfig, GameOptions, InputConfig};

const DIALOG: &str = "Dialog/Options.rsc:resource";
const BIND_DIALOG: &str = "Dialog/Options.rsc:inputBind";
const BOUND_DIALOG: &str = "Dialog/Options.rsc:inputBound";
const STRINGS: &str = "Dialog/StringTable.rsc";

/// Escape's shortcut code in `banished_ui` (the Cancel button carries action 8, Escape).
const ESCAPE: u32 = 27;

/// `+0x190..+0x1ac`: the video configuration being edited.
#[derive(Debug, Clone, Copy, PartialEq)]
struct VideoEdit {
    interface: i32,
    adapter: i32,
    width: i32,
    height: i32,
    refresh: (i32, i32),
    multisample: i32,
    filter: i32,
}

/// The rebinding message boxes (`OptionInputBindDialog`, `MessageDialog`).
enum Rebind {
    /// `inputBind`: wait for a released key or mouse button. `armed` is false until the frame
    /// after the click that opened it, whose release must not bind.
    Waiting {
        root: NodeId,
        row: usize,
        slot: usize,
        armed: bool,
    },
    /// `inputBound`: the input already triggers action `other`.
    Confirm {
        root: NodeId,
        row: usize,
        slot: usize,
        binding: Binding,
        other: usize,
    },
}

/// What the owning loop should do after an event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Stay,
    Closed,
}

struct Elements {
    ok: Option<NodeId>,
    cancel: Option<NodeId>,
    apply: Option<NodeId>,
    /// UI, music, ambient, effect.
    volumes: [Option<NodeId>; 4],
    renderer: Option<NodeId>,
    adapter: Option<NodeId>,
    resolution: Option<NodeId>,
    refresh: Option<NodeId>,
    antialias: Option<NodeId>,
    filter: Option<NodeId>,
    shadow_resolution: Option<NodeId>,
    shadow_quality: Option<NodeId>,
    reflection: Option<NodeId>,
    fullscreen: Option<NodeId>,
    vsync: Option<NodeId>,
    software_mouse: Option<NodeId>,
    mouse_clip: Option<NodeId>,
    autosave: Option<NodeId>,
    ui_scale: Option<NodeId>,
    status_icon: Option<NodeId>,
    icon_opacity: Option<NodeId>,
    icon_scale: Option<NodeId>,
    celsius: Option<NodeId>,
    edge_scroll: Option<NodeId>,
    weather: Option<NodeId>,
    autoslow: Option<NodeId>,
    autopause: Option<NodeId>,
    tabs: Option<NodeId>,
    /// Each `inputList` row's primary and secondary key buttons; row `r` is action `r + 5`.
    keys: Vec<[NodeId; 2]>,
}

pub struct OptionsDialog {
    root: NodeId,
    el: Elements,
    video: VideoEdit,
    /// Item data of the video combos (`ComboElement::GetItemData`).
    renderer_items: Vec<i32>,
    adapter_items: Vec<i32>,
    resolution_items: Vec<i32>,
    refresh_items: Vec<i32>,
    antialias_items: Vec<i32>,
    filter_items: Vec<i32>,
    rebind: Option<Rebind>,
}

/// First action shown in the key list.
const FIRST_KEY: usize = 5;

/// `StringTable::GetText(table, index)`: the `index`th entry's text.
fn table_text(ui: &Ui, table: &str, index: usize) -> String {
    ui.res
        .archive
        .read_ref(&format!("{STRINGS}:{table}"))
        .ok()
        .and_then(|d| banished_formats::ui::string_table(&d).ok())
        .and_then(|t| t.into_iter().nth(index).map(|(_, v)| v))
        .unwrap_or_default()
}

/// `GameGlobal::KeyName` (`FUN_14009f0f0`).
fn key_name(ui: &mut Ui, binding: Option<Binding>) -> String {
    let key = keymap::key_name_key(binding);
    ui.res
        .string(Some(r"Dialog\StringTable.rsc:keyNames"), &key)
}

/// The action's name: `gameKeys` entry `id` (`GameGlobal +200`).
fn action_name(ui: &Ui, id: usize) -> String {
    table_text(ui, "gameKeys", id)
}

fn combo_selected(ui: &Ui, combo: Option<NodeId>) -> Option<usize> {
    let list = combo_list(ui, combo?)?;
    match &ui.node(list).kind {
        Kind::List(l) => l.selected,
        _ => None,
    }
}

/// `ComboElement::RemoveAllItems` + `AddItem`s; returns nothing selected.
fn fill_combo(ui: &mut Ui, combo: Option<NodeId>, items: Vec<String>) {
    if let Some(c) = combo {
        ui.set_choices(c, items);
        if let Some(list) = combo_list(ui, c) {
            ui.list_select(list, None);
        }
        ui.set_text(c, "");
    }
}

/// `ComboElement::SetCurrentItem` (−1 or out of range clears the selection).
fn set_combo(ui: &mut Ui, combo: Option<NodeId>, index: i32) {
    let Some(c) = combo else { return };
    if index >= 0 {
        select_combo(ui, c, index as usize);
    }
}

fn set_slider(ui: &mut Ui, slider: Option<NodeId>, range: [f32; 2], value: f32) {
    if let Some(s) = slider {
        ui.set_slider_range(s, range);
        ui.set_value(s, value);
    }
}

fn slider(ui: &Ui, slider: Option<NodeId>) -> f32 {
    slider.and_then(|s| ui.value(s)).unwrap_or(0.0)
}

fn check(ui: &Ui, check: Option<NodeId>) -> bool {
    check.is_some_and(|c| ui.checked(c))
}

fn set_check(ui: &mut Ui, check: Option<NodeId>, value: bool) {
    if let Some(c) = check {
        ui.set_checked(c, value);
    }
}

fn active_tab(ui: &Ui, tabs: Option<NodeId>) -> i32 {
    match tabs.map(|t| &ui.node(t).kind) {
        Some(Kind::Control(Control::Tabs { selected, .. })) => *selected as i32,
        _ => 0,
    }
}

/// `TabElement::SetActiveTab`.
fn set_active_tab(ui: &mut Ui, tabs: Option<NodeId>, index: i32) {
    if let Some(Kind::Control(Control::Tabs { tabs, selected, .. })) =
        tabs.map(|t| &mut ui.node_mut(t).kind)
    {
        if index >= 0 && (index as usize) < tabs.len() {
            *selected = index as usize;
        }
    }
}

impl OptionsDialog {
    /// `FUN_1400b8ee0` (AddDialog) → bind `FUN_1400797a0`.
    pub fn open(ctx: &mut Ctx, screen: &mut Screen) -> Option<OptionsDialog> {
        let index = screen.add_modal(ctx.ui, DIALOG)?;
        let root = screen.dialogs[index].root;
        let ui = &mut *ctx.ui;
        let find = |name: &str| ui.find(root, name);
        let mut keys = Vec::new();
        if let Some(list) = find("inputList") {
            let children = match &ui.node(list).kind {
                Kind::Control(Control::Table { cells, .. }) => {
                    let mut cells = cells.clone();
                    cells.sort_by_key(|&(col, row, _)| (row, col));
                    cells.into_iter().map(|c| c.2).collect::<Vec<_>>()
                }
                _ => ui.children(list),
            };
            for row in children.chunks_exact(3) {
                keys.push([row[1], row[2]]);
            }
        }
        let el = Elements {
            ok: find("buttonOk"),
            cancel: find("buttonCancel"),
            apply: find("buttonApply"),
            volumes: [
                find("sliderUI"),
                find("sliderMusic"),
                find("sliderAmbient"),
                find("sliderEffect"),
            ],
            renderer: find("comboRenderer"),
            adapter: find("comboAdapter"),
            resolution: find("comboResolution"),
            refresh: find("comboRefresh"),
            antialias: find("comboAntialias"),
            filter: find("comboFilter"),
            shadow_resolution: find("comboShadowResolution"),
            shadow_quality: find("comboShadowQuality"),
            reflection: find("comboReflectionQuality"),
            fullscreen: find("checkFullscreen"),
            vsync: find("checkVSync"),
            software_mouse: find("checkSoftwareMouse"),
            mouse_clip: find("checkMouseClip"),
            autosave: find("comboAutosave"),
            ui_scale: find("sliderUIScale"),
            status_icon: find("checkStatusIcon"),
            icon_opacity: find("sliderStatusIcon"),
            icon_scale: find("sliderStatusIconScale"),
            celsius: find("checkShowCelsius"),
            edge_scroll: find("checkEdgeScroll"),
            weather: find("checkShowWeather"),
            autoslow: find("checkAutoslow"),
            autopause: find("checkAutopause"),
            tabs: find("tabOptions"),
            keys,
        };
        if let Some(cancel) = el.cancel {
            ui.set_shortcut(cancel, ESCAPE);
        }
        let v = ctx.settings.video.clone();
        let mut dialog = OptionsDialog {
            root,
            el,
            video: VideoEdit {
                interface: v.interface,
                adapter: v.adapter,
                width: v.width,
                height: v.height,
                refresh: (v.refresh_n, v.refresh_d),
                multisample: v.multisample,
                filter: v.texture_filter,
            },
            renderer_items: Vec::new(),
            adapter_items: Vec::new(),
            resolution_items: Vec::new(),
            refresh_items: Vec::new(),
            antialias_items: Vec::new(),
            filter_items: Vec::new(),
            rebind: None,
        };
        dialog.bind(ctx);
        Some(dialog)
    }

    fn bind(&mut self, ctx: &mut Ctx) {
        let a = ctx.settings.audio;
        for (slider, value) in self
            .el
            .volumes
            .iter()
            .zip([a.ui, a.music, a.ambient, a.effect])
        {
            set_slider(ctx.ui, *slider, [0.0, 100.0], value as f32);
        }
        self.validate(ctx);
        self.fill_video(ctx);
        let v = ctx.settings.video.clone();
        set_combo(ctx.ui, self.el.shadow_resolution, v.shadow_scalar);
        set_combo(ctx.ui, self.el.shadow_quality, v.shader_detail);
        set_combo(ctx.ui, self.el.reflection, v.reflection_detail);
        set_check(ctx.ui, self.el.fullscreen, v.fullscreen);
        set_check(ctx.ui, self.el.vsync, v.vsync);
        set_check(
            ctx.ui,
            self.el.software_mouse,
            ctx.settings.input.mouse_display,
        );
        set_check(
            ctx.ui,
            self.el.mouse_clip,
            ctx.settings.input.mouse_clipping,
        );
        self.refresh_keys(ctx);
        self.bind_game(ctx);
    }

    /// The game tab of `FUN_1400797a0`.
    fn bind_game(&mut self, ctx: &mut Ctx) {
        let g = ctx.settings.game;
        let items = (0..9).map(|i| table_text(ctx.ui, "autoSave", i)).collect();
        fill_combo(ctx.ui, self.el.autosave, items);
        set_combo(ctx.ui, self.el.autosave, g.autosave_interval.clamp(0, 8));
        set_check(ctx.ui, self.el.autoslow, g.auto_slow);
        set_check(ctx.ui, self.el.autopause, g.auto_pause);
        set_slider(ctx.ui, self.el.ui_scale, [75.0, 150.0], g.ui_scale as f32);
        set_check(ctx.ui, self.el.status_icon, g.status_icon_enable);
        set_slider(
            ctx.ui,
            self.el.icon_opacity,
            [0.0, 100.0],
            g.status_icon_opacity as f32,
        );
        set_slider(
            ctx.ui,
            self.el.icon_scale,
            [25.0, 200.0],
            g.status_icon_scale as f32 * 0.01 * 100.0,
        );
        set_check(ctx.ui, self.el.celsius, g.show_celsius);
        set_check(ctx.ui, self.el.edge_scroll, g.edge_scroll);
        set_check(ctx.ui, self.el.weather, g.show_weather);
        set_active_tab(ctx.ui, self.el.tabs, g.last_option);
    }

    /// `FUN_140078380`.
    fn validate(&mut self, ctx: &Ctx) {
        let v = &mut self.video;
        ctx.settings.display.validate(
            &mut v.interface,
            &mut v.adapter,
            &mut v.width,
            &mut v.height,
            &mut v.refresh,
            &mut v.multisample,
            &mut v.filter,
        );
    }

    /// `FUN_140079610`: the renderer list, then the adapter list and its dependants.
    fn fill_video(&mut self, ctx: &mut Ctx) {
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
    fn fill_adapters(&mut self, ctx: &mut Ctx) {
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
    fn fill_resolutions(&mut self, ctx: &mut Ctx) {
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
    fn fill_refresh(&mut self, ctx: &mut Ctx) {
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
    fn fill_antialias(&mut self, ctx: &mut Ctx) {
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
    fn fill_filters(&mut self, ctx: &mut Ctx) {
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
    fn refresh_keys(&self, ctx: &mut Ctx) {
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
    fn name_text(&self, ui: &Ui, row: usize) -> Option<NodeId> {
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

    fn is(&self, ui: &Ui, element: Option<NodeId>, node: NodeId) -> bool {
        element.is_some_and(|e| ui.descendant(e, node))
    }

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
    fn combo_changed(&mut self, ctx: &mut Ctx, id: NodeId) {
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
    fn apply(&mut self, ctx: &mut Ctx) {
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
    fn apply_video(&mut self, ctx: &mut Ctx) {
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
    fn cancel(&mut self, ctx: &mut Ctx) {
        let p = &ctx.settings.persistence;
        ctx.settings.input = InputConfig::load(p);
        ctx.settings.keymap = KeyMap::load(p);
        ctx.settings.audio = AudioConfig::load(p);
        let tab = active_tab(ctx.ui, self.el.tabs);
        ctx.settings.game = GameOptions::load(p);
        ctx.settings.game.last_option = tab;
        ctx.settings.revision = ctx.settings.revision.wrapping_add(1);
    }

    fn close(&mut self, ctx: &mut Ctx, screen: &mut Screen) {
        self.close_rebind(ctx, screen);
        screen.remove_root(ctx.ui, self.root);
    }

    /// `FUN_140077e70`: show `inputBind` for action `row + 5`.
    fn start_rebind(&mut self, ctx: &mut Ctx, screen: &mut Screen, row: usize, slot: usize) {
        let Some(index) = screen.add_modal(ctx.ui, BIND_DIALOG) else {
            return;
        };
        let root = screen.dialogs[index].root;
        let name = action_name(ctx.ui, FIRST_KEY + row);
        if let Some(text) = ctx.ui.find(root, "inputBindText") {
            ctx.ui.set_parameters(text, &[(0, &name)]);
        }
        self.rebind = Some(Rebind::Waiting {
            root,
            row,
            slot,
            armed: false,
        });
    }

    fn close_rebind(&mut self, ctx: &mut Ctx, screen: &mut Screen) {
        if let Some(Rebind::Waiting { root, .. } | Rebind::Confirm { root, .. }) =
            self.rebind.take()
        {
            screen.remove_root(ctx.ui, root);
        }
    }

    /// Message box buttons: `inputBind` Unbind (result 2) or Cancel; `inputBound` Yes (2) or
    /// No (3).
    fn rebind_click(&mut self, ctx: &mut Ctx, screen: &mut Screen, id: NodeId) {
        let Some(state) = &self.rebind else { return };
        let (root, row, slot) = match *state {
            Rebind::Waiting {
                root, row, slot, ..
            }
            | Rebind::Confirm {
                root, row, slot, ..
            } => (root, row, slot),
        };
        let is = |name: &str| {
            ctx.ui
                .find(root, name)
                .is_some_and(|b| ctx.ui.descendant(b, id))
        };
        let yes = is("buttonYes") || is("buttonOk");
        let no = is("buttonNo") || is("buttonCancel");
        if !yes && !no {
            return;
        }
        let id_ = FIRST_KEY + row;
        match *state {
            Rebind::Waiting { .. } => {
                if yes {
                    // Result 2: `KeyMap::ClearInput(id, slot)` and the button shows "Unbound".
                    ctx.settings.keymap.set(id_, slot, None);
                    self.set_key_text(ctx, row, slot);
                }
                self.close_rebind(ctx, screen);
            }
            Rebind::Confirm { binding, other, .. } => {
                self.close_rebind(ctx, screen);
                if yes {
                    ctx.settings.keymap.clear_input(other, binding);
                    ctx.settings.keymap.set(id_, slot, Some(binding));
                }
                self.refresh_keys(ctx);
            }
        }
    }

    fn set_key_text(&self, ctx: &mut Ctx, row: usize, slot: usize) {
        let Some(&button) = self.el.keys.get(row).map(|b| &b[slot]) else {
            return;
        };
        let binding = ctx.settings.keymap.binding(FIRST_KEY + row, slot);
        let text = key_name(ctx.ui, binding);
        if let Some(t) = ctx.ui.find(button, "textInputKey") {
            ctx.ui.set_text(t, &text);
        }
    }

    /// Per frame: the bind box listens from the frame after it opened (`FUN_140077150`).
    pub fn update(&mut self) {
        if let Some(Rebind::Waiting { armed, .. }) = &mut self.rebind {
            *armed = true;
        }
    }

    /// A released key (device 1, codes 0–0x68) or mouse button (device 2, codes 4–11) while
    /// the bind box waits: bind it, or ask when another action already uses it.
    pub fn released(&mut self, ctx: &mut Ctx, screen: &mut Screen, binding: Binding) {
        let Some(Rebind::Waiting {
            row, slot, armed, ..
        }) = self.rebind
        else {
            return;
        };
        let valid = match binding.0 {
            keymap::KEYBOARD => binding.1 < 0x69,
            keymap::MOUSE => (4..=0xb).contains(&binding.1),
            _ => false,
        };
        if !armed || !valid {
            return;
        }
        self.close_rebind(ctx, screen);
        let id = FIRST_KEY + row;
        let other = ctx.settings.keymap.id_for(binding);
        let this_name = action_name(ctx.ui, id);
        match other {
            Some(other) if action_name(ctx.ui, other) != this_name => {
                let Some(index) = screen.add_modal(ctx.ui, BOUND_DIALOG) else {
                    return;
                };
                let root = screen.dialogs[index].root;
                let key = key_name(ctx.ui, Some(binding));
                let other_name = action_name(ctx.ui, other);
                if let Some(text) = ctx.ui.find(root, "inputBoundText") {
                    ctx.ui
                        .set_parameters(text, &[(0, &key), (1, &other_name), (2, &this_name)]);
                }
                self.rebind = Some(Rebind::Confirm {
                    root,
                    row,
                    slot,
                    binding,
                    other,
                });
            }
            other => {
                // The same action name (or no other action): replace without asking.
                if let Some(other) = other {
                    ctx.settings.keymap.clear_input(other, binding);
                }
                ctx.settings.keymap.set(id, slot, Some(binding));
                self.refresh_keys(ctx);
            }
        }
    }

    /// Escape while a message box is open closes it (its Cancel/No); otherwise the dialog's
    /// Cancel shortcut handles it.
    pub fn escape(&mut self, ctx: &mut Ctx, screen: &mut Screen) -> Outcome {
        if self.rebind.is_some() {
            if matches!(self.rebind, Some(Rebind::Confirm { .. })) {
                self.close_rebind(ctx, screen);
                self.refresh_keys(ctx);
            }
            // While waiting, Escape is a key like any other (bound on release).
            return Outcome::Stay;
        }
        self.cancel(ctx);
        self.close(ctx, screen);
        Outcome::Closed
    }
}

fn slider_value(ui: &Ui, id: NodeId) -> i32 {
    ui.value(id).unwrap_or(0.0) as i32
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    fn game() -> Option<crate::Game> {
        let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../game-data/original/WinData");
        if !dir.join("data0.pkg").exists() {
            eprintln!("game data missing; skipping options integration");
            return None;
        }
        // Never touch the player's stored settings.
        let file =
            std::env::temp_dir().join(format!("banished-options-{}.txt", std::process::id()));
        std::env::set_var("BANISHED_SETTINGS", &file);
        let packages = ["data1.pkg", "patch1.pkg", "patch0.pkg", "data0.pkg"]
            .into_iter()
            .filter_map(|n| std::fs::read(dir.join(n)).ok().map(|d| (n.to_owned(), d)))
            .collect();
        crate::Game::with_settings(packages, crate::settings::Settings::default()).ok()
    }

    #[test]
    fn edits_apply_cancel_and_rebind_like_the_original() {
        let Some(mut game) = game() else { return };
        game.with_ctx(|_, ctx| {
            let mut screen = Screen::new(ctx.ui);
            let mut o = OptionsDialog::open(ctx, &mut screen).unwrap();
            assert_eq!(o.el.keys.len(), keymap::id::COUNT - FIRST_KEY);
            let name = o.name_text(ctx.ui, 3).unwrap();
            assert!(matches!(&ctx.ui.node(name).kind, Kind::Text(t) if t.text == "Menu/Pause"));
            // Event 0x17: the music volume follows its slider at once; Cancel reverts it.
            let music = o.el.volumes[1].unwrap();
            ctx.ui.set_value(music, 20.0);
            o.event(ctx, &mut screen, Event::Changed(music));
            assert_eq!(ctx.settings.audio.music, 20);
            let cancel = o.el.cancel.unwrap();
            assert_eq!(o.event(ctx, &mut screen, Event::Clicked(cancel)), Outcome::Closed);
            assert_eq!(ctx.settings.audio.music, AudioConfig::default().music);

            // Apply stores the game tab.
            let mut o = OptionsDialog::open(ctx, &mut screen).unwrap();
            ctx.ui.set_value(o.el.ui_scale.unwrap(), 140.0);
            ctx.ui.set_checked(o.el.celsius.unwrap(), true);
            o.event(ctx, &mut screen, Event::Clicked(o.el.apply.unwrap()));
            assert_eq!(ctx.settings.game.ui_scale, 140);
            assert!(ctx.settings.game.show_celsius);
            assert!(ctx.settings.persistence.serialize().contains("GameUIScale=140"));

            // Rebind Forward (row 6) to S: S is Back's, so `inputBound` asks; Yes moves it.
            let forward = keymap::id::FORWARD - FIRST_KEY;
            let s = (keymap::KEYBOARD, 41);
            o.event(ctx, &mut screen, Event::Clicked(o.el.keys[forward][0]));
            // The release of the opening click does not bind.
            o.released(ctx, &mut screen, (keymap::MOUSE, 4));
            assert!(matches!(o.rebind, Some(Rebind::Waiting { .. })));
            o.update();
            o.released(ctx, &mut screen, s);
            let Some(Rebind::Confirm { root, .. }) = o.rebind else {
                panic!("expected the inputBound box");
            };
            let text = ctx.ui.find(root, "inputBoundText").unwrap();
            assert!(matches!(&ctx.ui.node(text).kind,
                Kind::Text(t) if t.text == "S is already bound to Move View Back. Bind it to Move View Forward instead?"));
            let yes = ctx.ui.find(root, "buttonYes").unwrap();
            o.event(ctx, &mut screen, Event::Clicked(yes));
            assert_eq!(ctx.settings.keymap.binding(keymap::id::FORWARD, 0), Some(s));
            assert_eq!(ctx.settings.keymap.binding(keymap::id::BACK, 0), None);
            // Unbind (result 2) clears the slot.
            o.event(ctx, &mut screen, Event::Clicked(o.el.keys[forward][0]));
            let Some(Rebind::Waiting { root, .. }) = o.rebind else {
                panic!("expected the inputBind box");
            };
            let unbind = ctx.ui.find(root, "buttonYes").unwrap();
            o.event(ctx, &mut screen, Event::Clicked(unbind));
            assert_eq!(ctx.settings.keymap.binding(keymap::id::FORWARD, 0), None);
            // Ok writes the key map.
            assert_eq!(o.event(ctx, &mut screen, Event::Clicked(o.el.ok.unwrap())), Outcome::Closed);
            assert!(ctx.settings.persistence.serialize().contains("InputForward=-1"));
        });
        if let Ok(file) = std::env::var("BANISHED_SETTINGS") {
            let _ = std::fs::remove_file(file);
        }
    }
}
