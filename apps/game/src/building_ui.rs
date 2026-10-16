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
    /// The building's WorkPlaceComponent, when it has a profession (`+0x48`).
    pub workplace: Option<WorkPlaceInfo>,
    /// The building's Build and ClearArea components (a construction site).
    pub build: Option<BuildInfo>,
    /// Host `pages` active page (StatusIconUI shows icons only on pages 2 and 3).
    pub active_page: usize,
    pub citizen: Option<CitizenInfo>,
    /// The building's DestroyComponent.
    pub destroy: Option<DestroyInfo>,
}

/// What DestroyUI reads (`FUN_140066460`): `FUN_140123b60` damaged, the StateMachine's
/// pending else current state, `+0x50`, `+0x54/+0x58` the deconstruction, and whether `+0x60`
/// is the building's own template (a rebuild) or another than the description's (an upgrade,
/// `FUN_140123ad0`).
#[derive(Debug, Clone, Copy, Default)]
pub struct DestroyInfo {
    pub entity: usize,
    pub is_damaged: bool,
    pub state: i8,
    pub damaged: bool,
    pub count: i32,
    pub target: i32,
    pub rebuild: bool,
    pub upgrade: bool,
}

/// `DestroyUI` (vtable `0x140161c30`): bind `FUN_140066ab0`, refresh `FUN_140066460`, events
/// `FUN_1400669e0`.
pub struct DestroyUi {
    /// `+0x30` `progressRemove`, `+0x38` `progressDestruct` (each with its text template).
    remove: Option<(NodeId, Option<(NodeId, String)>)>,
    destruct: Option<(NodeId, Option<(NodeId, String)>)>,
    /// `+0x40..+0x58`: `textRemove`, `textUpgrade`, `textDamage`, `textFixing`.
    text_remove: Option<NodeId>,
    text_upgrade: Option<NodeId>,
    text_damage: Option<NodeId>,
    text_fixing: Option<NodeId>,
    /// `+0x60` `buttonReclaim` (`0x1771`), `+0x68` `buttonDestroy` (`0x1773`), `+0x70`
    /// `buttonRebuild` (`0x1772`).
    reclaim: Option<NodeId>,
    destroy: Option<NodeId>,
    rebuild: Option<NodeId>,
    filled: bool,
}

impl DestroyUi {
    pub fn bind(ui: &mut Ui, element: NodeId) -> DestroyUi {
        let progress = |ui: &Ui, n: &str| {
            ui.find(element, n).map(|p| {
                let content = match &ui.node(p).kind {
                    Kind::Control(Control::Progress { content, .. }) => *content,
                    _ => None,
                };
                (p, content.map(|c| (c, text_of(ui, c))))
            })
        };
        DestroyUi {
            remove: progress(ui, "progressRemove"),
            destruct: progress(ui, "progressDestruct"),
            text_remove: ui.find(element, "textRemove"),
            text_upgrade: ui.find(element, "textUpgrade"),
            text_damage: ui.find(element, "textDamage"),
            text_fixing: ui.find(element, "textFixing"),
            reclaim: ui.find(element, "buttonReclaim"),
            destroy: ui.find(element, "buttonDestroy"),
            rebuild: ui.find(element, "buttonRebuild"),
            filled: false,
        }
    }

    fn show(ui: &mut Ui, n: Option<NodeId>, visible: bool) {
        if let Some(n) = n {
            ui.set_visible(n, visible);
        }
    }

    fn fill_progress(ui: &mut Ui, p: &Option<(NodeId, Option<(NodeId, String)>)>, fill: f32) {
        if let Some((node, text)) = p {
            ui.set_value(*node, fill);
            if let Some((t, template)) = text {
                let percent = (fill * 100.0 + 0.5) as i32;
                ui.set_text(*t, &template.replace("@0", &percent.to_string()));
            }
        }
    }

    pub fn refresh(&mut self, ui: &mut Ui, data: &EntityData) {
        let Some(d) = data.destroy else { return };
        if !self.filled {
            // The bind's entity half: damaged, no reclaiming.
            self.filled = true;
            if d.is_damaged || d.damaged {
                Self::show(ui, self.reclaim, false);
            }
        }
        if !d.is_damaged {
            Self::show(ui, self.rebuild, false);
            Self::show(ui, self.destroy, false);
            Self::show(ui, self.text_damage, false);
            Self::show(ui, self.reclaim, true);
            if let Some(storage) = data.storage {
                let fill = if storage.capacity < 1 {
                    0.0
                } else {
                    storage.used(data.materials) as f32 / storage.capacity as f32
                };
                Self::fill_progress(ui, &self.remove, fill);
            }
            let fill = if d.target == 0 {
                0.0
            } else {
                d.count as f32 / d.target as f32
            };
            Self::fill_progress(ui, &self.destruct, fill);
            let deconstructing = fill != 0.0;
            Self::show(ui, self.remove.as_ref().map(|p| p.0), !deconstructing);
            Self::show(ui, self.destruct.as_ref().map(|p| p.0), deconstructing);
            if d.rebuild {
                Self::show(ui, self.text_fixing, true);
                Self::show(ui, self.text_remove, false);
                Self::show(ui, self.text_upgrade, false);
            } else {
                Self::show(ui, self.text_remove, !d.upgrade);
                Self::show(ui, self.text_upgrade, d.upgrade);
                Self::show(ui, self.text_fixing, false);
            }
            return;
        }
        // Damaged (6) or rubble (7): rebuild or clear it, not while it burns (6).
        for b in [self.rebuild, self.destroy].into_iter().flatten() {
            ui.node_mut(b).enabled = d.state != 6;
        }
        Self::show(ui, self.rebuild, true);
        Self::show(ui, self.destroy, true);
        Self::show(ui, self.text_damage, true);
        for n in [
            self.text_fixing,
            self.text_remove,
            self.text_upgrade,
            self.reclaim,
        ] {
            Self::show(ui, n, false);
        }
        Self::show(ui, self.remove.as_ref().map(|p| p.0), false);
        Self::show(ui, self.destruct.as_ref().map(|p| p.0), false);
    }

    /// `FUN_1400669e0`: `0x1771` `FUN_1401238d0` (keep it), `0x1772` `FUN_140123940` (state
    /// message 0: rebuild), `0x1773` `FUN_1401239a0` (state message 1: clear it).
    pub fn click(&self, node: NodeId, data: &EntityData) -> Option<WorkPlaceClick> {
        let entity = data.destroy?.entity;
        use banished_game::professions::Command;
        if Some(node) == self.reclaim {
            return Some(WorkPlaceClick::Command(Command::CancelDemolition {
                entity,
            }));
        }
        let message = if Some(node) == self.rebuild {
            0
        } else if Some(node) == self.destroy {
            1
        } else {
            return None;
        };
        Some(WorkPlaceClick::Command(Command::StateMessage {
            entity,
            message,
        }))
    }
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

/// `WorkPlaceUI` (vtable `0x140164450`, constructor `FUN_14008d2c0`): bind `FUN_14008ce10`,
/// refresh `FUN_14008c550`, events `FUN_14008ccb0`. Without a profession (`WorkPlaceComponent
/// +0x48`, e.g. storage barns and houses) the whole element is hidden.
pub struct WorkPlaceUi {
    element: NodeId,
    /// `+0x30`, `+0x38` and their parameter templates.
    count: Option<(NodeId, String)>,
    max_count: Option<(NodeId, String)>,
    /// `+0x40..+0x58`: element ids 0x1450..0x1453.
    worker_up: Option<NodeId>,
    worker_down: Option<NodeId>,
    job_up: Option<NodeId>,
    job_down: Option<NodeId>,
    /// Element id 0x1454.
    button: Option<NodeId>,
    /// `+0x60`: the worker the workplace button last showed.
    cursor: usize,
}

/// What WorkPlaceUI reads from the selected building.
#[derive(Debug, Clone)]
pub struct WorkPlaceInfo {
    pub entity: usize,
    /// The workplace profession's sprite (`Profession +0x28`, `+0x30`).
    pub sprite: (Option<String>, String),
    /// WorkPlaceComponent `+0x60`, WorkComponent `+0x30`.
    pub workers: Vec<usize>,
    pub max: i32,
    /// Adults (not at school) holding the workplace's profession, and laborers.
    pub holders: i32,
    pub laborers: i32,
    /// WorkDescription `+0x20`, `+0x28`, `+0x2c`.
    pub adjustable: bool,
    pub min_jobs: i32,
    pub max_jobs: i32,
    /// Builder workplace with a BuildComponent: InteractComponent build points (`FUN_140128540(1)`).
    pub build_points: Option<i32>,
}

/// A WorkPlaceUI element event (`FUN_14008ccb0`, event 0x12).
pub enum WorkPlaceClick {
    Command(banished_game::professions::Command),
    /// 0x1454 (`FUN_14008cba0` → `FUN_140108510`): pin the window, show and select the worker.
    ShowWorker(usize),
}

impl WorkPlaceUi {
    pub fn bind(ui: &mut Ui, element: NodeId) -> WorkPlaceUi {
        let f = |n: &str| ui.find(element, n);
        let param = |n: &str| f(n).map(|id| (id, text_of(ui, id)));
        WorkPlaceUi {
            element,
            count: param("workPlaceCount"),
            max_count: param("maxCount"),
            worker_up: f("workerCountUp"),
            worker_down: f("workerCountDown"),
            job_up: f("jobCountUp"),
            job_down: f("jobCountDown"),
            button: f("workPlaceButton"),
            cursor: 0,
        }
    }

    pub fn refresh(&self, ui: &mut Ui, data: &EntityData) {
        let Some(w) = &data.workplace else {
            ui.set_visible(self.element, false);
            return;
        };
        ui.set_visible(self.element, true);
        let jobs = w.adjustable || w.build_points.is_some();
        for b in [self.job_up, self.job_down].into_iter().flatten() {
            ui.set_visible(b, jobs);
        }
        if let Some((id, template)) = &self.count {
            let text = template
                .replace("@0", &w.workers.len().to_string())
                .replace("@1", &w.max.to_string());
            ui.set_text(*id, &text);
            if let Some(s) = ui.res.sprite(w.sprite.0.as_deref(), &w.sprite.1) {
                if let Kind::Label { icon, .. } = &mut ui.node_mut(*id).kind {
                    *icon = Some(s);
                }
            }
        }
        if let Some((id, template)) = &self.max_count {
            ui.set_text(*id, &template.replace("@0", &w.holders.to_string()));
        }
        let enable = |ui: &mut Ui, b: Option<NodeId>, on: bool| {
            if let Some(b) = b {
                ui.node_mut(b).enabled = on;
            }
        };
        enable(ui, self.worker_up, w.laborers > 0);
        enable(ui, self.worker_down, w.holders > 0);
        let (cap, floor) = match w.build_points {
            Some(points) => (w.max_jobs.min(points), 1),
            None => (w.max_jobs, w.min_jobs),
        };
        enable(ui, self.job_up, w.max < cap);
        enable(ui, self.job_down, w.max > floor);
    }

    pub fn click(&mut self, id: NodeId, data: &EntityData) -> Option<WorkPlaceClick> {
        use banished_game::professions::Command;
        let w = data.workplace.as_ref()?;
        let entity = w.entity;
        let command = if Some(id) == self.worker_up {
            Command::AddWorker { entity }
        } else if Some(id) == self.worker_down {
            Command::RemoveWorker { entity }
        } else if Some(id) == self.job_up {
            Command::ChangeMaxWorkers { entity, delta: 1 }
        } else if Some(id) == self.job_down {
            Command::ChangeMaxWorkers { entity, delta: -1 }
        } else if Some(id) == self.button {
            if w.workers.is_empty() {
                return None;
            }
            self.cursor += 1;
            if w.workers.len() <= self.cursor {
                self.cursor = 0;
            }
            return Some(WorkPlaceClick::ShowWorker(w.workers[self.cursor]));
        } else {
            return None;
        };
        Some(WorkPlaceClick::Command(command))
    }
}

/// `BuildUI`: bind `FUN_1400b7410`, refresh `FUN_1400b6f20`, events `FUN_1400b7370`. The
/// `storageTable` row 0 counts the cleared objects; rows 1.. are the BuildDescription's
/// materials, then the build work, then blanks.
pub struct BuildUi {
    /// `+0x48`: row 0's `textTotal`.
    cleared: Option<(NodeId, String)>,
    /// `+0x30/+0x38`: rows 1.. (`labelStorage`, `textTotal` and its template).
    rows: Vec<(Option<NodeId>, Option<(NodeId, String)>)>,
    /// `+0x40`, its content text template.
    progress: Option<NodeId>,
    progress_text: Option<(NodeId, String)>,
    /// Element id 6000.
    pause: Option<NodeId>,
    /// The bind's entity-dependent half runs on the first refresh, which has the entity.
    filled: bool,
}

/// What BuildUI reads from the selected building.
#[derive(Debug, Clone)]
pub struct BuildInfo {
    pub entity: usize,
    /// BuildComponent present: BuildDescription `+0x20` `(material, count × stages, delivered)`
    /// where delivered is the site storage's count less its reserved count; `+0x30` work ×
    /// stages and progress (`+0x48`).
    pub build: Option<BuildProgress>,
    /// ClearAreaComponent `+0x48` and `FUN_14011d800`.
    pub clear_total: i32,
    pub clear_remaining: i32,
    /// WorkPlaceComponent `+0x50` when there is one.
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone)]
pub struct BuildProgress {
    pub materials: Vec<(usize, i32, i32)>,
    pub work: i32,
    pub progress: i32,
}

impl BuildUi {
    pub fn bind(ui: &mut Ui, element: NodeId) -> BuildUi {
        let param =
            |ui: &Ui, root: NodeId, n: &str| ui.find(root, n).map(|id| (id, text_of(ui, id)));
        let table = ui.find(element, "storageTable");
        let children = table.map(|t| ui.children(t)).unwrap_or_default();
        let cleared = children.first().and_then(|&c| param(ui, c, "textTotal"));
        let rows = children
            .iter()
            .skip(1)
            .map(|&c| (ui.find(c, "labelStorage"), param(ui, c, "textTotal")))
            .collect();
        let progress = ui.find(element, "progress");
        let progress_text = progress.and_then(|p| match &ui.node(p).kind {
            Kind::Control(Control::Progress { content, .. }) => {
                content.map(|c| (c, text_of(ui, c)))
            }
            _ => None,
        });
        BuildUi {
            cleared,
            rows,
            progress,
            progress_text,
            pause: ui.find(element, "buttonPause"),
            filled: false,
        }
    }

    /// The entity half of `FUN_1400b7410`.
    fn fill(&mut self, ui: &mut Ui, archive: &Archive, data: &EntityData, b: &BuildInfo) {
        if let Some(p) = self.pause {
            match b.enabled {
                None => ui.node_mut(p).enabled = false,
                Some(on) => ui.set_checked(p, !on),
            }
        }
        if let Some((id, t)) = &self.cleared {
            ui.set_text(
                *id,
                &t.replace("@0", "0")
                    .replace("@1", &b.clear_total.to_string()),
            );
        }
        let materials = b.build.as_ref().map_or(&[][..], |p| &p.materials[..]);
        for (j, (label, total)) in self.rows.iter().enumerate() {
            if let Some(&(m, required, _)) = materials.get(j) {
                if let Some((id, t)) = total {
                    ui.set_text(*id, &t.replace("@1", &required.to_string()));
                }
                let d = material_file(&data.materials[m].reference);
                let d = archive
                    .read_ref(&format!("{d}:rawmaterial"))
                    .ok()
                    .and_then(|b| raw_material_description(&b).ok());
                if let (Some(l), Some(d)) = (*label, d) {
                    if let Some(s) = ui.res.sprite(d.sprite_sheet.as_deref(), &d.name) {
                        if let Kind::Label { icon, .. } = &mut ui.node_mut(l).kind {
                            *icon = Some(s);
                        }
                    }
                    let name = ui.res.string(d.string_table.as_deref(), &d.name);
                    ui.set_text(l, &name);
                    let table = ui.node(l).string_table.clone();
                    ui.node_mut(l).tooltip = ui.res.string(table.as_deref(), "ResourceTip");
                }
            } else if let (Some(p), true) = (&b.build, j == materials.len()) {
                if let Some((id, t)) = total {
                    ui.set_text(*id, &t.replace("@1", &p.work.to_string()));
                }
            } else {
                if let Some((id, _)) = total {
                    ui.set_text(*id, "");
                }
                if let Some(l) = *label {
                    ui.set_text(l, "");
                    if let Kind::Label { icon, .. } = &mut ui.node_mut(l).kind {
                        *icon = None;
                    }
                }
            }
        }
        // The totals keep their `@0` for the refresh: store the half-filled templates.
        for (_, total) in &mut self.rows {
            if let Some((id, t)) = total {
                *t = text_of(ui, *id);
            }
        }
        if let Some((_, t)) = &mut self.cleared {
            *t = t.replace("@1", &b.clear_total.to_string());
        }
    }

    pub fn refresh(&mut self, ui: &mut Ui, archive: &Archive, data: &EntityData) {
        let Some(b) = &data.build else { return };
        if !self.filled {
            self.filled = true;
            self.fill(ui, archive, data, b);
        }
        if let Some(p) = &b.build {
            if p.work <= p.progress {
                return;
            }
        }
        let removed = b.clear_total - b.clear_remaining;
        if let Some((id, t)) = &self.cleared {
            ui.set_text(*id, &t.replace("@0", &removed.to_string()));
        }
        let (total, done) = match &b.build {
            None => (b.clear_total, removed),
            Some(p) => {
                let n = self.rows.len().min(p.materials.len());
                let (mut delivered, mut required) = (0, 0);
                for (j, &(_, need, have)) in p.materials.iter().take(n).enumerate() {
                    if let Some((id, t)) = &self.rows[j].1 {
                        ui.set_text(*id, &t.replace("@0", &have.to_string()));
                    }
                    delivered += have;
                    required += need;
                }
                if let Some((_, Some((id, t)))) = self.rows.get(n) {
                    ui.set_text(*id, &t.replace("@0", &p.progress.to_string()));
                }
                (
                    p.work + b.clear_total + required,
                    p.progress + delivered + removed,
                )
            }
        };
        let fill = if total as f32 != 0.0 {
            done as f32 / total as f32
        } else {
            0.0
        };
        if let Some(p) = self.progress {
            ui.set_value(p, fill);
        }
        if let Some((id, t)) = &self.progress_text {
            let percent = (fill * 100.0 + 0.5) as i32;
            ui.set_text(*id, &t.replace("@0", &percent.to_string()));
        }
    }

    /// Element 6000: `SetEnabled(+0x50 == 0)`.
    pub fn click(&self, id: NodeId, data: &EntityData) -> Option<WorkPlaceClick> {
        use banished_game::professions::Command;
        let b = data.build.as_ref()?;
        let enabled = b.enabled?;
        (Some(id) == self.pause).then_some(WorkPlaceClick::Command(Command::SetEnabled {
            entity: b.entity,
            enabled: !enabled,
        }))
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
    Build(BuildUi),
    StatusIcon(StatusIconUi),
    Destroy(DestroyUi),
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
            WORKPLACE_UI => Some(Controller::WorkPlace(WorkPlaceUi::bind(ui, element))),
            BUILD_UI => Some(Controller::Build(BuildUi::bind(ui, element))),
            DESTROY_UI => Some(Controller::Destroy(DestroyUi::bind(ui, element))),
            // Not traced: the config's `+0x20` flag (no shipped StatusIconUI has a config).
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
            Controller::Build(c) => c.refresh(ui, archive, data),
            Controller::StatusIcon(c) => c.refresh(ui, data),
            Controller::Destroy(c) => c.refresh(ui, data),
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
