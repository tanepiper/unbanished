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
pub const CITIZEN_UI: u32 = 0x03e455a9;
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
    pub citizen: Option<CitizenInfo>,
}

/// What CitizenUI's refresh (`FUN_140065440`) reads from the Citizen and CitizenAI components.
#[derive(Debug, Clone)]
pub struct CitizenInfo {
    /// `+0x28`, `+0x38`, `+0x3c`.
    pub name: String,
    pub male: bool,
    pub age: f32,
    /// `+0x210 >= D+0x118`.
    pub educated: bool,
    /// `+0x1e4`: index into `Dialog\StringTable.rsc:clothing`.
    pub clothing: i32,
    /// 0 none, 1 iron, 2 steel (`+0xac`, `+0xa8` vs `D+0x110`).
    pub tool: i32,
    /// Profession shown: `(string table, key)`.
    pub profession: Option<(Option<String>, String)>,
    pub health: f32,
    pub health_max: f32,
    pub happiness: f32,
    pub happiness_max: f32,
    /// CitizenAI `+0x28`, or 16 while working away from the citizen's workplace.
    pub activity: i32,
    /// Profession button enabled (`+0x248` work or `+0x208` school).
    pub has_work: bool,
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
/// refresh `FUN_140080570` shows icon `i` when status bit `i` is set. Only a controller with a
/// config (`+0x10`) whose `+0x20` flag is clear additionally requires the host page to be 2 or 3;
/// the shipped entity hosts attach StatusIconUI without a config.
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

/// `CitizenUI` (vtable `0x140161b30`): bind `FUN_140065b00`, refresh `FUN_140065440`, close
/// `FUN_140065a90`, events `FUN_140065a50`.
pub struct CitizenUi {
    male: Option<NodeId>,
    female: Option<NodeId>,
    name: Option<(NodeId, String)>,
    age: Option<NodeId>,
    profession: Option<NodeId>,
    educated_yes: Option<NodeId>,
    educated_no: Option<NodeId>,
    clothing: Option<NodeId>,
    tool: Option<NodeId>,
    name_is: Option<(NodeId, String)>,
    action: Option<NodeId>,
    hearts: Option<NodeId>,
    stars: Option<NodeId>,
    /// Host buttons (element ids 0x227, 0x226, 0x228).
    button_profession: Option<NodeId>,
}

impl CitizenUi {
    pub fn bind(ui: &Ui, element: NodeId, root: NodeId) -> CitizenUi {
        let f = |n: &str| ui.find(element, n);
        let param = |n: &str| f(n).map(|id| (id, text_of(ui, id)));
        CitizenUi {
            male: f("imageMale"),
            female: f("imageFemale"),
            name: param("textName"),
            age: f("numberAge"),
            profession: f("textProfession"),
            educated_yes: f("textEducatedYes"),
            educated_no: f("textEducatedNo"),
            clothing: f("textClothing"),
            tool: f("textTool"),
            name_is: param("textNameIs"),
            action: f("textAction"),
            hearts: f("groupHearts"),
            stars: f("groupStars"),
            button_profession: ui.find(root, "buttonProfession"),
        }
    }

    pub fn refresh(&self, ui: &mut Ui, data: &EntityData) {
        let Some(c) = &data.citizen else { return };
        let show = |ui: &mut Ui, n: Option<NodeId>, v: bool| {
            if let Some(n) = n {
                ui.set_visible(n, v);
            }
        };
        show(ui, self.female, !c.male);
        show(ui, self.male, c.male);
        show(ui, self.educated_no, !c.educated);
        show(ui, self.educated_yes, c.educated);
        for (id, template) in [&self.name, &self.name_is].into_iter().flatten() {
            ui.set_text(*id, &template.replace("@0", &c.name));
        }
        // `DAT_140178898` = 5.
        if let Some(n) = self.age {
            ui.set_value(n, (c.age * 5.0) as i32 as f32);
        }
        if let Some(n) = self.clothing {
            let t = ui
                .res
                .string_at(r"Dialog\StringTable.rsc:clothing", c.clothing);
            ui.set_text(n, &t);
        }
        if let Some(n) = self.tool {
            let t = ui.res.string_at(r"Dialog\StringTable.rsc:toolType", c.tool);
            ui.set_text(n, &t);
        }
        if let (Some(n), Some((table, key))) = (self.profession, &c.profession) {
            let t = ui.res.string(table.as_deref(), key);
            ui.set_text(n, &t);
        }
        crate::utility::StatusBar::set_meter(ui, self.hearts, c.health, c.health_max);
        crate::utility::StatusBar::set_meter(ui, self.stars, c.happiness, c.happiness_max);
        if let Some(b) = self.button_profession {
            ui.node_mut(b).enabled = c.has_work;
        }
        if let Some(n) = self.action {
            let t = ui
                .res
                .string_at(r"Dialog\StringTable.rsc:citizenActions", c.activity);
            ui.set_text(n, &t);
        }
    }
}

pub enum Controller {
    Citizen(CitizenUi),
    Storage(StorageUi),
    Capacity(StorageCapacityUi),
    WorkPlace(WorkPlaceUi),
    StatusIcon(StatusIconUi),
    /// Attached, refresh not ported (logged once with its id).
    #[allow(dead_code)]
    Pending(u32),
}

impl Controller {
    pub fn bind(
        ui: &mut Ui,
        archive: &Archive,
        type_id: u32,
        element: NodeId,
        root: NodeId,
        has_config: bool,
    ) -> Controller {
        let bound = match type_id {
            CITIZEN_UI => Some(Controller::Citizen(CitizenUi::bind(ui, element, root))),
            STORAGE_UI => StorageUi::bind(ui, element, archive).map(Controller::Storage),
            STORAGE_CAPACITY_UI => StorageCapacityUi::bind(ui, element).map(Controller::Capacity),
            WORKPLACE_UI => Some(Controller::WorkPlace(WorkPlaceUi { element })),
            // Not traced: the config's `+0x20` flag (no shipped StatusIconUI has a config).
            // Not ported: BuildUI (FUN_1400b7410/FUN_1400b6f20) and DestroyUI
            // (FUN_140066ab0/FUN_140066460) read the StateMachine (`+0x50/+0x52`, 6/7 damaged or
            // destroyed), Build and Destroy components, which arrive with construction.
            BUILD_UI | DESTROY_UI => None,
            STATUS_ICON_UI => {
                StatusIconUi::bind(ui, element, !has_config).map(Controller::StatusIcon)
            }
            _ => None,
        };
        bound.unwrap_or_else(|| {
            log::info!("entity controller {type_id:#010x}: refresh not ported");
            Controller::Pending(type_id)
        })
    }

    pub fn refresh(&mut self, ui: &mut Ui, archive: &Archive, data: &EntityData) {
        match self {
            Controller::Citizen(c) => c.refresh(ui, data),
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
