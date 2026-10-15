//! Orchards: OrchardComponent (vtable `0x1401602a0`, states `FUN_140052a40`), FruitComponent
//! (`0x14015f5a0`) on the trees, and the works PlantOrchard (`0x140167d30`), TendArea (as an
//! orchard's) and fruit picking (`0x140167940`). See docs/FARMING.md "Orchards".

use std::collections::BTreeMap;

use crate::character::{Action, Component, Message, MoveTo};
use crate::construction::Callbacks;
use crate::farm::AreaWork;
use crate::jobs::{Jobs, Work, WorkKind};
use crate::pathfind::{nearest_neighbour, WALKABLE};
use crate::scheduler::{seconds, ticks};
use crate::town::{cell_ground, Sim};

/// OrchardDescription (`FUN_140052c60`, versioned): tree spacing `+0x24` / `+0x28`, a tending
/// visit's ripening `+0x2c` (share of the ripening time), the stock slot `+0x20` (version 1).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct OrchardTemplate {
    pub spacing: (i32, i32),
    pub tend: f32,
    pub group: i32,
}

/// One tree slot (`+0x50`, 0x28 bytes).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TreeSlot {
    /// `+0x00`: the map cell.
    pub cell: i32,
    /// `+0x04`: 0 to plant, 1 standing, 2 lost.
    pub state: i32,
    /// `+0x08`: the tree.
    pub entity: Option<usize>,
    /// `+0x20`: picked this harvest.
    pub picked: bool,
}

/// OrchardComponent `+0x48..+0x60`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Orchard {
    /// `+0x48`: tending on.
    pub enabled: bool,
    /// `+0x4c`: the calendar year of the last harvest.
    pub year: u32,
    pub trees: Vec<TreeSlot>,
    /// `+0x60`: the tree (natural-resource template), chosen in the orchard's window.
    pub tree: Option<usize>,
}

/// FruitComponent `+0x30` ripeness (0–1) and `+0x34` picked this season.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Fruit {
    pub ripeness: f32,
    pub picked: bool,
}

/// FruitDescription (`FUN_14004ab40`): ripening time `+0x20` (× 300 s), the best temperature
/// `+0x24` and its range `+0x28`, the fruit's raw material (pile template) `+0x30`.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct FruitTemplate {
    pub months: f32,
    pub best: f32,
    pub range: f32,
    pub pile: Option<usize>,
}

/// `FUN_14004a740`: FruitComponent's scheduled update. A picked tree waits for the next
/// season (months 7–11 keep it picked). A grown tree ripens at `1 / (months × 300)` per
/// second scaled by `1 − ((t − best) / range)²` (0 outside the range, −10 at or below 32 °F).
pub fn ripen(f: &mut Fruit, d: &FruitTemplate, growth: f32, dt: i64, calendar: f32, t: f32) {
    let rate = 1.0 / (d.months * 300.0);
    let month = (calendar / 3600.0 * 12.0) as i32;
    if f.picked && 4 < (month - 7) as u32 {
        f.picked = false;
    }
    let distance = (t - d.best).abs() / d.range;
    let weight = if !f.picked && 1.0 <= growth {
        if !(0.0..=1.0).contains(&distance) {
            if t <= 32.0 {
                -10.0
            } else {
                0.0
            }
        } else {
            1.0 - distance * distance
        }
    } else {
        0.0
    };
    let g = growth.min(1.0);
    f.ripeness += seconds(dt) * g * rate * weight;
    if f.ripeness < 0.0 {
        f.ripeness = 0.0;
    }
    if 1.0 < f.ripeness {
        f.ripeness = 1.0;
    }
}

impl Sim<'_, '_> {
    fn orchard(&self, e: usize) -> Option<&Orchard> {
        self.sites.get(&e)?.orchard.as_ref()
    }

    fn orchard_mut(&mut self, e: usize) -> Option<&mut Orchard> {
        self.sites.get_mut(&e)?.orchard.as_mut()
    }

    fn orchard_desc(&self, e: usize) -> OrchardTemplate {
        self.site_templates[self.sites[&e].template]
            .orchard
            .unwrap_or_default()
    }

    /// `FUN_140052190`: a standing tree that has gone (or been replaced) is lost.
    fn orchard_refresh(&mut self, e: usize) {
        let objects = &self.cells.objects;
        let placed = &self.placed;
        let templates = self.templates;
        let Some(o) = self.sites.get_mut(&e).and_then(|s| s.orchard.as_mut()) else {
            return;
        };
        for t in o.trees.iter_mut().filter(|t| t.state == 1) {
            let here = objects[t.cell as usize];
            let alive = here.is_some_and(|x| {
                templates[placed[x].template].natural.is_some() && Some(x) == t.entity
            });
            if !alive {
                t.state = 2;
                t.entity = None;
            }
        }
    }

    fn orchard_count(&mut self, e: usize, state: i32) -> usize {
        self.orchard_refresh(e);
        self.orchard(e)
            .map_or(0, |o| o.trees.iter().filter(|t| t.state == state).count())
    }

    /// State 2 enter (`FUN_140051e90`): the workplace is off.
    pub(crate) fn orchard_enter2(&mut self, e: usize) {
        let mut workers = self.workers();
        self.workforce.set_enabled(&mut workers, e, false);
        self.set_workers(&workers);
    }

    /// State 2 update (`FUN_140051e20`) and message 3 (`FUN_140051db0`): switched on, plant.
    pub(crate) fn orchard_update2(&mut self, e: usize) {
        if self.orchard(e).is_some_and(|o| o.enabled) {
            self.machine(e).request(9);
        }
    }

    /// State 9 enter (`FUN_1400519d0`): a tree slot every `spacing` cells (rows, then
    /// columns), the footprint lifted, then PlantOrchard.
    pub(crate) fn orchard_enter9(&mut self, jobs: &mut Jobs, e: usize) {
        let on = self.orchard(e).is_some_and(|o| o.enabled);
        let mut workers = self.workers();
        self.workforce.set_enabled(&mut workers, e, on);
        self.set_workers(&workers);
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
    fn orchard_plant_work(&mut self, jobs: &mut Jobs, e: usize) {
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

    /// Message 5 (`FUN_140051d40`): clear the orchard (0x14).
    pub fn orchard_clear_now(&mut self, e: usize) {
        self.machine(e).request(0x14);
    }

    /// Message 4 (`FUN_140051260`): harvest now; nothing is picked yet this harvest.
    pub fn orchard_harvest_now(&mut self, e: usize) {
        self.machine(e).request(0xb);
        let year = self.year;
        if let Some(o) = self.orchard_mut(e) {
            for t in &mut o.trees {
                t.picked = false;
            }
            o.year = year;
        }
    }

    /// `FUN_140051fa0`: the mean ripeness of the standing trees (at most 1 each) over all.
    fn orchard_ripeness(&self, e: usize) -> f32 {
        let Some(o) = self.orchard(e) else { return 0.0 };
        if o.trees.is_empty() {
            return 1.0;
        }
        let sum: f32 = o
            .trees
            .iter()
            .filter(|t| t.state == 1 && t.entity.is_some())
            .filter(|t| self.cells.objects[t.cell as usize] == t.entity)
            .map(|t| {
                let x = t.entity.unwrap();
                self.fruits.get(&x).map_or(0.0, |f| f.ripeness.min(1.0))
            })
            .sum();
        sum / o.trees.len() as f32
    }

    /// `FUN_140051ee0`: switched on and not yet harvested this year, harvest when warm in months
    /// 7–11 with any fruit, or when all is ripe (> 0.99).
    fn orchard_harvest_due(&mut self, e: usize) -> bool {
        let Some(o) = self.orchard(e) else {
            return false;
        };
        if !o.enabled || self.year == o.year {
            return false;
        }
        let month = (self.calendar / 3600.0 * 12.0) as i32;
        let ripe = self.orchard_ripeness(e);
        let autumn = self.temperature >= 33.0 && month >= 0 && ((month - 7) as u32) < 5;
        if (autumn && 0.0 < ripe) || 0.99 < ripe {
            self.orchard_harvest_now(e);
            return true;
        }
        false
    }

    /// State 10 enter (`FUN_140051790`): lost trees → replant (12); else the tending work.
    pub(crate) fn orchard_enter10(&mut self, jobs: &mut Jobs, e: usize) {
        if self.orchard_count(e, 2) > 0 {
            self.machine(e).request(0xc);
            return;
        }
        self.orchard_tend_work(jobs, e);
    }

    fn orchard_tend_work(&mut self, jobs: &mut Jobs, e: usize) {
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
            WorkKind::TendArea(crate::herbalist::HerbalistWork::default()),
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
            WorkKind::Clear(crate::construction::ClearWork {
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

    /// OrchardComponent's message slot (`FUN_1400522f0`). 0: slot `index` was planted (a tree
    /// with fruit stands, else it is lost); 1: the tree was picked (its fruit starts again);
    /// 2: a tending visit ripens its fruit by `ripening time × 300 s × tend`.
    pub fn orchard_message(&mut self, e: usize, message: i32, index: i32) {
        let Some(slot) = self
            .orchard(e)
            .and_then(|o| o.trees.get(index as usize))
            .copied()
        else {
            return;
        };
        let here = self.cells.objects[slot.cell as usize];
        match message {
            0 => {
                let fruit = here.is_some_and(|x| {
                    let t = &self.templates[self.placed[x].template];
                    t.natural.is_some() && t.fruit.is_some()
                });
                if let Some(t) = self
                    .orchard_mut(e)
                    .and_then(|o| o.trees.get_mut(index as usize))
                {
                    if fruit {
                        t.state = 1;
                        t.entity = here;
                    } else {
                        t.state = 2;
                    }
                }
            }
            1 => {
                self.orchard_refresh(e);
                let Some(t) = self
                    .orchard_mut(e)
                    .and_then(|o| o.trees.get_mut(index as usize))
                else {
                    return;
                };
                t.picked = true;
                if let Some(x) = t.entity {
                    let f = self.fruits.entry(x).or_default();
                    f.ripeness = 0.0;
                    f.picked = true;
                }
            }
            2 => {
                self.orchard_refresh(e);
                let Some(x) = here else { return };
                let Some(fd) = self.templates[self.placed[x].template].fruit else {
                    return;
                };
                let tend = self.orchard_desc(e).tend;
                let dt = ticks(fd.months * 300.0 * tend);
                let growth = self.placed[x].growth;
                let (calendar, t) = (self.calendar, self.temperature);
                ripen(
                    self.fruits.entry(x).or_default(),
                    &fd,
                    growth,
                    dt,
                    calendar,
                    t,
                );
            }
            _ => {}
        }
    }

    /// The orchard's buttons (`FUN_140052500`): 0 plant now (message 3, state 2), 1 harvest
    /// now (message 4, states 10 and 12), 2 tending on/off, 3 clear (message 5, states 9–12).
    pub fn orchard_action(&mut self, e: usize, action: i32) {
        let Some(state) = self.sites.get(&e).map(|s| s.machine.state) else {
            return;
        };
        match action {
            0 if state == 2 => self.machine(e).request(9),
            1 if state == 0xa || state == 0xc => self.orchard_harvest_now(e),
            2 => {
                let on = match self.orchard_mut(e) {
                    Some(o) => {
                        o.enabled = !o.enabled;
                        o.enabled
                    }
                    None => return,
                };
                let mut workers = self.workers();
                self.workforce.set_enabled(&mut workers, e, on);
                self.set_workers(&workers);
            }
            3 if matches!(state, 9..=0xc) => self.orchard_clear_now(e),
            _ => {}
        }
    }

    /// The orchard's tree list (as CropUI's slot `+0x58`): choosing the first tree switches
    /// tending on.
    pub fn set_orchard_tree(&mut self, e: usize, tree: Option<usize>) {
        let Some(old) = self.orchard(e).map(|o| o.tree) else {
            return;
        };
        if old.is_none() && tree.is_some() {
            self.orchard_action(e, 2);
        }
        if let Some(o) = self.orchard_mut(e) {
            o.tree = tree;
        }
    }

    /// PlantOrchard slot `+0x20` (`FUN_1400f9fc0`): the first empty, unclaimed slot next to a
    /// reachable cell gets a tree.
    pub(crate) fn plant_orchard_try(&mut self, work: &mut Work, c: usize) -> u8 {
        let e = work.entity;
        self.orchard_refresh(e);
        let WorkKind::PlantOrchard(pa) = &work.kind else {
            return 0;
        };
        if self.work_max(e) < pa.claims.len() as i32 {
            return 1;
        }
        let Some(o) = self.orchard(e).cloned() else {
            return 0;
        };
        let from = self.citizen_cell(c);
        for (i, slot) in o.trees.iter().enumerate() {
            let WorkKind::PlantOrchard(pa) = &work.kind else {
                return 0;
            };
            if pa.need.get(i).copied().unwrap_or(true) || pa.claims.contains_key(&(i as i32)) {
                continue;
            }
            if slot.entity.is_some() {
                continue;
            }
            let Some(nb) = nearest_neighbour(self.size, &self.cells.types, slot.cell, from, 0xe0f)
            else {
                continue;
            };
            if !self.reachable(from, nb) {
                continue;
            }
            let Some(tree) = pa.crop else { return 1 };
            let point = cell_ground(self.size, self.terrain, nb);
            let message = Message {
                component: Component::Orchard,
                message: 0,
                entity: Some(e),
                a: i as i32,
                b: 0,
            };
            self.plant_sequence(
                c,
                work,
                tree,
                point,
                slot.cell,
                Some(e),
                Some(message),
                i as i32,
            );
            self.took_job(c, work);
            if let WorkKind::PlantOrchard(pa) = &mut work.kind {
                pa.claims.insert(i as i32, c);
                pa.need[i] = true;
            }
            return 2;
        }
        1
    }

    /// PlantOrchard refresh (`FUN_1400f9cc0`): slots whose worker lost the task are free.
    pub(crate) fn plant_orchard_refresh(&mut self, work: &mut Work) {
        let id = work.id;
        let WorkKind::PlantOrchard(pa) = &mut work.kind else {
            return;
        };
        let lost: Vec<i32> = pa
            .claims
            .iter()
            .filter(|(_, &c)| self.characters[c].stack.holds_task(id).is_none())
            .map(|(&i, _)| i)
            .collect();
        for i in lost {
            pa.claims.remove(&i);
            if let Some(n) = pa.need.get_mut(i as usize) {
                *n = false;
            }
        }
    }

    /// PlantOrchard complete and fail (`FUN_1400f9f60`, `FUN_1400f9ef0`): the slot is free.
    pub(crate) fn plant_orchard_done(work: &mut Work, index: i32) {
        if let WorkKind::PlantOrchard(pa) = &mut work.kind {
            pa.claims.remove(&index);
            if let Some(n) = pa.need.get_mut(index as usize) {
                *n = false;
            }
        }
    }

    /// TendArea slot `+0x20` (`FUN_1400fa960`) for an orchard: a random tree.
    pub(crate) fn tend_orchard_try(&mut self, work: &mut Work, c: usize) -> u8 {
        let e = work.entity;
        let WorkKind::TendArea(t) = &work.kind else {
            return 0;
        };
        if self.work_max(e) < t.workers.len() as i32 {
            return 1;
        }
        let from = self.citizen_cell(c);
        let count = self.orchard(e).map_or(0, |o| o.trees.len()) as u32;
        let index = self.rng.rand_below(count) as i32;
        let Some(cell) = self
            .orchard(e)
            .and_then(|o| o.trees.get(index as usize))
            .map(|t| t.cell)
        else {
            return 0;
        };
        let Some(nb) = nearest_neighbour(self.size, &self.cells.types, cell, from, WALKABLE) else {
            return 0;
        };
        if !self.reachable(from, nb) {
            return 0;
        }
        let at = cell_ground(self.size, self.terrain, cell);
        let stand = cell_ground(self.size, self.terrain, nb);
        let message = Message {
            component: Component::Orchard,
            message: 2,
            entity: Some(e),
            a: index,
            b: 0,
        };
        self.tend_sequence(c, work, stand, at, message);
        self.took_job(c, work);
        if let WorkKind::TendArea(t) = &mut work.kind {
            t.workers.insert(c);
        }
        2
    }

    /// The picking work's slot `+0x20` (`FUN_1400f2d90`) after the AutoPickup offers.
    pub(crate) fn pick_fruit_try(&mut self, work: &mut Work, c: usize) -> u8 {
        let e = work.entity;
        let WorkKind::PickFruit(pf) = &work.kind else {
            return 0;
        };
        if self.work_max(e) < pf.claims.len() as i32 {
            return 1;
        }
        let group = self.orchard_desc(e).group;
        let (count, limit) = self.stock[group.clamp(0, 20) as usize];
        // Not ported: building status icons (`FUN_1401318c0(icon, 2, limited)`).
        if limit <= count {
            return 1;
        }
        let Some(o) = self.orchard(e).cloned() else {
            return 0;
        };
        let from = self.citizen_cell(c);
        for (i, slot) in o.trees.iter().enumerate() {
            let WorkKind::PickFruit(pf) = &work.kind else {
                return 0;
            };
            if !pf.need[i] || pf.claims.contains_key(&(i as i32)) {
                continue;
            }
            let Some(tree) = self.cells.objects[slot.cell as usize] else {
                if let WorkKind::PickFruit(pf) = &mut work.kind {
                    pf.need[i] = false;
                }
                continue;
            };
            let Some(nb) = nearest_neighbour(self.size, &self.cells.types, slot.cell, from, 0xe0f)
            else {
                continue;
            };
            if !self.reachable(from, nb) {
                continue;
            }
            if self.templates[self.placed[tree].template].natural.is_none() {
                if let WorkKind::PickFruit(pf) = &mut work.kind {
                    pf.need[i] = false;
                }
                continue;
            }
            if self.claimed.contains(&tree) {
                continue;
            }
            let fd = self.templates[self.placed[tree].template]
                .fruit
                .unwrap_or_default();
            let amount = self.fruits.get(&tree).map_or(0.0, |f| f.ripeness);
            let point = cell_ground(self.size, self.terrain, nb);
            self.pick_sequence(c, work, e, point, fd.pile, i % 2 == 0, amount, nb, i as i32);
            self.took_job(c, work);
            if let WorkKind::PickFruit(pf) = &mut work.kind {
                pf.claims.insert(i as i32, c);
                pf.need[i] = false;
            }
            return 2;
        }
        1
    }

    /// `FUN_1400c0c80`: walk to the tree, work 1 s, leave the fruit (scaled by its ripeness,
    /// every other tree only), tell the orchard, report.
    #[allow(clippy::too_many_arguments)]
    fn pick_sequence(
        &mut self,
        c: usize,
        work: &mut Work,
        orchard: usize,
        point: [f32; 3],
        fruit: Option<usize>,
        leave: bool,
        amount: f32,
        cell: i32,
        index: i32,
    ) {
        self.clear_citizen(c, Some(work));
        self.on_empty_think(c);
        self.push(
            c,
            Action::CompleteTask {
                work: work.id,
                param: index,
            },
        );
        self.push(
            c,
            Action::SendMessage(Message {
                component: Component::Orchard,
                message: 1,
                entity: Some(orchard),
                a: index,
                b: 0,
            }),
        );
        if leave {
            if let Some(t) = fruit {
                let batch = self.states[c].carried.capacity;
                self.push(
                    c,
                    Action::CreateRawMaterial {
                        template: t,
                        cell,
                        batch,
                        workplace: self.states[c].workplace,
                        factor: amount,
                    },
                );
            }
        }
        self.push(c, self.perform(c, -1000.0, 8, 1.0, true));
        let walk = MoveTo::new(point, 1, 0, WALKABLE, 0, -1.0, self.rng);
        self.push(c, Action::move_to(walk));
    }

    /// Picking complete and fail (`FUN_1400f2d50`, `FUN_1400f2ce0`).
    pub(crate) fn pick_done(work: &mut Work, index: i32, failed: bool) {
        if let WorkKind::PickFruit(pf) = &mut work.kind {
            pf.claims.remove(&index);
            if failed {
                if let Some(n) = pf.need.get_mut(index as usize) {
                    *n = true;
                }
            }
        }
    }
}
