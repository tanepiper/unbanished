//! Entity-window controllers (`UI::DialogController` subclasses attached by UIDescription):
//! slot `+0x20` binds, `+0x38` refreshes, `+0x48` handles element events. See
//! docs/BUILDING-WINDOWS.md. Refresh is read-only: nothing here touches simulation state or the
//! game Random.

use std::collections::BTreeMap;

use banished_formats::game::{raw_material_description, RawMaterialDescription};
use banished_formats::pkg::Archive;
use banished_game::stock::{RawMaterial, Storage};
use banished_ui::{Control, Kind, NodeId, Ui};

/// CRC32 (UTF-16LE) of the controller class names.
pub const STORAGE_UI: u32 = 0x6186b6a4;
pub const STORAGE_CAPACITY_UI: u32 = 0x3061173b;
pub const WORKPLACE_UI: u32 = 0xbdb788dc;
pub const STATUS_ICON_UI: u32 = 0xdf3a1bb2;
pub const BUILD_UI: u32 = 0x293e60a1;
pub const DESTROY_UI: u32 = 0x2e56a4c8;

/// What a controller reads from the selected entity.
pub struct EntityData<'a> {
    pub storage: Option<&'a Storage>,
    pub materials: &'a [RawMaterial],
    /// StatusIconComponent `+0xf0`.
    pub status: u32,
    /// WorkPlaceComponent `+0x48`: the workplace has a profession.
    pub workplace_profession: bool,
    /// Host `pages` active page (StatusIconUI shows icons only on pages 2 and 3).
    pub active_page: usize,
}

fn text_of(ui: &Ui, id: NodeId) -> String {
    match &ui.node(id).kind {
        Kind::Text(t) | Kind::Label { text: t, .. } => t.text.clone(),
        _ => String::new(),
    }
}

/// `System::String::operator<` / `StrCompare`: byte comparison on signed `char`.
fn str_less(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    for i in 0.. {
        let (x, y) = (
            a.get(i).map_or(0, |&c| c as i8),
            b.get(i).map_or(0, |&c| c as i8),
        );
        if x != y {
            return x < y;
        }
        if x == 0 {
            return false;
        }
    }
    false
}

fn material_file(reference: &str) -> String {
    reference.split(':').next().unwrap_or("").replace('\\', "/")
}

/// `StorageUI` (vtable `0x140165d80`): bind `FUN_1400bdcf0`, refresh `FUN_1400bd920`.
pub struct StorageUi {
    /// Per table cell: `labelStorage`, `numberCount`.
    cells: Vec<(Option<NodeId>, Option<NodeId>)>,
    descriptions: BTreeMap<usize, Option<RawMaterialDescription>>,
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
        let materials = archive
            .names()
            .into_iter()
            .filter(|n| n.ends_with("_rawmaterial.crs"))
            .count();
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

    fn description(
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

/// `StorageCapacityUI` (vtable `0x140163d90`): bind `FUN_140080ad0` (the element's progress
/// bar), refresh `FUN_1400809a0`: fill = used / capacity, text `%i` of `(int)(fill * 100 + 0.5)`.
pub struct StorageCapacityUi {
    progress: NodeId,
    /// The progress content text before parameter substitution (`@0% full`).
    text: Option<(NodeId, String)>,
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

/// `WorkPlaceUI` (vtable `0x140164450`): bind `FUN_14008ce10`, refresh `FUN_14008c550`,
/// events `FUN_14008ccb0`. Without a profession (`WorkPlaceComponent +0x48`, e.g. storage
/// barns and houses) the whole element is hidden.
pub struct WorkPlaceUi {
    element: NodeId,
}

impl WorkPlaceUi {
    pub fn refresh(&self, ui: &mut Ui, data: &EntityData) {
        ui.set_visible(self.element, data.workplace_profession);
        // Not ported: worker counts and the +/- worker and job commands (0x1450..0x1454), which
        // need workplace profession assignment in the simulation.
    }
}

/// `StatusIconUI` (vtable `0x140163cd0`): bind `FUN_140080740` caches the `icons` children;
/// refresh `FUN_140080570` shows icon `i` when status bit `i` is set and the host shows page 2
/// or 3 (unless the controller config's `+0x20` flag is set).
pub struct StatusIconUi {
    icons: Vec<NodeId>,
    always: bool,
}

impl StatusIconUi {
    pub fn bind(ui: &Ui, element: NodeId, always: bool) -> Option<StatusIconUi> {
        let icons = ui.find(element, "icons")?;
        Some(StatusIconUi {
            icons: ui.children(icons),
            always,
        })
    }

    pub fn refresh(&self, ui: &mut Ui, data: &EntityData) {
        let page_ok = self.always || matches!(data.active_page, 2 | 3);
        for (i, &icon) in self.icons.iter().enumerate() {
            let bit = 1u32.rotate_left(i as u32);
            ui.set_visible(icon, page_ok && data.status & bit != 0);
        }
    }
}

pub enum Controller {
    Storage(StorageUi),
    Capacity(StorageCapacityUi),
    WorkPlace(WorkPlaceUi),
    StatusIcon(StatusIconUi),
    /// Attached, refresh not ported (logged once with its id).
    Pending(u32),
}

impl Controller {
    pub fn bind(ui: &mut Ui, archive: &Archive, type_id: u32, element: NodeId) -> Controller {
        let bound = match type_id {
            STORAGE_UI => StorageUi::bind(ui, element, archive).map(Controller::Storage),
            STORAGE_CAPACITY_UI => StorageCapacityUi::bind(ui, element).map(Controller::Capacity),
            WORKPLACE_UI => Some(Controller::WorkPlace(WorkPlaceUi { element })),
            // The config `+0x20` flag is not traced for shipped configs; entity hosts have none.
            STATUS_ICON_UI => StatusIconUi::bind(ui, element, false).map(Controller::StatusIcon),
            _ => None,
        };
        bound.unwrap_or_else(|| {
            log::info!("entity controller {type_id:#010x}: refresh not ported");
            Controller::Pending(type_id)
        })
    }

    pub fn refresh(&mut self, ui: &mut Ui, archive: &Archive, data: &EntityData) {
        match self {
            Controller::Storage(c) => c.refresh(ui, archive, data),
            Controller::Capacity(c) => c.refresh(ui, data),
            Controller::WorkPlace(c) => c.refresh(ui, data),
            Controller::StatusIcon(c) => c.refresh(ui, data),
            Controller::Pending(_) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signed_byte_order() {
        assert!(str_less("Coal", "Iron"));
        assert!(str_less("Log", "Logs"));
        assert!(!str_less("Logs", "Log"));
        // Bytes >= 0x80 compare as negative `char`.
        assert!(str_less("Ä", "A"));
    }
}
