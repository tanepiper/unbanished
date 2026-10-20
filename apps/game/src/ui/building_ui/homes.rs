//! Houses: the residents table and the upgrade button.

use super::*;

/// `ResidenceUI` (type `0x1701e613`): bind `FUN_1400bad20`, refresh `FUN_1400ba6c0`. Each cell
/// of `residenceTable` (else `residenceLongTable`) has `imageMale`, `imageFemale`,
/// `textName`, `textAge`, `textProfession` and `buttonGoto` (id `300 + cell`).
pub struct ResidenceUi {
    rows: Vec<ResidentRow>,
}

pub(super) struct ResidentRow {
    male: Option<NodeId>,
    female: Option<NodeId>,
    name: Option<(NodeId, String)>,
    age: Option<(NodeId, String)>,
    profession: Option<(NodeId, String)>,
    goto: Option<NodeId>,
}

/// `UpgradeUI` (type `0x061d5f73`): bind `FUN_14008c260`, events `FUN_14008c160`. The element
/// is a button (id 33000) showing the toolbar sprite of UpgradeUIConfig `+0x20`'s template; a
/// click makes it the DestroyComponent's replacement (`+0x60`) and tears the building down
/// (`FUN_140123470`).
pub struct UpgradeUi {
    button: NodeId,
    template: Option<String>,
}

/// `UpgradeUIConfig`: `+0x20` the template to put in the building's place.
pub(super) fn upgrade_config(data: &[u8]) -> Option<String> {
    use unbanished_formats::crs;
    crs::parse(data, |r: &mut crs::Reader, _| {
        r.block(|r, _| r.external_ref())
    })
    .ok()
    .and_then(|(_, v)| v)
}

impl ResidenceUi {
    pub fn bind(ui: &mut Ui, element: NodeId) -> ResidenceUi {
        let table = ui
            .find(element, "residenceTable")
            .or_else(|| ui.find(element, "residenceLongTable"));
        // The table's cells, row by row (`Child(row × width + column)`): goto, name, gender,
        // age and profession across each row.
        let mut by_row: std::collections::BTreeMap<usize, Vec<NodeId>> = Default::default();
        if let Some(t) = table {
            if let unbanished_ui::Kind::Control(unbanished_ui::Control::Table { cells, .. }) =
                &ui.node(t).kind
            {
                for &(_, y, n) in cells {
                    by_row.entry(y).or_default().push(n);
                }
            }
        }
        let rows = by_row
            .values()
            .map(|cells| {
                let find = |ui: &Ui, n: &str| cells.iter().find_map(|&c| ui.find(c, n));
                let param = |ui: &Ui, n: &str| find(ui, n).map(|id| (id, text_of(ui, id)));
                ResidentRow {
                    male: find(ui, "imageMale"),
                    female: find(ui, "imageFemale"),
                    name: param(ui, "textName"),
                    age: param(ui, "textAge"),
                    profession: param(ui, "textProfession"),
                    goto: find(ui, "buttonGoto"),
                }
            })
            .collect();
        ResidenceUi { rows }
    }

    /// `FUN_1400ba6c0`: a row per resident: the sex's image, `@0` name, `@0` age (× 5 years),
    /// the profession (a child's or student's for those); the rest empty, their button off.
    pub fn refresh(&mut self, ui: &mut Ui, data: &EntityData) {
        let Some(residents) = &data.residents else {
            return;
        };
        for (i, row) in self.rows.iter().enumerate() {
            let resident = residents.get(i).map(|(_, c)| c);
            if let Some(n) = row.male {
                ui.set_visible(n, resident.is_some_and(|c| c.male));
            }
            if let Some(n) = row.female {
                ui.set_visible(n, resident.is_some_and(|c| !c.male));
            }
            let fill = |ui: &mut Ui, p: &Option<(NodeId, String)>, v: &str| {
                if let Some((n, t)) = p {
                    ui.set_text(*n, &t.replace("@0", v));
                }
            };
            match resident {
                Some(c) => {
                    fill(ui, &row.name, &c.name);
                    fill(ui, &row.age, &((c.age * 5.0) as i32).to_string());
                    if let Some((n, _)) = &row.profession {
                        let text = c
                            .profession
                            .as_ref()
                            .map(|(t, k)| ui.res.string(t.as_deref(), k))
                            .unwrap_or_default();
                        ui.set_text(*n, &text);
                    }
                }
                None => {
                    fill(ui, &row.name, "");
                    fill(ui, &row.age, "");
                    if let Some((n, t)) = &row.profession {
                        ui.set_text(*n, t);
                    }
                }
            }
            if let Some(b) = row.goto {
                ui.node_mut(b).enabled = resident.is_some();
            }
        }
    }

    /// `buttonGoto` (`300 + row`; the handler at `0x1400bacf0` → `FUN_1400bab70`): the
    /// resident at that place counting family by family, shown and selected
    /// (`FUN_140108510`).
    pub fn click(&self, ui: &Ui, node: NodeId, data: &EntityData) -> Option<WorkPlaceClick> {
        let i = self
            .rows
            .iter()
            .position(|r| r.goto.is_some_and(|b| b == node || ui.descendant(b, node)))?;
        let (c, _) = data.residents.as_ref()?.get(i)?;
        Some(WorkPlaceClick::ShowWorker(*c))
    }
}

impl UpgradeUi {
    pub fn bind(ui: &mut Ui, archive: &Archive, element: NodeId, config: Option<&str>) -> Self {
        let template = config
            .and_then(|c| archive.read_ref(&c.replace('\\', "/")).ok())
            .and_then(|d| upgrade_config(&d));
        // Its first child shows the template's ToolbarDescription sprite (`+0x20`, `+0x28`).
        let toolbar = template.as_deref().and_then(|t| {
            let file = material_file(t);
            archive
                .read_ref(&format!("{file}:toolbar"))
                .ok()
                .and_then(|b| unbanished_formats::game::toolbar_description(&b).ok())
        });
        if let (Some(&image), Some(t)) = (ui.children(element).first(), toolbar) {
            if let Some(s) = ui.res.sprite(t.sprite_sheet.as_deref(), &t.sprite) {
                if let Kind::Image { sprite, .. } = &mut ui.node_mut(image).kind {
                    *sprite = Some(s);
                }
            }
        }
        UpgradeUi {
            button: element,
            template,
        }
    }

    pub fn click(&self, ui: &Ui, node: NodeId, entity: usize) -> Option<WorkPlaceClick> {
        if node != self.button && !ui.descendant(self.button, node) {
            return None;
        }
        Some(WorkPlaceClick::Upgrade {
            entity,
            template: self.template.clone()?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ResidenceUI's table is 5 cells by 8 rows (`FUN_1400bad20` reads child `row × 5 +
    /// column`): every row gets its own goto, name, gender, age and profession.
    #[test]
    fn residence_rows_span_the_table_columns() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../game-data/original/WinData");
        let mut archive = Archive::default();
        for name in ["data1.pkg", "patch1.pkg", "patch0.pkg", "data0.pkg"] {
            let Ok(bytes) = std::fs::read(dir.join(name)) else {
                return;
            };
            archive.add(unbanished_formats::pkg::Package::parse(bytes).unwrap());
        }
        let mut ui = Ui::new(std::rc::Rc::new(archive));
        let element = ui.instantiate(r"Dialog\Residence.rsc:residence").unwrap();
        let r = ResidenceUi::bind(&mut ui, element);
        assert_eq!(r.rows.len(), 8);
        for row in &r.rows {
            assert!(row.goto.is_some() && row.name.is_some() && row.age.is_some());
            assert!(row.male.is_some() && row.female.is_some() && row.profession.is_some());
        }
        let names: std::collections::BTreeSet<_> = r
            .rows
            .iter()
            .map(|row| row.name.as_ref().unwrap().0)
            .collect();
        assert_eq!(names.len(), 8);
    }
}
