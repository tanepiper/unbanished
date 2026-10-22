//! The tutorial (`TutorialInstance`, the step machine `FUN_1400e7280`) and its window
//! (`TutorialUI`), setups, goals and images. See docs/TUTORIAL.md.
//!
//! A scenario's FinishTutorialGoal starts it (`FUN_1400dcf60`): the window opens at the
//! screen's right edge less 464, and each frame the current step's setups update and its goals
//! are tested; when they are all met the setups exit and the next step is entered.

mod definition;
mod goals;
mod highlight;
mod scenarios;
pub use definition::{load, Goal, Image, Setup, TutorialDef};
pub use scenarios::{scenario_game, scenarios, GameGoal};
#[cfg(test)]
pub use scenarios::scenario_params;

use unbanished_formats::pkg::Archive;
use unbanished_formats::scenario::{self, ImageExtra, TutorialPart};
use unbanished_ui::{Kind, NodeId, Ui};

use super::GameplayLoop;
use crate::loops::Ctx;

/// Game UI `+0x80..+0xa0`: what the player may do while a RestrictPlayer setup holds.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Restrict {
    /// The cells the placement must cover, `(x0, y0)` to `(x1, y1)`, and its rotation.
    pub rect: [i32; 4],
    pub rotation: i32,
    /// `+0x88`: the only template to place; `+0x90` the only one to select.
    pub place: String,
    pub select: String,
}

/// TutorialUI's elements (`FUN_1400e8c70`).
#[derive(Debug, Clone, Default)]
pub struct TutorialUi {
    pub root: Option<NodeId>,
    pub pre: Option<NodeId>,
    pub post: Option<NodeId>,
    pub table: Option<NodeId>,
    pub next: Option<NodeId>,
    pub quit: Option<NodeId>,
}

/// What the highlight points at: an element, or an entity in the world (`+0xe8`, projected).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Element(NodeId),
    Entity(usize),
}

/// `TutorialInstance`: `+0x10` the step, `+0x18` the window, `+0x20` its TutorialUI.
#[derive(Debug, Clone, Default)]
pub struct Tutorial {
    pub def: TutorialDef,
    pub step: usize,
    pub ui: Option<TutorialUi>,
    /// The highlight dialog (TutorialHighlightDialog): its root, arrow, target and angle, and
    /// its time (`+0xb8`).
    pub highlight: Option<(NodeId, Option<NodeId>, Target, i32, f32)>,
    /// The elements pressed since the last update (element flag `0x400000`).
    pub pressed: Vec<NodeId>,
    /// The step was entered (the window opened a frame before, `FUN_1400e7280`).
    pub entered: bool,
}

impl Tutorial {
    pub fn done(&self) -> bool {
        self.def.steps.len() <= self.step
    }
}

/// `CRC32` of a controller class's UTF-16 name.
fn class_id(name: &str) -> u32 {
    unbanished_formats::crc::type_id(name)
}

fn file(r: &str) -> String {
    r.split(':').next().unwrap_or("").to_ascii_lowercase()
}

/// `FUN_1400e7720`: lock or unlock an element and everything in it.
fn set_locked(ui: &mut Ui, n: NodeId, locked: bool) {
    use unbanished_formats::ui::flags::LOCKED;
    let node = ui.node_mut(n);
    if locked {
        node.flags |= LOCKED;
    } else {
        node.flags &= !LOCKED;
    }
    for c in ui.children(n) {
        set_locked(ui, c, locked);
    }
}

impl GameplayLoop {
    /// Every dialog's root (the user interface's dialogs).
    fn dialog_roots(&self) -> Vec<NodeId> {
        self.screen
            .as_ref()
            .map_or(Vec::new(), |s| s.dialogs.iter().map(|d| d.root).collect())
    }

    /// `FUN_1400e6d40`: an element by its path, `ToolbarDialog:<item>` or
    /// `DialogInstance:<controller>:<element>[:<template>]`.
    pub(super) fn tutorial_element(&self, ui: &Ui, path: &str) -> Option<NodeId> {
        let parts: Vec<&str> = path.split(':').collect();
        match parts.as_slice() {
            ["ToolbarDialog", item] => self.toolbar.as_ref()?.button(ui, item),
            ["DialogInstance", class, rest @ ..] => {
                let id = class_id(class);
                let name = rest.first().copied();
                let template = rest.get(1).copied();
                for (root, element) in self.controller_elements(ui, id, template) {
                    let Some(name) = name else {
                        return Some(element);
                    };
                    if let Some(n) = ui.find(element, name).or_else(|| ui.find(root, name)) {
                        return Some(n);
                    }
                }
                None
            }
            _ => None,
        }
    }

    /// The open windows with a controller of the class: (window root, controller element),
    /// an entity's only when its template's name is `template` (when given).
    fn controller_elements(
        &self,
        ui: &Ui,
        id: u32,
        template: Option<&str>,
    ) -> Vec<(NodeId, NodeId)> {
        let mut out = Vec::new();
        if id == class_id("TutorialUI") {
            if let Some(root) = self.tutorial.as_ref().and_then(|t| t.ui.as_ref()?.root) {
                out.push((root, root));
            }
        }
        for (root, ids) in self.utility.controllers() {
            if ids.contains(&id) {
                out.push((root, root));
            }
        }
        let town = self.sim.town.as_ref();
        for w in &self.entities.open {
            if let (Some(t), Some(town)) = (template, town) {
                let name = w.entity.template(town).to_ascii_lowercase();
                if !name.contains(&format!("\\{}.rsc", t.to_ascii_lowercase())) {
                    continue;
                }
            }
            for (c, element) in &w.controllers {
                if c.type_id == id {
                    out.push((w.root, *element));
                }
            }
        }
        let _ = ui;
        out
    }

    /// Open the tutorial: its window at the screen's right edge less 464 (`FUN_1401056c0`),
    /// the first step entered on the next update.
    pub(super) fn start_tutorial(&mut self, ctx: &mut Ctx, def: TutorialDef) {
        self.tutorial = Some(Tutorial {
            def,
            ..Default::default()
        });
        self.open_tutorial_window(ctx);
    }

    fn open_tutorial_window(&mut self, ctx: &mut Ctx) {
        let Some(window) = self.tutorial.as_ref().map(|t| t.def.window.clone()) else {
            return;
        };
        let Some(screen) = self.screen.as_mut() else {
            return;
        };
        let x = ctx.ui.screen[0] - 0x1d0;
        let root = self.utility.open_at(
            ctx.ui,
            screen,
            ctx.archive,
            &window,
            (x.max(0), 0),
            self.sim.town.as_ref().map_or(0, |t| t.cells.size as u32),
        );
        let Some(root) = root else { return };
        let f = |n: &str| ctx.ui.find(root, n);
        let ui = TutorialUi {
            root: Some(root),
            pre: f("preText"),
            post: f("postText"),
            table: f("imageTable"),
            next: f("nextButton"),
            quit: f("quitButton"),
        };
        // `FUN_1400e8c70`: the host's close button goes.
        if let Some(c) = f("buttonClose") {
            ctx.ui.set_visible(c, false);
        }
        if let Some(t) = &mut self.tutorial {
            t.ui = Some(ui);
        }
    }

    /// `FUN_1400e6730`: the step's setups enter; its texts and images fill the window.
    fn enter_step(&mut self, ctx: &mut Ctx) {
        let Some(t) = &self.tutorial else { return };
        let Some(step) = t.def.steps.get(t.step).cloned() else {
            return;
        };
        for s in &step.setups {
            self.setup_enter(ctx, s);
        }
        let Some(ui) = self.tutorial.as_ref().and_then(|t| t.ui.clone()) else {
            return;
        };
        for (n, key) in [(ui.pre, &step.pre), (ui.post, &step.post)] {
            if let Some(n) = n {
                // `UI::String::IsValid`: a key the table lacks shows nothing.
                let text = if key.is_empty() {
                    String::new()
                } else {
                    ctx.ui.res.string(step.strings.as_deref(), key)
                };
                let text = if &text == key { String::new() } else { text };
                ctx.ui.set_visible(n, !text.is_empty());
                ctx.ui.set_text(n, &text);
            }
        }
        if let Some(table) = ui.table {
            if let Kind::Control(unbanished_ui::Control::Table { cells, rows, .. }) =
                &mut ctx.ui.node_mut(table).kind
            {
                cells.clear();
                rows.clear();
            }
            for image in &step.images {
                let Some(cell) = self.image_element(ctx, image) else {
                    continue;
                };
                if let Kind::Control(unbanished_ui::Control::Table {
                    cells,
                    rows,
                    columns,
                    ..
                }) = &mut ctx.ui.node_mut(table).kind
                {
                    let (x, y) = (image.column.max(0) as usize, image.row.max(0) as usize);
                    if columns.len() <= x {
                        columns.resize(x + 1, 0);
                    }
                    if rows.len() <= y {
                        rows.resize(y + 1, 0);
                    }
                    cells.retain(|c| (c.0, c.1) != (x, y));
                    cells.push((x, y, cell));
                }
            }
        }
    }

    /// `TutorialImage` slot `+0x48`: the element, with a toolbar item's icon or key, or a
    /// key-map action's key.
    fn image_element(&self, ctx: &mut Ctx, image: &Image) -> Option<NodeId> {
        let element = ctx.ui.instantiate(image.element.as_deref()?)?;
        match &image.extra {
            ImageExtra::None => {}
            ImageExtra::ToolbarIcon(r) => {
                let sprite = self
                    .toolbar
                    .as_ref()
                    .and_then(|t| t.entry_sprite(r))
                    .and_then(|(sheet, name)| ctx.ui.res.sprite(sheet.as_deref(), &name));
                if let (Some(s), Some(img)) = (sprite, find_kind(ctx.ui, element, is_image)) {
                    if let Kind::Image { sprite, .. } = &mut ctx.ui.node_mut(img).kind {
                        *sprite = Some(s);
                    }
                }
            }
            ImageExtra::HotKey(r) => {
                let action = self.toolbar.as_ref().and_then(|t| t.entry_action(r));
                let name = action.and_then(|a| {
                    crate::ui::toolbar::key_name(ctx.ui, &ctx.settings.keymap, a as i32)
                });
                if let (Some(name), Some(text)) = (name, find_kind(ctx.ui, element, is_text)) {
                    ctx.ui.set_text(text, &name);
                }
            }
            ImageExtra::InputKey(a) => {
                let name = crate::ui::toolbar::key_name(ctx.ui, &ctx.settings.keymap, *a);
                if let (Some(name), Some(text)) = (name, find_kind(ctx.ui, element, is_text)) {
                    ctx.ui.set_text(text, &name);
                }
            }
        }
        Some(element)
    }

    fn setup_enter(&mut self, ctx: &mut Ctx, s: &Setup) {
        match s {
            // `FUN_1400e7ac0`: everything locked but the listed elements; the listed dialogs
            // close; the highlight points at its element.
            Setup::Dialog { close, .. } => {
                self.apply_locks(ctx.ui, s);
                for path in close {
                    self.close_dialogs(ctx, path);
                }
                self.update_highlight(ctx, s, false);
            }
            // `FUN_1400e75a0`: the camera goes to the cells.
            Setup::Camera(c) => {
                self.focus_cells(c[0], c[1], c[2], c[3]);
            }
            // `FUN_1400e8230`.
            Setup::Speed { speed, unpause } => {
                if let Some(i) = super::SPEED_LEVELS.iter().position(|&l| l as i32 == *speed) {
                    self.speed = i as u8;
                }
                if *unpause {
                    self.paused = false;
                }
                self.update_hud(ctx.ui);
            }
            // `FUN_1400e83f0`: the player may place and select only the templates, there;
            // with a template to place the camera goes there.
            Setup::Restrict {
                rect,
                rotation,
                place,
                select,
            } => {
                self.restrict = Some(Restrict {
                    rect: *rect,
                    rotation: *rotation,
                    place: place.clone(),
                    select: select.clone(),
                });
                if !place.is_empty() {
                    self.focus_cells(rect[0], rect[1], rect[2], rect[3]);
                }
            }
        }
    }

    fn setup_update(&mut self, ctx: &mut Ctx, s: &Setup) {
        // `FUN_1400e7990`: the locks again (new dialogs appear) and the highlight follows.
        if let Setup::Dialog { .. } = s {
            self.apply_locks(ctx.ui, s);
            self.update_highlight(ctx, s, true);
        }
    }

    fn setup_exit(&mut self, ctx: &mut Ctx, s: &Setup) {
        match s {
            // `FUN_1400e7930`: everything unlocked; the highlight closes.
            Setup::Dialog { .. } => {
                for root in self.dialog_roots() {
                    set_locked(ctx.ui, root, false);
                }
                self.close_highlight(ctx);
            }
            // `FUN_1400e8360`: the restriction lifts; with a template to place, its tool goes.
            Setup::Restrict { place, .. } => {
                self.restrict = None;
                if !place.is_empty() {
                    self.end_placement(ctx);
                }
            }
            _ => {}
        }
    }

    /// `FUN_1400e7ec0`: every dialog locked (`FUN_1400e7800`, each root and its children),
    /// then the listed elements unlocked.
    fn apply_locks(&mut self, ui: &mut Ui, s: &Setup) {
        let Setup::Dialog { enable, .. } = s else {
            return;
        };
        for root in self.dialog_roots() {
            set_locked(ui, root, true);
        }
        for path in enable {
            if let Some(n) = self.tutorial_element(ui, path) {
                set_locked(ui, n, false);
            }
        }
    }

    /// `FUN_1400e7ac0`'s close list: an entity's window (and its selection) or a utility
    /// window with the controller.
    fn close_dialogs(&mut self, ctx: &mut Ctx, path: &str) {
        let parts: Vec<&str> = path.split(':').collect();
        let ["DialogInstance", class, ..] = parts.as_slice() else {
            return;
        };
        let id = class_id(class);
        let Some(screen) = self.screen.as_mut() else {
            return;
        };
        let roots: Vec<NodeId> = self
            .entities
            .open
            .iter()
            .filter(|w| w.controllers.iter().any(|(c, _)| c.type_id == id))
            .map(|w| w.root)
            .collect();
        for root in roots {
            self.entities.open.retain(|w| w.root != root);
            screen.remove_root(ctx.ui, root);
            self.selection.selected = None;
        }
        self.utility.close_with(ctx.ui, screen, id);
    }

    /// `FUN_1400e7280`: the step machine, after the frame's iterations.
    pub(super) fn update_tutorial(&mut self, ctx: &mut Ctx, dt: f32) {
        if self.tutorial.as_ref().is_none_or(|t| t.done()) {
            return;
        }
        if self.tutorial.as_ref().is_some_and(|t| t.ui.is_none()) {
            self.open_tutorial_window(ctx);
            return;
        }
        if self.tutorial.as_ref().is_some_and(|t| !t.entered) {
            if let Some(t) = &mut self.tutorial {
                t.entered = true;
            }
            self.enter_step(ctx);
        }
        let Some(step) = self
            .tutorial
            .as_ref()
            .and_then(|t| t.def.steps.get(t.step).cloned())
        else {
            return;
        };
        for s in &step.setups {
            self.setup_update(ctx, s);
        }
        let met = step
            .goals
            .iter()
            .filter(|g| self.goal_met(ctx.ui, ctx.archive, g))
            .count();
        if let Some(t) = &mut self.tutorial {
            t.pressed.clear();
        }
        self.place_highlight(ctx.ui, dt);
        if met < step.goals.len() {
            return;
        }
        for s in &step.setups {
            self.setup_exit(ctx, s);
        }
        let Some(t) = &mut self.tutorial else { return };
        t.step += 1;
        if !t.done() {
            self.enter_step(ctx);
            return;
        }
        // Done: the next scenario starts (`FUN_1400e6240`, none below 0).
        let next = self.tutorial.as_ref().map_or(-1, |t| t.def.next);
        if 0 <= next {
            self.next_scenario = Some(next as usize);
        }
        // Done: the window closes (`FUN_140106f50`).
        if let (Some(root), Some(screen)) = (
            self.tutorial.as_ref().and_then(|t| t.ui.as_ref()?.root),
            self.screen.as_mut(),
        ) {
            self.utility.close_root(ctx.ui, screen, root);
        }
    }

    /// The tutorial's quit button (`0x579`, `FUN_1400e8bb0`).
    pub(super) fn tutorial_quit(&self, node: NodeId) -> bool {
        self.tutorial
            .as_ref()
            .and_then(|t| t.ui.as_ref()?.quit)
            .is_some_and(|q| q == node)
    }

    /// TutorialUI's refresh (`FUN_1400e8bd0`): its buttons shown while usable.
    pub(super) fn refresh_tutorial_ui(&self, ui: &mut Ui) {
        let Some(t) = self.tutorial.as_ref().and_then(|t| t.ui.as_ref()) else {
            return;
        };
        for b in [t.next, t.quit].into_iter().flatten() {
            let usable = ui.node(b).usable();
            ui.set_visible(b, usable);
        }
    }
}

fn is_image(k: &Kind) -> bool {
    matches!(k, Kind::Image { .. })
}

fn is_text(k: &Kind) -> bool {
    matches!(k, Kind::Text(_) | Kind::Label { .. })
}

/// `Element::FindElementType`: the first element of a kind in `n`.
fn find_kind(ui: &Ui, n: NodeId, f: fn(&Kind) -> bool) -> Option<NodeId> {
    if f(&ui.node(n).kind) {
        return Some(n);
    }
    ui.children(n).into_iter().find_map(|c| find_kind(ui, c, f))
}

impl GameplayLoop {}
