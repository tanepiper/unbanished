//! Demolition, damage and rubble: DestroyComponent. See docs/JOBS.md "Demolition".
//!
//! - 3: the work and status icons go; a damaged building goes on to 6, else to 4 with the
//!   BuildDestroy icon;
//! - 4: the building becomes a site again; a finished one with goods has them carried out first
//!   (ClearStorage), then 5;
//! - 5: builders take a started building down (a BuildStructure work with the destroy flag, half
//!   its construction work); then it is gone, leaving part of its materials in piles, or what
//!   replaces it;
//! - 6: damaged (burning): the goods are lost; it collapses when its time runs out;
//! - 7: rubble, to rebuild (message 0) or clear (message 1).
//!
//! Message 0 in 4 and 5 cancels the demolition.
//!
//! Original: `FUN_140034520`, `FUN_1401231d0`.
// vtable 0x140169500

use crate::town::{Sim, CONSTRUCTION_MASK};
use crate::work::construction::{run, BuildWork, Callbacks, Controller};
use crate::work::jobs::{Jobs, Work, WorkKind};
use crate::work::professions::icon;

/// A resolved [`banished_formats::game::DestroyDescription`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DestroyTemplate {
    // +0x20
    /// What is left in the building's place (a placed-object template).
    pub replace: Option<usize>,
    // +0x28
    /// The ModelComponent's material set once damaged.
    ///
    /// Original: `FUN_14012a4f0`.
    pub material: i32,
    // +0x2c
    /// The decal's material while damaged.
    pub decal: i32,
    // +0x30
    /// The lowest construction mesh a collapse steps down to.
    pub min_mesh: i32,
}

// +0x48; +0x50; +0x51; +0x54; +0x58; +0x60; +0x20
// 0x68 bytes; set up by FUN_140122fd0 from the description.
/// The demolition state of a building: the damage time left, whether it is damaged, whether its
/// materials come back, the demolition work done and needed, and what to put in its place.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Destroy {
    pub time: i64,
    pub damaged: bool,
    pub refund: bool,
    pub count: i32,
    pub target: i32,
    pub rebuild: Option<usize>,
    // +0x28
    /// The model shows the description's damaged material set (set by the damage with time left and
    /// by every collapse).
    pub damaged_look: bool,
}

impl Destroy {
    /// The demolition state of a new building.
    pub fn new(t: &DestroyTemplate) -> Destroy {
        Destroy {
            rebuild: t.replace,
            ..Default::default()
        }
    }
}

/// An entity put in a torn-down building's place: its template over the same map rectangle, turned
/// as it was, with its model's position and variant.
///
/// Original: `FUN_1401215f0`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rebuild {
    pub template: usize,
    pub cell: (i32, i32),
    pub footprint: (i32, i32),
    pub yaw: f32,
    pub position: [f32; 3],
    pub variant: usize,
}

/// `DAT_140178904`, `DAT_1401787fc`.
const QUARTER: f32 = 90.0;
const FLAT: f32 = 0.2;

impl Sim<'_, '_> {
    fn destroy_state(&mut self, e: usize) -> Option<&mut Destroy> {
        self.sites.get_mut(&e)?.destroy.as_mut()
    }

    /// The state machine's pending state, else its state (`+0x52`, `+0x50`).
    fn shown_state(&self, e: usize) -> i8 {
        self.sites.get(&e).map_or(-1, |s| {
            if s.machine.pending < 0 {
                s.machine.state
            } else {
                s.machine.pending
            }
        })
    }

    /// Damaged (6) or rubble (7).
    ///
    /// Original: `FUN_140123b60`.
    pub fn is_damaged(&self, e: usize) -> bool {
        matches!(self.shown_state(e), 6 | 7)
    }

    /// Being torn down (3 to 5).
    ///
    /// Original: `FUN_140123be0`.
    pub fn is_demolishing(&self, e: usize) -> bool {
        matches!(self.shown_state(e), 3..=5)
    }

    /// BuildComponent `+0x48 < BuildDescription +0x30 × +0x4c`: not finished.
    fn unfinished(&self, e: usize) -> bool {
        let site = &self.sites[&e];
        let t = &self.site_templates[site.template];
        site.progress < t.work * site.stages
    }

    /// A BuildComponent (removal markers have none).
    pub(crate) fn has_build(&self, e: usize) -> bool {
        let site = &self.sites[&e];
        self.site_templates[site.template]
            .controllers
            .contains(&Controller::Build)
    }

    /// Remove Buildings and the demolish button. Damaged or rubble: clear it (message 1); else the
    /// machine runs what is pending, then goes to 3.
    ///
    /// Original: `FUN_140123470`.
    pub fn demolish(&mut self, jobs: &mut Jobs, e: usize) {
        if self.destroy_state(e).is_none() {
            return;
        }
        if self.is_damaged(e) {
            self.destroy_message(jobs, e, 1);
            return;
        }
        run(self, jobs, e, false, false);
        self.machine(e).request(3);
        run(self, jobs, e, false, false);
    }

    /// UpgradeUI's button: the replacement is `template`, and the building is torn down.
    ///
    /// Original: `FUN_14008c160`, `FUN_140123470`.
    pub fn upgrade(&mut self, jobs: &mut Jobs, e: usize, template: usize) {
        let Some(d) = self.destroy_state(e) else {
            return;
        };
        d.rebuild = Some(template);
        self.demolish(jobs, e);
    }

    // slot +0x20
    /// DestroyUI's cancel (`0x1771`): message 0, and the replacement goes back to the
    /// description's.
    ///
    /// Original: `FUN_1401238d0`.
    pub fn cancel_demolition(&mut self, jobs: &mut Jobs, e: usize) {
        self.destroy_message(jobs, e, 0);
        let site = &self.sites[&e];
        let replace = self.site_templates[site.template]
            .destroy
            .and_then(|d| d.replace);
        if let Some(d) = self.destroy_state(e) {
            d.rebuild = replace;
        }
    }

    /// Damage for `time` (fire, the tornado). A building not started yet takes none; else it goes
    /// to 3 (then 6).
    ///
    /// Original: `FUN_140123c80`.
    pub fn damage(&mut self, jobs: &mut Jobs, e: usize, time: i64) -> bool {
        if self.destroy_state(e).is_none() {
            return false;
        }
        if self.has_build(e) && self.sites[&e].progress == 0 {
            return false;
        }
        let d = self.destroy_state(e).unwrap();
        d.damaged = true;
        d.time = time;
        if self.machine(e).index < 0 {
            run(self, jobs, e, false, false);
        }
        self.machine(e).request(3);
        run(self, jobs, e, false, false);
        true
    }

    /// Straight to rubble.
    ///
    /// Original: `FUN_140123ee0`.
    pub fn to_rubble(&mut self, jobs: &mut Jobs, e: usize) {
        if self.machine(e).index < 0 {
            run(self, jobs, e, false, false);
        }
        self.machine(e).request(7);
        run(self, jobs, e, false, false);
    }

    // +0x10
    /// The StateMachine's message slot for the Destroy states: the current state's handler.
    pub fn destroy_message(&mut self, jobs: &mut Jobs, e: usize, msg: i32) {
        if self.destroy_state(e).is_none() || self.machine(e).index < 0 {
            return;
        }
        match (self.machine(e).state, msg) {
            (4 | 5, 0) => self.cancel(jobs, e),
            (6, 2) => self.machine(e).request(7),
            // `FUN_140122770`: rebuild what stood here.
            (7, 0) => {
                self.become_site(jobs, e);
                let template = self.placed[e].template;
                let d = self.destroy_state(e).unwrap();
                d.refund = true;
                d.rebuild = Some(template);
                self.machine(e).request(5);
            }
            // `FUN_140122670`: clear the rubble.
            (7, 1) => {
                self.become_site(jobs, e);
                let build = self.has_build(e);
                if build {
                    self.destroy_state(e).unwrap().refund = true;
                }
                self.machine(e).request(5);
            }
            _ => {}
        }
    }

    /// `FUN_140121190`: the demolition is called off. A building goes back to its finished
    /// state (2), a site to building (8) or clearing (0, no mesh yet); a damaged one to rubble.
    fn cancel(&mut self, jobs: &mut Jobs, e: usize) {
        self.set_icon(e, icon::DESTROY, false);
        self.remove_work(jobs, e);
        if self.destroy_state(e).is_some_and(|d| d.damaged) {
            self.become_building(jobs, e);
            self.machine(e).request(7);
            return;
        }
        if !self.has_build(e) {
            self.machine(e).request(2);
            run(self, jobs, e, false, false);
        } else {
            // The MultiObject's first part's model (`+0x166` mesh, `+0x164` variant).
            let m = self.sites[&e].parts.first().copied().unwrap_or(e);
            let mesh = self.placed[m].mesh;
            let state = if mesh == usize::MAX {
                0
            } else if mesh as i32 == self.meshes_of(m) - 1 {
                2
            } else {
                8
            };
            self.machine(e).request(state);
        }
        self.become_building(jobs, e);
    }

    fn meshes_of(&self, m: usize) -> i32 {
        let p = &self.placed[m];
        self.templates[p.template]
            .model
            .as_ref()
            .and_then(|model| model.variants.get(p.variant))
            .map_or(1, |v| v.meshes.len() as i32)
    }

    /// Residence state 3 enter (`FUN_14012edf0` → `FUN_14012f3c0`): the icons 6 and 7 go,
    /// each family moves out (`FUN_14011b8a0(first, none, -1)`) and the residence leaves the
    /// town's (`+0x20`). The ResidenceComponent is kept on the site meanwhile.
    pub(crate) fn residence_leave(&mut self, e: usize) {
        let Some(mut home) = self.homes.remove(&e) else {
            return;
        };
        crate::people::status::set_icon(&mut home.status, icon::NO_FOOD, false, false);
        crate::people::status::set_icon(&mut home.status, icon::NO_FIREWOOD, false, false);
        for slot in 0..home.families.len() {
            while let Some(&c) = home.families[slot].first() {
                self.move_out(&mut home, c);
            }
        }
        self.workforce.residences.retain(|&r| r != e);
        if let Some(site) = self.sites.get_mut(&e) {
            site.home = Some(home);
        }
    }

    /// `FUN_14011b8a0` with no home: the citizen and its dependents leave theirs.
    fn move_out(&mut self, home: &mut crate::people::life::Home, c: usize) {
        for f in home.families.iter_mut() {
            f.retain(|&x| x != c);
        }
        self.states[c].home = None;
        self.states[c].home_slot = -1;
        for d in self.states[c].dependents.clone() {
            if self.states[d].home.is_some() {
                self.move_out(home, d);
            }
        }
    }

    /// State 3 enter (`FUN_140122c80`).
    pub(crate) fn destroy_enter3(&mut self, jobs: &mut Jobs, e: usize) {
        if let Some(d) = self.destroy_state(e) {
            d.count = 0;
        }
        let id = self.sites[&e].work_id;
        if jobs.works.contains_key(&id) {
            self.remove_work(jobs, e);
        }
        // `FUN_140131770`: every status icon goes.
        if let Some(w) = self.workforce.workplaces.get_mut(&e) {
            for bit in 0..32 {
                w.set_icon(bit, false);
            }
        }
        if self.destroy_state(e).is_some_and(|d| d.damaged) {
            self.machine(e).request(6);
        } else {
            self.machine(e).request(4);
            self.set_icon(e, icon::DESTROY, true);
        }
    }

    /// State 4 enter (`FUN_1401224a0`): a site again; a finished building's goods go first.
    pub(crate) fn destroy_enter4(&mut self, jobs: &mut Jobs, e: usize) {
        self.become_site(jobs, e);
        let build = self.has_build(e);
        if build {
            self.destroy_state(e).unwrap().refund = true;
        }
        if self.storage_empty(e) || (build && self.unfinished(e)) {
            self.machine(e).request(5);
            return;
        }
        let work = Work::new(
            e,
            self.rect_cell(e),
            self.professions.laborer,
            WorkKind::ClearStorage(crate::production::herbalist::HerbalistWork::default()),
        );
        self.set_work(jobs, e, work, 1, false);
    }

    /// `FUN_140122f50`.
    fn storage_empty(&self, e: usize) -> bool {
        let s = self.sites[&e].storage;
        self.storages[s].used(self.materials) == 0
    }

    /// State 4 update (`FUN_140122390`): empty and nobody carrying from it (`FUN_140133ef0`,
    /// the work's haulers): on to 5.
    pub(crate) fn destroy_update4(&mut self, jobs: &mut Jobs, e: usize) {
        if !self.storage_empty(e) {
            return;
        }
        let id = self.sites[&e].work_id;
        let busy = jobs.works.get(&id).is_some_and(|w| match &w.kind {
            WorkKind::ClearStorage(c) => !c.workers.is_empty(),
            _ => false,
        });
        if !busy {
            self.machine(e).request(5);
            self.remove_work(jobs, e);
        }
    }

    /// State 5 enter (`FUN_140122140`): a started building is taken down by builders, half
    /// its construction work (BuildStructure with the destroy flag, `FUN_1400eae90(.., 1)`);
    /// one not started is gone at once.
    pub(crate) fn destroy_enter5(&mut self, jobs: &mut Jobs, e: usize) {
        if self.has_build(e) {
            let site = &self.sites[&e];
            if site.progress != 0 {
                let work = self.site_templates[site.template].work;
                let target = ((work * site.stages) / 2).max(1);
                self.destroy_state(e).unwrap().target = target;
                let w = Work::new(
                    e,
                    self.rect_cell(e),
                    self.professions.builder,
                    WorkKind::Build(BuildWork {
                        destroy: true,
                        occupied: vec![false; self.build_point_count(e)],
                        ..Default::default()
                    }),
                );
                self.set_work(jobs, e, w, CONSTRUCTION_MASK, false);
                return;
            }
            // The materials come back before the entity goes (its zone still holds the
            // footprint).
            if self.destroy_state(e).is_some_and(|d| d.refund) {
                let (x, y) = self.placed[e].cell;
                let (w, h) = self.placed[e].footprint;
                self.refund(e, x, y, w, h);
            }
        }
        self.deactivate(jobs, e);
    }

    /// State 5 update (`FUN_1401215f0`): once the work is done the building goes; what
    /// replaces it is placed, or its materials partly come back.
    pub(crate) fn destroy_update5(&mut self, jobs: &mut Jobs, e: usize) {
        let Some(d) = self.destroy_state(e).cloned() else {
            return;
        };
        if d.count < d.target {
            return;
        }
        self.remove_work(jobs, e);
        let p = &self.placed[e];
        let (cell, footprint, yaw) = (p.cell, p.footprint, p.yaw);
        let (position, variant) = (p.position, p.variant);
        self.become_building(jobs, e);
        let finished = !self.unfinished(e);
        self.deactivate(jobs, e);
        if let Some(template) = d.rebuild {
            if finished || d.damaged {
                let own = self.placed[e].template;
                let replace = self.site_templates[self.sites[&e].template]
                    .destroy
                    .and_then(|t| t.replace);
                // `FUN_140123ad0`, `FUN_140123a80`: a rebuild or another replacement than the
                // description's gives the materials back too.
                if d.refund && (template == own || Some(template) != replace) {
                    self.refund(e, cell.0, cell.1, footprint.0, footprint.1);
                }
                let mut yaw = yaw as i32;
                while yaw < 0 {
                    yaw += 360;
                }
                self.rebuilds.push(Rebuild {
                    template,
                    cell,
                    footprint,
                    yaw: yaw as f32,
                    position,
                    variant,
                });
                return;
            }
        }
        if d.refund {
            self.refund(e, cell.0, cell.1, footprint.0, footprint.1);
        }
    }

    /// State 6 enter (`FUN_1401229a0`): the goods are lost; with no time left it collapses.
    pub(crate) fn destroy_enter6(&mut self, e: usize) {
        let s = self.sites[&e].storage;
        // `FUN_14008f300`: out of the town's storages.
        self.storage_changes.push((e, false));
        self.storages[s].items.clear();
        if self.destroy_state(e).is_some_and(|d| d.time < 1) {
            self.collapse(e);
        } else {
            // `FUN_14012a4f0(model, +0x28)`.
            self.damaged_look(e);
        }
    }

    /// State 6 update (`FUN_140122960`).
    pub(crate) fn destroy_update6(&mut self, e: usize, dt: i64) {
        let Some(d) = self.destroy_state(e) else {
            return;
        };
        if d.time != 0 {
            d.time -= dt;
            if d.time < 1 {
                d.time = 0;
                self.collapse(e);
            }
        }
    }

    /// State 6 exit (`FUN_1401228d0`): with time left it collapses, and one time in two it
    /// collapses (again).
    pub(crate) fn destroy_exit6(&mut self, e: usize) {
        if self.templates[self.placed[e].template].model.is_none() {
            return;
        }
        if self.destroy_state(e).is_some_and(|d| 0 < d.time) {
            self.collapse(e);
        }
        if self.rng.rand_below(2) == 0 {
            self.collapse(e);
        }
    }

    /// The model's damaged material set (`FUN_14012a4f0` with the description's `+0x28`).
    fn damaged_look(&mut self, e: usize) {
        if let Some(d) = self.destroy_state(e) {
            if !d.damaged_look {
                d.damaged_look = true;
                *self.models_dirty = true;
            }
        }
    }

    /// `FUN_140122aa0`: one construction mesh down, not below the description's `+0x30`; the
    /// model takes the damaged material set and the decal the damaged material. Its entity
    /// event 0 reaches only the HighlightComponent (`FUN_14004ce00`, the outline of the new
    /// mesh) and `FUN_14012e420` re-registers the picking mesh; the port's highlight and
    /// picking read the entity's current mesh, so both follow.
    fn collapse(&mut self, e: usize) {
        let Some(t) = self
            .sites
            .get(&e)
            .and_then(|s| self.site_templates[s.template].destroy)
        else {
            return;
        };
        let mesh = self.placed[e].mesh;
        let current = if mesh == usize::MAX { -1 } else { mesh as i32 };
        let next = (current - 1).max(0).max(t.min_mesh);
        self.damaged_look(e);
        let d = self.decals.entry(e).or_default();
        if d.material != t.decal {
            d.material = t.decal;
            *self.models_dirty = true;
        }
        if next != current {
            self.placed[e].mesh = next as usize;
            *self.models_dirty = true;
        }
    }

    /// `FUN_1400c8370` on an active entity: each component's deactivate slot runs now (the
    /// zone and the map stamp are lifted); the entity is removed after the machine has run.
    pub(crate) fn deactivate(&mut self, jobs: &mut Jobs, e: usize) {
        self.remove_work(jobs, e);
        let n = self.size as i32;
        let stamped = {
            let p = &self.placed[e];
            let (x, y) = p.cell;
            let (w, h) = p.footprint;
            (y..y + h).any(|cy| {
                (x..x + w).any(|cx| self.cells.objects[(cy * n + cx) as usize] == Some(e))
            })
        };
        if stamped {
            self.unstamp(e);
        }
        let p = &self.placed[e];
        let (x, y) = p.cell;
        let (w, h) = p.footprint;
        for cy in y..y + h {
            for cx in x..x + w {
                let i = (cy * n + cx) as usize;
                if self.cells.occupants[i] == Some(e) {
                    self.cells.occupants[i] = None;
                }
            }
        }
        if let Some(site) = self.sites.get_mut(&e) {
            site.destroyed = true;
        }
    }

    /// `FUN_140121a00`: piles of the materials over the rectangle (`x`, `y`, `w`, `h`), by rows,
    /// on free flat ground: an unfinished building's stored materials (half once work
    /// began), else half its build materials (a quarter when damaged); at most `100 / size`
    /// a pile, each turned a random quarter.
    fn refund(&mut self, e: usize, x: i32, y: i32, w: i32, h: i32) {
        let site = &self.sites[&e];
        let t = &self.site_templates[site.template];
        let mut piles: Vec<(usize, i32)> = Vec::new();
        if site.progress < t.work * site.stages {
            let div = if 0 < site.progress { 2 } else { 1 };
            for item in &self.storages[site.storage].items {
                let Some(k) = t.materials.iter().position(|&(_, m)| m == item.material) else {
                    continue;
                };
                if let Some(&Some(pile)) = t.material_templates.get(k) {
                    piles.push((pile, (item.count - item.reserved) / div));
                }
            }
        } else {
            let div = if self.sites[&e].destroy.as_ref().is_some_and(|d| d.damaged) {
                4
            } else {
                2
            };
            for (k, &(count, _)) in t.materials.iter().enumerate() {
                if let Some(&Some(pile)) = t.material_templates.get(k) {
                    piles.push((pile, count / div));
                }
            }
        }
        let n = self.size as i32;
        let (mut col, mut row) = (0, 0);
        for (pile, mut left) in piles {
            if left <= 0 {
                continue;
            }
            let Some(material) = self.templates[pile].pile_material else {
                continue;
            };
            // The cursor carries on from the last material's, and the scan checks the last
            // row after the cell (a material after the rectangle is full scans on below it).
            loop {
                let cell = (y + row) * n + x + col;
                if cell < 0 || n * n <= cell {
                    break;
                }
                let i = cell as usize;
                let ground = self.ground_at(cell);
                if self.cells.occupants[i].is_none()
                    && self.cells.types[i].0 == 2
                    && 0.0 <= ground[2]
                    && ground[2] < FLAT
                {
                    let amount = left.min(100 / self.materials[material].size.max(1));
                    let yaw = self.rng.rand_tol(2) as f32 * QUARTER;
                    self.spawn_pile(pile, (x + col, y + row), amount, yaw, None);
                    left -= amount;
                }
                col += 1;
                if w <= col {
                    col = 0;
                    row += 1;
                }
                if row == h || left <= 0 {
                    break;
                }
            }
        }
    }
}
