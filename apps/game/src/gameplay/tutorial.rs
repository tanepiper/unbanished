//! The tutorial (`TutorialInstance`, the step machine `FUN_1400e7280`) and its window
//! (`TutorialUI`), setups, goals and images. See docs/TUTORIAL.md.
//!
//! A scenario's FinishTutorialGoal starts it (`FUN_1400dcf60`): the window opens at the
//! screen's right edge less 464, and each frame the current step's setups update and its goals
//! are tested; when they are all met the setups exit and the next step is entered.

mod setup;
mod window;

mod definition;
mod goals;
mod highlight;
mod scenarios;
pub use definition::{load, Goal, Image, Setup, TutorialDef};
#[cfg(test)]
pub use scenarios::scenario_params;
pub use scenarios::{scenario_game, scenarios, GameGoal};

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
    /// Open the tutorial: its window at the screen's right edge less 464 (`FUN_1401056c0`),
    /// the first step entered on the next update.
    pub(super) fn start_tutorial(&mut self, ctx: &mut Ctx, def: TutorialDef) {
        self.tutorial = Some(Tutorial {
            def,
            ..Default::default()
        });
        self.open_tutorial_window(ctx);
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
}

impl GameplayLoop {}
