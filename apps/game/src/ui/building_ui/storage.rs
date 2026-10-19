//! Storage: the goods a storage holds and its capacity.

use super::*;

/// `StorageUI` (vtable `0x140165d80`): bind `FUN_1400bdcf0`, refresh `FUN_1400bd920`.
pub struct StorageUi {
    /// Per table cell: `labelStorage`, `numberCount`.
    cells: Vec<(Option<NodeId>, Option<NodeId>)>,
    descriptions: BTreeMap<usize, Option<RawMaterialDescription>>,
}

/// `StorageCapacityUI` (vtable `0x140163d90`): bind `FUN_140080ad0` (the element's progress
/// bar), refresh `FUN_1400809a0`: fill = used / capacity, text `%i` of `(int)(fill * 100 + 0.5)`.
pub struct StorageCapacityUi {
    progress: NodeId,
    /// The progress content text before parameter substitution (`@0% full`).
    text: Option<(NodeId, String)>,
}

pub(super) fn material_file(reference: &str) -> String {
    reference.split(':').next().unwrap_or("").replace('\\', "/")
}

impl StorageUi {
    /// `FUN_1400bdcf0`: size `storageTable` (or `storageTable2`) for every RawMaterialDescription
    /// (`ceil(n / columns)` rows, at least the resource's rows), fill it with copies of its cell
    /// element and cache each cell's label and count.
    pub fn bind(ui: &mut Ui, element: NodeId, archive: &Archive) -> Option<StorageUi> {
        let table = ui
            .find(element, "storageTable")
            .or_else(|| ui.find(element, "storageTable2"))?;
        let (columns, min_rows, template) = match &ui.node(table).kind {
            Kind::Control(Control::Table {
                columns,
                rows,
                cells,
                ..
            }) => (columns.len().max(1), rows.len(), cells.first().map(|c| c.2)),
            _ => return None,
        };
        let reference = template.map(|t| ui.node(t).reference.clone())?;
        let materials = crate::mods::raw_material_count(archive);
        let rows = materials.div_ceil(columns).max(min_rows);
        if let Kind::Control(Control::Table { cells, rows: r, .. }) = &mut ui.node_mut(table).kind {
            cells.clear();
            r.clear();
        }
        let mut out = Vec::new();
        for i in 0..rows * columns {
            let Some(cell) = ui.instantiate(&reference) else {
                continue;
            };
            ui.set_table_child(table, i, cell);
            out.push((ui.find(cell, "labelStorage"), ui.find(cell, "numberCount")));
        }
        Some(StorageUi {
            cells: out,
            descriptions: BTreeMap::new(),
        })
    }

    pub(super) fn description(
        &mut self,
        archive: &Archive,
        materials: &[RawMaterial],
        m: usize,
    ) -> Option<&RawMaterialDescription> {
        self.descriptions
            .entry(m)
            .or_insert_with(|| {
                let file = material_file(&materials.get(m)?.reference);
                raw_material_description(&archive.read_ref(&format!("{file}:rawmaterial")).ok()?)
                    .ok()
            })
            .as_ref()
    }

    /// `FUN_1400bd920`: the stored materials with a non-zero count, sorted by localized name
    /// (`FUN_140021d50` with `FUN_1400bd840`), one per cell; the remaining cells are hidden.
    pub fn refresh(&mut self, ui: &mut Ui, archive: &Archive, data: &EntityData) {
        let Some(storage) = data.storage else {
            return;
        };
        let mut rows = Vec::new();
        for item in storage.items.iter().filter(|i| i.count != 0) {
            let Some(d) = self.description(archive, data.materials, item.material) else {
                continue;
            };
            let d = d.clone();
            let name = ui.res.string(d.string_table.as_deref(), &d.name);
            rows.push((name, d, item.count));
        }
        // Names are unique, so any correct sort reproduces the original order.
        rows.sort_by(|a, b| {
            if str_less(&a.0, &b.0) {
                std::cmp::Ordering::Less
            } else if str_less(&b.0, &a.0) {
                std::cmp::Ordering::Greater
            } else {
                std::cmp::Ordering::Equal
            }
        });
        for (i, &(label, number)) in self.cells.iter().enumerate() {
            match rows.get(i) {
                Some((name, d, count)) => {
                    if let Some(l) = label {
                        if let Some(s) = ui.res.sprite(d.sprite_sheet.as_deref(), &d.name) {
                            if let Kind::Label { icon, .. } = &mut ui.node_mut(l).kind {
                                *icon = Some(s);
                            }
                        }
                        ui.set_text(l, name);
                        ui.set_visible(l, true);
                    }
                    if let Some(n) = number {
                        ui.set_value(n, *count as f32);
                        ui.set_visible(n, true);
                    }
                }
                None => {
                    for n in [label, number].into_iter().flatten() {
                        ui.set_visible(n, false);
                    }
                }
            }
        }
    }
}

impl StorageCapacityUi {
    pub fn bind(ui: &mut Ui, element: NodeId) -> Option<StorageCapacityUi> {
        let mut stack = vec![element];
        let mut progress = None;
        while let Some(n) = stack.pop() {
            if matches!(ui.node(n).kind, Kind::Control(Control::Progress { .. })) {
                progress = Some(n);
                break;
            }
            stack.extend(ui.children(n).into_iter().rev());
        }
        let progress = progress?;
        let content = match &ui.node(progress).kind {
            Kind::Control(Control::Progress { content, .. }) => *content,
            _ => None,
        };
        let text = content.map(|c| (c, text_of(ui, c)));
        Some(StorageCapacityUi { progress, text })
    }

    pub fn refresh(&self, ui: &mut Ui, data: &EntityData) {
        let Some(storage) = data.storage else {
            return;
        };
        let used = storage.used(data.materials);
        let fill = if storage.capacity == 0 {
            0.0
        } else {
            used as f32 / storage.capacity as f32
        };
        ui.set_value(self.progress, fill);
        if let Some((node, template)) = &self.text {
            let percent = (fill * 100.0 + 0.5) as i32;
            ui.set_text(*node, &template.replace("@0", &percent.to_string()));
        }
    }
}
