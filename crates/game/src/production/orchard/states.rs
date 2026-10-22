//! An orchard's states: planting, tending, harvesting, clearing and their work lists.

use super::*;

impl Sim<'_, '_> {
    /// State 9 enter (`FUN_1400519d0`): a tree slot every `spacing` cells (rows, then
    /// columns), the footprint lifted, then PlantOrchard.
    pub(crate) fn orchard_enter9(&mut self, jobs: &mut Jobs, e: usize) {
        let on = self.orchard(e).is_some_and(|o| o.enabled);
        self.enable_workplace(e, on);
        let d = self.orchard_desc(e);
        let (sx, sy) = (d.spacing.0.max(1), d.spacing.1.max(1));
        let n = self.size as i32;
        let (x0, y0) = self.placed[e].cell;
        let (w, h) = self.placed[e].footprint;
        let cols = (w - 1 + sx) / sx;
        let rows = (h - 1 + sy) / sy;
        let mut trees = Vec::new();
        for r in 0..rows {
            for c in 0..cols {
                trees.push(TreeSlot {
                    cell: (y0 + r * sy) * n + x0 + c * sx,
                    ..Default::default()
                });
            }
        }
        if let Some(o) = self.orchard_mut(e) {
            o.trees = trees;
        }
        self.unstamp(e);
        self.orchard_plant_work(jobs, e);
    }

    /// `FUN_1400fa610`.
    pub(super) fn orchard_plant_work(&mut self, jobs: &mut Jobs, e: usize) {
        self.orchard_refresh(e);
        let count = self.orchard(e).map_or(0, |o| o.trees.len());
        let tree = self.orchard(e).and_then(|o| o.tree);
        let profession = self
            .workforce
            .workplaces
            .get(&e)
            .and_then(|w| w.profession)
            .unwrap_or(self.professions.laborer);
        let work = Work::new(
            e,
            self.rect_cell(e),
            profession,
            WorkKind::PlantOrchard(AreaWork {
                claims: BTreeMap::new(),
                need: vec![false; count],
                crop: tree,
            }),
        );
        self.set_work(jobs, e, work, 1, false);
    }

    /// State 9 update (`FUN_1400518f0`): every tree standing → tend (10).
    pub(crate) fn orchard_update9(&mut self, jobs: &mut Jobs, e: usize) {
        let total = self.orchard(e).map_or(0, |o| o.trees.len());
        if self.orchard_count(e, 1) == total {
            self.remove_work(jobs, e);
            self.machine(e).request(10);
        }
    }

    /// State 10 enter (`FUN_140051790`): lost trees → replant (12); else the tending work.
    pub(crate) fn orchard_enter10(&mut self, jobs: &mut Jobs, e: usize) {
        if self.orchard_count(e, 2) > 0 {
            self.machine(e).request(0xc);
            return;
        }
        self.orchard_tend_work(jobs, e);
    }

    pub(super) fn orchard_tend_work(&mut self, jobs: &mut Jobs, e: usize) {
        let profession = self
            .workforce
            .workplaces
            .get(&e)
            .and_then(|w| w.profession)
            .unwrap_or(self.professions.laborer);
        let work = Work::new(
            e,
            self.rect_cell(e),
            profession,
            WorkKind::TendArea(crate::production::herbalist::HerbalistWork::default()),
        );
        self.set_work(jobs, e, work, 1, false);
    }

    /// Exits of 10 and 12 (`FUN_140051500`, `FUN_140051310`): the work goes.
    pub(crate) fn orchard_exit_work(&mut self, jobs: &mut Jobs, e: usize) {
        let id = self.sites[&e].work_id;
        if id >= 0 && jobs.works.contains_key(&id) {
            self.remove_work(jobs, e);
        }
    }

    /// State 10 update (`FUN_140051560`): a harvest, or replanting lost trees; tending runs
    /// in months 1–8 only.
    pub(crate) fn orchard_update10(&mut self, jobs: &mut Jobs, e: usize) {
        self.orchard_refresh(e);
        if self.orchard_harvest_due(e) {
            return;
        }
        if self.orchard_count(e, 2) > 0 {
            self.machine(e).request(0xc);
        }
        let month = (self.calendar / 3600.0 * 12.0) as i32;
        let season = month >= 0 && ((month - 1) as u32) < 0xb && 2 < (month - 9) as u32;
        let id = self.sites[&e].work_id;
        let registered = id >= 0 && jobs.works.contains_key(&id);
        if registered && !season {
            self.remove_work(jobs, e);
        } else if !registered && season {
            self.orchard_tend_work(jobs, e);
        }
    }

    /// State 12 enter (`FUN_140051440`): PlantOrchard again.
    pub(crate) fn orchard_enter12(&mut self, jobs: &mut Jobs, e: usize) {
        self.orchard_plant_work(jobs, e);
    }

    /// State 12 update (`FUN_140051370`): a harvest, or back to tending once no tree is lost.
    pub(crate) fn orchard_update12(&mut self, e: usize) {
        self.orchard_refresh(e);
        if self.orchard_harvest_due(e) {
            return;
        }
        if self.orchard_count(e, 2) == 0 {
            self.machine(e).request(10);
        }
    }

    /// State 11 enter (`FUN_140051160`): the picking work; the orchard's cells remember its
    /// tree (`FUN_1400a4c40`, cell `+0x10`).
    pub(crate) fn orchard_enter11(&mut self, jobs: &mut Jobs, e: usize) {
        let need: Vec<bool> = self
            .orchard(e)
            .map(|o| o.trees.iter().map(|t| t.entity.is_some()).collect())
            .unwrap_or_default();
        let profession = self
            .workforce
            .workplaces
            .get(&e)
            .and_then(|w| w.profession)
            .unwrap_or(self.professions.laborer);
        let work = Work::new(
            e,
            self.rect_cell(e),
            profession,
            WorkKind::PickFruit(AreaWork {
                claims: BTreeMap::new(),
                need,
                crop: None,
            }),
        );
        self.set_work(jobs, e, work, 1, false);
        let tree = self.orchard(e).and_then(|o| o.tree);
        let n = self.size as i32;
        let (x0, y0) = self.placed[e].cell;
        let (w, h) = self.placed[e].footprint;
        for y in y0..y0 + h {
            for x in x0..x0 + w {
                self.cells.previous_template[(y * n + x) as usize] = tree;
            }
        }
    }

    /// State 11 update (`FUN_140050fb0`): every standing tree picked → tend (10).
    pub(crate) fn orchard_update11(&mut self, jobs: &mut Jobs, e: usize) {
        let Some(o) = self.orchard(e) else { return };
        let standing = o.trees.iter().filter(|t| t.state == 1).count();
        let picked = o.trees.iter().filter(|t| t.picked).count();
        if standing <= picked {
            self.remove_work(jobs, e);
            self.machine(e).request(10);
        }
    }

    /// State 0x14 enter (`FUN_140050e40`): the trees are cleared (a ClearArea work with the
    /// ClearArea description's filter, `FUN_1400ebe30(.., 1, 0, 1)`, categories `0x401`).
    pub(crate) fn orchard_enter14(&mut self, jobs: &mut Jobs, e: usize) {
        let id = self.sites[&e].work_id;
        if id >= 0 && jobs.works.contains_key(&id) {
            self.remove_work(jobs, e);
        }
        let filter = self.site_templates[self.sites[&e].template]
            .clear
            .as_ref()
            .and_then(|c| c.reference.clone());
        let work = Work::new(
            e,
            self.rect_cell(e),
            self.professions.laborer,
            WorkKind::Clear(crate::work::construction::ClearWork {
                filter,
                clear_resources: true,
                clear_cells: false,
                clear_piles: true,
                ..Default::default()
            }),
        );
        self.set_work(jobs, e, work, 0x401, false);
    }

    /// State 0x14 exit (`FUN_140050d40`).
    pub(crate) fn orchard_exit14(&mut self, jobs: &mut Jobs, e: usize) {
        self.remove_work(jobs, e);
    }

    /// State 0x14 update (`FUN_140050d90`): cleared → plant again (9).
    pub(crate) fn orchard_update14(&mut self, e: usize) {
        if self.is_clear(e) {
            self.machine(e).request(9);
        }
    }
}
