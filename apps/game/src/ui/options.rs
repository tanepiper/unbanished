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
//! `crate::saves::autosave` and auto slow-down the disaster slow-down.
//! See docs/OPTIONS.md.

mod controls;
mod events;
mod fill;
mod rebind;
use controls::{
    action_name, active_tab, check, combo_selected, fill_combo, key_name, set_active_tab,
    set_check, set_combo, set_slider, slider, slider_value, table_text,
};

use unbanished_ui::{Control, Event, Kind, NodeId, Ui};

use crate::keymap::{self, Binding, KeyMap};
use crate::loops::{Ctx, Screen};
use crate::menu::{combo_list, select_combo};
use crate::settings::{AudioConfig, GameOptions, InputConfig};

const DIALOG: &str = "Dialog/Options.rsc:resource";
const BIND_DIALOG: &str = "Dialog/Options.rsc:inputBind";
const BOUND_DIALOG: &str = "Dialog/Options.rsc:inputBound";
const STRINGS: &str = "Dialog/StringTable.rsc";

/// Escape's shortcut code in `unbanished_ui` (the Cancel button carries action 8, Escape).
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

    fn is(&self, ui: &Ui, element: Option<NodeId>, node: NodeId) -> bool {
        element.is_some_and(|e| ui.descendant(e, node))
    }

    fn close(&mut self, ctx: &mut Ctx, screen: &mut Screen) {
        self.close_rebind(ctx, screen);
        screen.remove_root(ctx.ui, self.root);
    }

    /// Per frame: the bind box listens from the frame after it opened (`FUN_140077150`).
    pub fn update(&mut self) {
        if let Some(Rebind::Waiting { armed, .. }) = &mut self.rebind {
            *armed = true;
        }
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests;
