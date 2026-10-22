//! The professions window: its rows and spinners, going to a worker and changing a profession's count.

use super::*;

/// `ProfessionUI` (vtable `0x140163520`): one `professionRow` per profession, bound by
/// `FUN_14007c170` (OnOpened) and refreshed by `FUN_14007ba70`.
pub(super) struct ProfessionUi {
    /// Per profession: the spinner (`spinnerCount`, ids 300 + i) and its number edit.
    pub(super) spinners: Vec<Option<NodeId>>,
    /// Per profession: `numberAvailable`.
    pub(super) available: Vec<Option<NodeId>>,
    /// Per profession: `buttonGoto` (ids 500 + i).
    pub(super) gotos: Vec<Option<NodeId>>,
    /// `+0x70`: the citizen the goto buttons showed last.
    pub(super) cursor: i32,
}

/// Set a spinner's value and its number text.
pub(super) fn set_spinner(ui: &mut Ui, id: NodeId, value: i32) {
    let number = match &mut ui.node_mut(id).kind {
        Kind::Control(unbanished_ui::Control::Spinner {
            parts, value: v, ..
        }) => {
            *v = value;
            parts[0]
        }
        _ => None,
    };
    if let Some(n) = number {
        ui.set_text(n, &value.to_string());
    }
}

impl ProfessionUi {
    const ROW: &'static str = r"Dialog\Profession.rsc:professionRow";

    pub(super) fn bind(ui: &mut Ui, root: NodeId, archive: &Archive) -> ProfessionUi {
        let list = crate::mods::profession_list(archive);
        let mut spinners = Vec::new();
        let mut available = Vec::new();
        let mut gotos = Vec::new();
        let Some(table) = ui.find(root, "professionTable") else {
            return ProfessionUi {
                spinners,
                available,
                gotos: Vec::new(),
                cursor: 0,
            };
        };
        // The table holds the row template; rows replace it (`TableElement::SetSize/SetChild`).
        ui.node_mut(table).children.clear();
        for (i, reference) in list.iter().enumerate() {
            let Some(p) = archive
                .read_ref(&reference.replace('\\', "/"))
                .ok()
                .and_then(|d| profession(&d).ok())
            else {
                spinners.push(None);
                available.push(None);
                gotos.push(None);
                continue;
            };
            let Some(row) = ui.instantiate(Self::ROW) else {
                spinners.push(None);
                available.push(None);
                gotos.push(None);
                continue;
            };
            ui.set_table_child(table, i, row);
            let table_ref = p.string_table.as_deref();
            if let Some(label) = ui.find(row, "labelProfession") {
                let text = ui.res.string(table_ref, &p.name);
                ui.set_text(label, &text);
                ui.node_mut(label).tooltip = ui.res.string(table_ref, &p.tooltip);
                if let Some(s) = ui.res.sprite(p.sprite_sheet.as_deref(), &p.name) {
                    if let Kind::Label { icon, .. } = &mut ui.node_mut(label).kind {
                        *icon = Some(s);
                    }
                }
            }
            let spinner = ui.find(row, "spinnerCount");
            // The laborer row (manager `+0x30`) has no workplaces: no "of N" and no goto button.
            if i == 0 {
                for name in ["ofText", "numberAvailable", "buttonGoto"] {
                    if let Some(n) = ui.find(row, name) {
                        ui.set_visible(n, false);
                    }
                }
            }
            spinners.push(spinner);
            gotos.push(if i == 0 {
                None
            } else {
                ui.find(row, "buttonGoto")
            });
            available.push(if i == 0 {
                None
            } else {
                ui.find(row, "numberAvailable")
            });
        }
        ProfessionUi {
            spinners,
            available,
            gotos,
            cursor: 0,
        }
    }

    /// The goto button (`500 + i`) under `node`: its profession.
    pub(super) fn goto(&self, ui: &Ui, node: NodeId) -> Option<usize> {
        self.gotos
            .iter()
            .position(|g| g.is_some_and(|g| g == node || ui.descendant(g, node)))
    }

    /// `FUN_14007bf00`: with any working adult (grown, not at school), the next citizen after
    /// the last shown in the town's list with the profession; after as many misses as there
    /// are working adults, the one reached. `citizens` are the town's (citizen, profession,
    /// working adult).
    pub(super) fn next_worker(
        &mut self,
        profession: usize,
        citizens: &[(usize, usize, bool, u32)],
    ) -> Option<usize> {
        let adults = citizens.iter().filter(|c| c.2).count() as i32;
        if adults == 0 {
            return None;
        }
        let n = citizens.len() as i32;
        self.cursor += 1;
        if self.cursor < 0 {
            self.cursor = 0;
        }
        let mut tries = 0;
        loop {
            if n <= self.cursor {
                self.cursor = 0;
            }
            let c = citizens[self.cursor as usize];
            if (c.1 == profession && c.2) || adults <= tries {
                return Some(c.0);
            }
            self.cursor += 1;
            tries += 1;
        }
    }

    /// A spinner changed: its profession and value, clamped as `FUN_14007ba70` sets the
    /// limits (0 to its count plus the laborers; the laborer spinner is fixed).
    pub(super) fn changed(
        &self,
        ui: &Ui,
        node: NodeId,
        values: &StatusValues,
    ) -> Option<(usize, i32)> {
        let i = self
            .spinners
            .iter()
            .position(|s| s.is_some_and(|s| s == node || ui.descendant(s, node)))?;
        let s = self.spinners[i]?;
        let value = match &ui.node(s).kind {
            Kind::Control(unbanished_ui::Control::Spinner { value, .. }) => *value,
            _ => return None,
        };
        let count = |p: usize| values.professions.get(p).copied().unwrap_or(0) as i32;
        if i == 0 {
            return Some((0, count(0)));
        }
        Some((i, value.clamp(0, count(i) + count(0))))
    }

    /// `FUN_14007ba70`: worker counts; the laborer spinner is fixed, the others may take up to
    /// their count plus the laborers. Workplace capacity (`numberAvailable`) is 0 until
    /// workplaces exist.
    pub(super) fn update(&self, ui: &mut Ui, values: &StatusValues) {
        let count = |p: usize| values.professions.get(p).copied().unwrap_or(0) as i32;
        for (i, spinner) in self.spinners.iter().enumerate() {
            if let Some(s) = *spinner {
                set_spinner(ui, s, count(i));
                // `NumberEditElement::SetMinMax`: the laborers' fixed, the others' 0 to their
                // count plus the laborers.
                let (lo, hi) = if i == 0 {
                    (count(0), count(0))
                } else {
                    (0, count(i) + count(0))
                };
                ui.set_spinner_range(s, lo, hi);
            }
            if let Some(n) = self.available.get(i).copied().flatten() {
                let v = values.profession_capacity.get(i).copied().unwrap_or(0);
                ui.set_text(n, &v.to_string());
            }
        }
    }
}
