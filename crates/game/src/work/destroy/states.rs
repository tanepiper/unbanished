//! A building's states while it is emptied, torn down, burnt or collapsing.

use super::*;

impl Sim<'_, '_> {
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
    pub(super) fn storage_empty(&self, e: usize) -> bool {
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
    pub(super) fn damaged_look(&mut self, e: usize) {
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
    pub(super) fn collapse(&mut self, e: usize) {
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
}
