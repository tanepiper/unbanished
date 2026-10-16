//! `AchievementDialog` (`Dialog/Achievement.rsc`, the main menu's id 8; vtable `0x140161090`:
//! bind `FUN_14005ddf0`, refresh `FUN_14005dba0`, events `FUN_14005dda0`). See
//! docs/ACHIEVEMENTS.md.
//!
//! `tableAchievement` holds a `groupAchievement` per achievement: `imageIcon` its sprite,
//! `textTitle`, `textDescription`, and `imageStar` shown once it is awarded; the icon and texts
//! are dimmed until then. `textComplete` shows the share awarded (`@0` percent, rounded).
//! `buttonOk` (id 100) closes it.

use banished_ui::{Kind, NodeId, Ui};

use crate::achievements::Achievement;
use crate::loops::Screen;

const DIALOG: &str = "Dialog/Achievement.rsc";
const ROW: &str = r"Dialog\Achievement.rsc:groupAchievement";

struct Row {
    icon: Option<NodeId>,
    star: Option<NodeId>,
    title: Option<NodeId>,
    description: Option<NodeId>,
}

pub struct AchievementDialog {
    pub root: NodeId,
    ok: Option<NodeId>,
    rows: Vec<Row>,
    complete: Option<(NodeId, String)>,
}

impl AchievementDialog {
    pub fn open(ui: &mut Ui, screen: &mut Screen, list: &[Achievement]) -> Option<Self> {
        let index = screen.add_modal(ui, DIALOG)?;
        let root = screen.dialogs[index].root;
        let find = |ui: &Ui, n: &str| ui.find(root, n);
        let ok = find(ui, "buttonOk");
        let mut rows = Vec::new();
        if let Some(table) = find(ui, "tableAchievement") {
            for (i, a) in list.iter().enumerate() {
                let Some(cell) = ui.instantiate(ROW) else {
                    continue;
                };
                ui.set_table_child(table, i, cell);
                let f = |ui: &Ui, n: &str| ui.find(cell, n);
                let row = Row {
                    icon: f(ui, "imageIcon"),
                    star: f(ui, "imageStar"),
                    title: f(ui, "textTitle"),
                    description: f(ui, "textDescription"),
                };
                let d = &a.desc;
                if let (Some(n), Some(s)) =
                    (row.icon, ui.res.sprite(d.sprites.as_deref(), &d.sprite))
                {
                    if let Kind::Image { sprite, .. } = &mut ui.node_mut(n).kind {
                        *sprite = Some(s);
                    }
                }
                if let Some(n) = row.star {
                    ui.set_visible(n, false);
                }
                let title = ui.res.string(d.strings.as_deref(), &d.title);
                let description = ui.res.string(d.strings.as_deref(), &d.description);
                if let Some(n) = row.title {
                    ui.set_text(n, &title);
                }
                if let Some(n) = row.description {
                    ui.set_text(n, &description);
                }
                rows.push(row);
            }
        }
        let complete = find(ui, "textComplete").map(|n| (n, crate::building_ui::text_of(ui, n)));
        Some(AchievementDialog {
            root,
            ok,
            rows,
            complete,
        })
    }

    /// `FUN_14005dba0`: the awarded ones starred and bright, and the share done.
    pub fn refresh(&self, ui: &mut Ui, awarded: &[bool]) {
        let mut done = 0;
        for (i, row) in self.rows.iter().enumerate() {
            let won = awarded.get(i).copied().unwrap_or(false);
            if won {
                done += 1;
            }
            if let Some(n) = row.star {
                ui.set_visible(n, won);
            }
            let (icon, text) = if won {
                (0xffe0_e0e0, 0xffc0_c0c0)
            } else {
                (0xff30_3030, 0xffb0_b0b0)
            };
            if let Some(n) = row.icon {
                ui.node_mut(n).color = icon;
            }
            for n in [row.title, row.description].into_iter().flatten() {
                ui.node_mut(n).color = text;
            }
        }
        if let Some((n, template)) = &self.complete {
            let share = if self.rows.is_empty() {
                0
            } else {
                ((done as f32 / self.rows.len() as f32) * 100.0 + 0.5) as i32
            };
            ui.set_text(*n, &template.replace("@0", &share.to_string()));
        }
    }

    /// `FUN_14005dda0`: OK closes it.
    pub fn closes(&self, node: NodeId) -> bool {
        self.ok == Some(node)
    }
}

/// What is awarded, as stored (`Achievement<title>`).
pub fn awarded(list: &[Achievement], p: &crate::settings::Persistence) -> Vec<bool> {
    list.iter()
        .map(|a| {
            let mut v = false;
            p.get_bool("Achievement", &a.desc.title, &mut v);
            v
        })
        .collect()
}
