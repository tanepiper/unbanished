//! `ScenarioDialog` (vtable `0x140163700`: bind `FUN_14007d4e0`, refresh `FUN_14007d0b0`,
//! events `FUN_14007d450`), for the main menu's Scenario (id 9, `Dialog/Scenario.rsc`) and
//! Tutorial (id 12, `Dialog/Tutorial.rsc`, `+0xb8` set) buttons. See docs/TUTORIAL.md.
//!
//! `tableScenario` holds a `groupAchievement` per scenario of the list whose `+0x69` is the
//! dialog's flag (the tutorials, or the others): `imageIcon` its sprite, `textTitle`,
//! `textDescription`; it takes the mouse (`0x10000`, id `1000 + i`) and pressing it chooses it
//! (event 2), `selectSave` showing on the chosen one. `buttonOk` (`0x65`) is enabled once one
//! is chosen and starts it (`FUN_14007d180`); `buttonCancel` (100) closes the dialog.

use unbanished_formats::scenario::Scenario;
use unbanished_ui::{Kind, NodeId, Ui};

use crate::loops::Screen;

const ROW: &str = r"Dialog\Scenario.rsc:groupAchievement";

struct Row {
    group: NodeId,
    select: Option<NodeId>,
    /// The scenario's index in the list.
    index: usize,
}

pub struct ScenarioDialog {
    pub root: NodeId,
    ok: Option<NodeId>,
    cancel: Option<NodeId>,
    rows: Vec<Row>,
    /// `+0xc8`: the chosen row.
    chosen: Option<usize>,
}

/// What a click asks of the menu.
pub enum ScenarioClick {
    Close,
    Start(usize),
    Handled,
}

impl ScenarioDialog {
    pub fn open(
        ui: &mut Ui,
        screen: &mut Screen,
        reference: &str,
        tutorials: bool,
        list: &[(String, Scenario)],
    ) -> Option<Self> {
        let index = screen.add_modal(ui, reference)?;
        let root = screen.dialogs[index].root;
        let find = |ui: &Ui, n: &str| ui.find(root, n);
        let mut rows = Vec::new();
        if let Some(table) = find(ui, "tableScenario") {
            for (i, (_, s)) in list.iter().enumerate() {
                if s.b69 != tutorials {
                    continue;
                }
                let Some(cell) = ui.instantiate(ROW) else {
                    continue;
                };
                ui.set_table_child(table, rows.len(), cell);
                ui.node_mut(cell).flags |= unbanished_formats::ui::flags::MOUSE;
                let a = &s.achievement;
                if let Some(n) = ui.find(cell, "imageIcon") {
                    let sprite = ui.res.sprite(a.sprites.as_deref(), &a.sprite);
                    if let Kind::Image { sprite: s, .. } = &mut ui.node_mut(n).kind {
                        *s = sprite;
                    }
                }
                for (name, key) in [("textTitle", &a.title), ("textDescription", &a.description)] {
                    if let Some(n) = ui.find(cell, name) {
                        let text = ui.res.string(a.strings.as_deref(), key);
                        ui.set_text(n, &text);
                    }
                }
                rows.push(Row {
                    group: cell,
                    select: ui.find(cell, "selectSave"),
                    index: i,
                });
            }
        }
        let d = ScenarioDialog {
            root,
            ok: find(ui, "buttonOk"),
            cancel: find(ui, "buttonCancel"),
            rows,
            chosen: None,
        };
        d.refresh(ui);
        Some(d)
    }

    /// `FUN_14007d0b0`: the chosen row's `selectSave` shown; OK enabled once one is chosen.
    fn refresh(&self, ui: &mut Ui) {
        for (i, r) in self.rows.iter().enumerate() {
            if let Some(s) = r.select {
                ui.set_visible(s, self.chosen == Some(i));
            }
        }
        if let Some(ok) = self.ok {
            ui.node_mut(ok).enabled = self.chosen.is_some();
        }
    }

    /// `FUN_14007d450`.
    pub fn click(&mut self, ui: &mut Ui, node: NodeId) -> Option<ScenarioClick> {
        if self.cancel == Some(node) {
            return Some(ScenarioClick::Close);
        }
        if self.ok == Some(node) {
            return self
                .chosen
                .map(|i| ScenarioClick::Start(self.rows[i].index));
        }
        let i = self
            .rows
            .iter()
            .position(|r| r.group == node || ui.descendant(r.group, node))?;
        self.chosen = Some(i);
        self.refresh(ui);
        Some(ScenarioClick::Handled)
    }
}
