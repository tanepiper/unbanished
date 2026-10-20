//! Workplaces: the workers and the worker count buttons.

use super::*;

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
    Command(unbanished_game::work::professions::Command),
    /// 0x1454 (`FUN_14008cba0` → `FUN_140108510`): pin the window, show and select the worker.
    ShowWorker(usize),
    /// ResourceLimitUI: the stock manager's limit for a slot.
    StockLimit {
        slot: usize,
        limit: i32,
    },
    /// UpgradeUI: tear the building down for `template`.
    Upgrade {
        entity: usize,
        template: String,
    },
    /// TownHallUI's goto: show and select a workplace.
    ShowBuilding(usize),
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
        use unbanished_game::work::professions::Command;
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
