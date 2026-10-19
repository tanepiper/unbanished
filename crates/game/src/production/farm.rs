//! Crop fields: CropFieldComponent (vtable `0x1401692f0`, states `FUN_140120620`), and the
//! works `PlantArea` (`0x140167cb0`), `TendArea` (`0x140167db0`) and the harvest work
//! (`0x1401678c0`). See docs/FARMING.md.
//!
//! A field waits for spring (state 2), plants its crop on every cell (9), tends it while it
//! grows (10) and harvests it when ripe or in autumn (11). The ClearArea controller's state
//! 0x15 then clears what is left before the field waits again.

use std::collections::{BTreeMap, BTreeSet};

use crate::behaviour::character::{Action, Component, Message, MoveTo};
use crate::pathfind::{nearest_neighbour, WALKABLE};
use crate::scheduler::{seconds, ticks};
use crate::town::Sim;
use crate::work::construction::Callbacks;
use crate::work::jobs::{Work, WorkKind};

/// `DAT_1401788d4`: the warmth planting and the autumn harvest need (°F).
const WARM: f32 = 33.0;

/// One cell of a field (`+0x58`, 0x20 bytes).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CropCell {
    /// `+0x00`: the map cell.
    pub cell: i32,
    /// `+0x04`: 0 to plant, 1 growing, 2 done (harvested or lost).
    pub state: i32,
    /// `+0x08`: the crop entity.
    pub entity: Option<usize>,
    /// `+0x18`: harvesting this cell leaves a pile.
    pub pile: bool,
}

/// CropFieldComponent `+0x48..+0x60`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CropField {
    /// `+0x48`: farming on (the field's toggle).
    pub enabled: bool,
    /// `+0x49`: the crop leaves piles (its natural resource names a raw material).
    pub piles: bool,
    /// `+0x50`: the crop (natural-resource template), chosen in the field's window.
    pub crop: Option<usize>,
    pub cells: Vec<CropCell>,
}

/// CropFieldDescription (`FUN_140120740`, versioned): `+0x24` a tending visit's growth (as a
/// share of the crop's growing time) and, from version 1, `+0x20` the stock slot.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CropFieldTemplate {
    pub tend: f32,
    pub group: i32,
}

/// PlantArea and the harvest work: `+0x48` the cells claimed (index → worker), `+0x58`
/// the cells still to do, `+0x68` (PlantArea) the crop.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AreaWork {
    pub claims: BTreeMap<i32, usize>,
    pub need: Vec<bool>,
    pub crop: Option<usize>,
}

impl Sim<'_, '_> {
    fn month(&self) -> i32 {
        (self.calendar / 3600.0 * 12.0) as i32
    }

    fn field(&self, e: usize) -> Option<&CropField> {
        self.sites.get(&e)?.crop_field.as_ref()
    }

    fn field_mut(&mut self, e: usize) -> Option<&mut CropField> {
        self.sites.get_mut(&e)?.crop_field.as_mut()
    }

    fn set_workplace_enabled(&mut self, e: usize, on: bool) {
        self.enable_workplace(e, on);
    }

    /// State 2 enter (`FUN_14011fdf0`): the workplace follows the toggle.
    pub(crate) fn crop_enter2(&mut self, e: usize) {
        let on = self.field(e).is_some_and(|f| f.enabled);
        self.set_workplace_enabled(e, on);
    }

    /// State 2 update (`FUN_14011fd30`): switched on, warm and in spring (months 0–2), plant.
    pub(crate) fn crop_update2(&mut self, e: usize) {
        if !self.field(e).is_some_and(|f| f.enabled) || self.temperature < WARM {
            return;
        }
        let month = self.month();
        if month >= 0 && 8 < (month - 3) as u32 {
            self.crop_plant_now(e);
        }
    }

    /// `FUN_14011fc80` (also the field's message 3 in state 2): plant now.
    pub fn crop_plant_now(&mut self, e: usize) {
        let on = self.field(e).is_some_and(|f| f.enabled);
        self.set_workplace_enabled(e, on);
        self.machine(e).request(9);
    }

    /// CropUI slot `+0x58` (`FUN_1400b7df0`).
    pub fn set_crop(&mut self, e: usize, crop: Option<usize>) {
        let Some(old) = self.field(e).map(|f| f.crop) else {
            return;
        };
        if old.is_none() && crop.is_some() {
            self.field_action(e, 2);
        }
        if let Some(f) = self.field_mut(e) {
            f.crop = crop;
        }
    }

    /// CropFieldComponent slot `+0x70` (`FUN_140120190`): 0 sends the machine message 3
    /// (handled in state 2: plant now), 1 message 4 (state 10: harvest now), 2 switches
    /// farming on or off.
    pub fn field_action(&mut self, e: usize, action: i32) {
        let Some(m) = self.sites.get(&e).map(|s| &s.machine) else {
            return;
        };
        let state = m.state;
        match action {
            0 if state == 2 => self.crop_plant_now(e),
            1 if state == 0xa => self.crop_harvest_now(e),
            2 => {
                let on = match self.field_mut(e) {
                    Some(f) => {
                        f.enabled = !f.enabled;
                        f.enabled
                    }
                    None => return,
                };
                self.set_workplace_enabled(e, on);
            }
            _ => {}
        }
    }

    /// The field's message 4 in state 10 (`FUN_14011f3d0`): harvest now.
    pub fn crop_harvest_now(&mut self, e: usize) {
        self.machine(e).request(0xb);
    }

    /// State 9 enter (`FUN_14011f7e0`): one cell per footprint cell (rows, then columns);
    /// every `pile_spacing`-th cell leaves a pile, shuffled with `Rand(count)` swaps; then the
    /// planting work.
    pub(crate) fn crop_enter9(&mut self, jobs: &mut crate::work::jobs::Jobs, e: usize) {
        let n = self.size as i32;
        let (x0, y0) = self.placed[e].cell;
        let (w, h) = self.placed[e].footprint;
        let mut cells = Vec::with_capacity((w * h) as usize);
        for row in 0..h {
            for col in 0..w {
                cells.push(CropCell {
                    cell: (row + y0) * n + col + x0,
                    ..Default::default()
                });
            }
        }
        let crop = self.field(e).and_then(|f| f.crop);
        let spacing = crop
            .and_then(|t| self.templates[t].pile)
            .and_then(|p| self.templates[p].pile_material)
            .map(|m| self.materials[m].pile_spacing);
        let count = cells.len();
        let mut flags: Vec<bool> = match spacing {
            Some(k) => (0..count as i32).map(|i| k != 0 && i % k == 0).collect(),
            None => vec![false; count],
        };
        for i in 0..count {
            let r = self.rng.rand_below(count as u32) as usize;
            if i != r {
                flags.swap(i, r);
            }
        }
        for (c, f) in cells.iter_mut().zip(flags) {
            c.pile = f;
        }
        if let Some(field) = self.field_mut(e) {
            field.piles = spacing.is_some();
            field.cells = cells;
        }
        // `FUN_140129c20`: the field's footprint is lifted, so crops can stand on it.
        self.unstamp(e);
        let profession = self.profession_of(e);
        let work = Work::new(
            e,
            self.rect_cell(e),
            profession,
            WorkKind::PlantArea(AreaWork {
                claims: BTreeMap::new(),
                need: vec![true; count],
                crop,
            }),
        );
        self.set_work(jobs, e, work, 1, false);
    }

    /// The workplace's profession (slot `+0x50` of the works).
    fn profession_of(&self, e: usize) -> usize {
        self.workforce
            .workplaces
            .get(&e)
            .and_then(|w| w.profession)
            .unwrap_or(self.professions.laborer)
    }

    /// `FUN_14011fef0`: a growing cell whose crop has gone (or been replaced) is done.
    fn crop_refresh(&mut self, e: usize) {
        let objects = &self.cells.objects;
        let placed = &self.placed;
        let templates = self.templates;
        let Some(field) = self.sites.get_mut(&e).and_then(|s| s.crop_field.as_mut()) else {
            return;
        };
        for c in field.cells.iter_mut().filter(|c| c.state == 1) {
            let here = objects[c.cell as usize];
            let alive = here.is_some_and(|o| {
                templates[placed[o].template].natural.is_some() && Some(o) == c.entity
            });
            if !alive {
                c.state = 2;
                c.entity = None;
            }
        }
    }

    fn crop_counts(&mut self, e: usize) -> (usize, usize, usize) {
        self.crop_refresh(e);
        let Some(field) = self.field(e) else {
            return (0, 0, 0);
        };
        let growing = field.cells.iter().filter(|c| c.state == 1).count();
        let done = field.cells.iter().filter(|c| c.state == 2).count();
        (field.cells.len(), growing, done)
    }

    /// State 9 update (`FUN_14011f6b0`): everything lost → clear (0x15); everything planted
    /// or lost → grow (10).
    pub(crate) fn crop_update9(&mut self, jobs: &mut crate::work::jobs::Jobs, e: usize) {
        let (total, growing, done) = self.crop_counts(e);
        if done == total {
            self.remove_work(jobs, e);
            self.machine(e).request(0x15);
        } else if growing + done == total {
            self.remove_work(jobs, e);
            self.machine(e).request(10);
        }
    }

    /// State 10 enter (`FUN_14011f5d0`): the tending work.
    pub(crate) fn crop_enter10(&mut self, jobs: &mut crate::work::jobs::Jobs, e: usize) {
        let profession = self.profession_of(e);
        let work = Work::new(
            e,
            self.rect_cell(e),
            profession,
            WorkKind::TendArea(crate::production::herbalist::HerbalistWork::default()),
        );
        self.set_work(jobs, e, work, 1, false);
    }

    /// State 10 exit (`FUN_14011f440`).
    pub(crate) fn crop_exit10(&mut self, jobs: &mut crate::work::jobs::Jobs, e: usize) {
        let id = self.sites[&e].work_id;
        if id >= 0 && jobs.works.contains_key(&id) {
            self.remove_work(jobs, e);
        }
    }

    /// `FUN_1401208e0`: the mean growth of the growing cells (at most 1 each) over all cells.
    fn crop_ripeness(&mut self, e: usize) -> f32 {
        self.crop_counts(e);
        self.field(e).map_or(0.0, |f| ripeness(f, self.placed))
    }

    /// State 10 update (`FUN_14011f4a0`): everything lost → clear; with piles and switched
    /// on, harvest when ripe, or when warm in months 6–11.
    pub(crate) fn crop_update10(&mut self, e: usize) {
        let (total, _, done) = self.crop_counts(e);
        if done == total {
            self.machine(e).request(0x15);
        }
        let Some(field) = self.field(e) else { return };
        if !field.enabled || !field.piles {
            return;
        }
        let month = self.month();
        let autumn = self.temperature >= WARM && month >= 0 && (month - 6) as u32 <= 5;
        if !autumn && self.crop_ripeness(e) <= 0.99 {
            return;
        }
        self.crop_harvest_now(e);
    }

    /// State 11 enter (`FUN_14011f330`): the harvest work (`FUN_1400f2990`).
    pub(crate) fn crop_enter11(&mut self, jobs: &mut crate::work::jobs::Jobs, e: usize) {
        let count = self.field(e).map_or(0, |f| f.cells.len());
        let profession = self.profession_of(e);
        let work = Work::new(
            e,
            self.rect_cell(e),
            profession,
            WorkKind::Harvest(AreaWork {
                claims: BTreeMap::new(),
                need: vec![true; count],
                crop: None,
            }),
        );
        self.set_work(jobs, e, work, 1, false);
    }

    /// State 11 update (`FUN_14011f250`): everything harvested → clear.
    pub(crate) fn crop_update11(&mut self, jobs: &mut crate::work::jobs::Jobs, e: usize) {
        let (total, _, done) = self.crop_counts(e);
        if done == total {
            self.remove_work(jobs, e);
            self.machine(e).request(0x15);
        }
    }

    /// CropFieldComponent's message slot (`FUN_140120020`). Message 0: cell `index` was
    /// planted (its crop keeps the cell's pile flag, NaturalResource `+0x2d`); message 1: a
    /// tending visit advances the cell's crop by `growing time × 300 s × tend`.
    pub fn crop_message(&mut self, e: usize, message: i32, index: i32) {
        let Some(field) = self.field(e) else { return };
        let Some(&c) = field.cells.get(index as usize) else {
            return;
        };
        let here = self.cells.objects[c.cell as usize];
        match message {
            0 => {
                if let Some(o) = here {
                    self.keep_piles.insert(o, c.pile);
                }
                if let Some(cell) = self
                    .field_mut(e)
                    .and_then(|f| f.cells.get_mut(index as usize))
                {
                    cell.state = 1;
                    cell.entity = here;
                }
            }
            1 => {
                let Some(o) = here else { return };
                let tend = self.site_templates[self.sites[&e].template]
                    .crop_field
                    .map_or(0.0, |d| d.tend);
                let template = self.placed[o].template;
                let Some(g) = self.templates[template].growth.clone() else {
                    return;
                };
                let dt = ticks(g.f28 * 300.0 * tend);
                let temperature = self.temperature;
                let p = &mut self.placed[o];
                let Some(state) = &mut p.growth_state else {
                    return;
                };
                let result = state.update(&g, &mut p.growth, dt, temperature, false);
                // A terminal transition reached by tending, as the scheduled update handles
                // it (`FUN_140125940`): the map byte, then the crop falls or goes (on the next
                // animation pass).
                if let Some(adjustment) = result.map_adjustment {
                    let n = self.size;
                    let (x, y) = self.placed[o].cell;
                    let i = y as usize * n + x as usize;
                    self.cells.growth_bytes[i] = crate::nature::growth::adjust_map_byte(
                        self.cells.growth_bytes[i],
                        adjustment,
                    );
                    self.cells.previous_template[i] = Some(template);
                    let falls = self.templates[template]
                        .natural
                        .as_ref()
                        .is_some_and(|nr| nr.flag);
                    let p = &mut self.placed[o];
                    p.removal_requested = true;
                    if falls {
                        p.animation = self.templates[template]
                            .animations
                            .get(p.variant)
                            .and_then(Option::as_ref)
                            .and_then(|g| g.clips.get(2))
                            .map(|_| crate::behaviour::animation::Playback::new(2, 0));
                    }
                    *self.models_dirty = true;
                }
            }
            _ => {}
        }
    }

    /// PlantArea slot `+0x20` (`FUN_1400f94e0`).
    pub(crate) fn plant_try(&mut self, work: &mut Work, c: usize) -> u8 {
        let e = work.entity;
        let WorkKind::PlantArea(pa) = &work.kind else {
            return 0;
        };
        if self.work_max(e) < pa.claims.len() as i32 {
            return 1;
        }
        let Some(field) = self.field(e).cloned() else {
            return 0;
        };
        let from = self.citizen_cell(c);
        let count = pa.need.len() as i32;
        let mut nothing = true;
        let mut i = 0;
        while i < count {
            let pa = match &work.kind {
                WorkKind::PlantArea(pa) => pa,
                _ => return 0,
            };
            if !pa.need[i as usize] {
                i += 1;
                continue;
            }
            nothing = false;
            if pa.claims.contains_key(&i) {
                i += 1;
                continue;
            }
            let j = i + 1;
            let pair = j < count && pa.need[j as usize] && !pa.claims.contains_key(&j);
            let cell_i = field.cells[i as usize].cell;
            let cell_j = if pair {
                field.cells[j as usize].cell
            } else {
                -1
            };
            let Some(nb) = nearest_neighbour(self.size, &self.cells.types, cell_i, from, 0xe0f)
            else {
                i += 1;
                continue;
            };
            if !self.reachable(from, nb) {
                i += 1;
                continue;
            }
            let crop = pa.crop;
            let point = self.ground_at(nb);
            let param = (if pair { j } else { count }) << 16 | i;
            let cells: Vec<(i32, i32)> = if pair {
                vec![(cell_i, i), (cell_j, j)]
            } else {
                vec![(cell_i, i)]
            };
            self.plant_crop_sequence(c, work, e, crop, point, &cells, param);
            self.took_job(c, work);
            if let WorkKind::PlantArea(pa) = &mut work.kind {
                pa.claims.insert(i, c);
                pa.need[i as usize] = false;
                if pair {
                    pa.claims.insert(j, c);
                    pa.need[j as usize] = false;
                }
            }
            return 2;
        }
        if nothing {
            let idle = matches!(&work.kind, WorkKind::PlantArea(pa) if pa.claims.is_empty());
            if idle {
                self.crop_planted(e);
            }
        }
        1
    }

    /// `FUN_140120860`: planting is over; cells never planted count as done.
    fn crop_planted(&mut self, e: usize) {
        let m = &self.sites[&e].machine;
        let state = if m.pending < 0 { m.state } else { m.pending };
        if state != 9 {
            return;
        }
        if let Some(f) = self.field_mut(e) {
            for c in f.cells.iter_mut().filter(|c| c.state == 0) {
                c.state = 2;
            }
        }
    }

    /// `FUN_1400c31f0` as PlantArea calls it: take the hoe, walk to the cell, work 2.1 s,
    /// plant each cell (the field occupies it) and tell the field, put the hoe away, report.
    #[allow(clippy::too_many_arguments)]
    fn plant_crop_sequence(
        &mut self,
        c: usize,
        work: &mut Work,
        field: usize,
        crop: Option<usize>,
        point: [f32; 3],
        cells: &[(i32, i32)],
        param: i32,
    ) {
        self.clear_citizen(c, Some(work));
        self.on_empty_think(c);
        self.push(
            c,
            Action::CompleteTask {
                work: work.id,
                param,
            },
        );
        self.push(c, Action::ShowTool(-1));
        for &(cell, index) in cells {
            self.push(
                c,
                Action::SendMessage(Message {
                    component: Component::CropField,
                    message: 0,
                    entity: Some(field),
                    a: index,
                    b: 0,
                }),
            );
            if let Some(t) = crop {
                self.push(
                    c,
                    Action::CreateNaturalResource {
                        template: t,
                        occupant: Some(field),
                        cell,
                    },
                );
            }
        }
        self.push(c, self.perform(c, -1000.0, 0xc, 2.1, true));
        let walk = MoveTo::new(point, 3, 0, WALKABLE, 0, -1.0, self.rng);
        self.push(c, Action::move_to(walk));
        self.push(c, Action::ShowTool(3));
    }

    /// TendArea slot `+0x20` (`FUN_1400fa960`) for a crop field.
    pub(crate) fn tend_try(&mut self, work: &mut Work, c: usize) -> u8 {
        let e = work.entity;
        let WorkKind::TendArea(t) = &work.kind else {
            return 0;
        };
        if self.work_max(e) < t.workers.len() as i32 {
            return 1;
        }
        let from = self.citizen_cell(c);
        let Some(field) = self.field(e) else { return 0 };
        let count = field.cells.len() as u32;
        let index = self.rng.rand_below(count) as i32;
        let Some(cell) = self
            .field(e)
            .and_then(|f| f.cells.get(index as usize))
            .map(|c| c.cell)
        else {
            return 0;
        };
        let Some(nb) = nearest_neighbour(self.size, &self.cells.types, cell, from, WALKABLE) else {
            return 0;
        };
        if !self.reachable(from, nb) {
            return 0;
        }
        let at = self.ground_at(cell);
        let stand = self.ground_at(nb);
        let message = Message {
            component: Component::CropField,
            message: 1,
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

    /// `FUN_1400c0570`: take the hoe, walk next to the cell, work 10 s facing it, tell the
    /// field, put the hoe away, report.
    pub(crate) fn tend_sequence(
        &mut self,
        c: usize,
        work: &mut Work,
        stand: [f32; 3],
        at: [f32; 3],
        message: Message,
    ) {
        self.clear_citizen(c, Some(work));
        self.on_empty_think(c);
        self.push(
            c,
            Action::CompleteTask {
                work: work.id,
                param: 0,
            },
        );
        let heading = crate::behaviour::character::work_facing(at, stand);
        self.push(c, Action::ShowTool(-1));
        self.push(c, Action::SendMessage(message));
        self.push(c, self.perform(c, heading, 0xc, 10.0, true));
        let walk = MoveTo::new(stand, 3, 0, WALKABLE, 0, -1.0, self.rng);
        self.push(c, Action::move_to(walk));
        self.push(c, Action::ShowTool(3));
    }

    /// The harvest work's slot `+0x20` (`FUN_1400f21a0`) after the AutoPickup offers.
    pub(crate) fn harvest_try(&mut self, work: &mut Work, c: usize) -> u8 {
        let e = work.entity;
        let group = self.site_templates[self.sites[&e].template]
            .crop_field
            .map_or(0, |d| d.group);
        let (count_s, limit) = self.stock[group.clamp(0, 20) as usize];
        self.set_icon(e, crate::work::professions::icon::LIMIT, limit <= count_s);
        if limit <= count_s {
            return 1;
        }
        let WorkKind::Harvest(hw) = &work.kind else {
            return 0;
        };
        if self.work_max(e) < hw.claims.len() as i32 {
            return 1;
        }
        let Some(field) = self.field(e).cloned() else {
            return 0;
        };
        let from = self.citizen_cell(c);
        let count = field.cells.len() as i32;
        for i in 0..count {
            let hw = match &work.kind {
                WorkKind::Harvest(hw) => hw,
                _ => return 0,
            };
            if !hw.need[i as usize] || hw.claims.contains_key(&i) {
                continue;
            }
            let j = i + 1;
            let mut pair = j < count && hw.need[j as usize] && !hw.claims.contains_key(&j);
            let Some(first) = self.cells.objects[field.cells[i as usize].cell as usize] else {
                if let WorkKind::Harvest(hw) = &mut work.kind {
                    hw.need[i as usize] = false;
                }
                continue;
            };
            let second = if pair {
                self.cells.objects[field.cells[j as usize].cell as usize]
            } else {
                None
            };
            if second.is_none() {
                pair = false;
            }
            let Some(nb) = nearest_neighbour(
                self.size,
                &self.cells.types,
                field.cells[i as usize].cell,
                from,
                0xe0f,
            ) else {
                continue;
            };
            if !self.reachable(from, nb) {
                continue;
            }
            if self.claimed.contains(&first) {
                continue;
            }
            if pair && second.is_some_and(|s| self.claimed.contains(&s)) {
                pair = false;
            }
            let high = if pair { j } else { count };
            let point = self.ground_at(nb);
            let mut crops = vec![first];
            if pair {
                crops.extend(second);
            }
            self.harvest_sequence(c, work, &crops, point, high << 16 | i);
            self.took_job(c, work);
            if let WorkKind::Harvest(hw) = &mut work.kind {
                hw.claims.insert(i, c);
                hw.need[i as usize] = false;
                if pair {
                    hw.claims.insert(j, c);
                    hw.need[j as usize] = false;
                }
            }
            return 2;
        }
        1
    }

    /// `FUN_1400c6a00`: the removal sequence over one or two crops (no falling animation),
    /// with the auto-pickup flag.
    fn harvest_sequence(
        &mut self,
        c: usize,
        work: &mut Work,
        crops: &[usize],
        point: [f32; 3],
        param: i32,
    ) {
        let Some(nr) = self.templates[self.placed[crops[0]].template]
            .natural
            .clone()
        else {
            return;
        };
        let facing = crate::behaviour::character::work_facing(self.facing_centre(crops[0]), point);
        self.clear_citizen(c, Some(work));
        self.on_empty_think(c);
        let tool = nr.value as i32;
        let clip = if nr.value < 11 { tool + 9 } else { 8 } as usize;
        self.push(
            c,
            Action::CompleteTask {
                work: work.id,
                param,
            },
        );
        self.push(c, Action::ShowTool(-1));
        for &o in crops {
            self.push(
                c,
                Action::RemoveResource {
                    entity: o,
                    flag: true,
                },
            );
            self.claimed.insert(o);
        }
        self.push(c, self.perform(c, facing, clip, seconds(nr.time_a), true));
        let walk = match tool {
            0 | 1 | 4..=7 => 2,
            2 | 3 => 3,
            _ => 1,
        };
        let m = MoveTo::new(point, walk, 0, WALKABLE, 0, -1.0, self.rng);
        self.push(c, Action::move_to(m));
        self.push(c, Action::ShowTool(tool));
    }

    /// PlantArea and harvest slot `+0x30` (`FUN_1400f9430` / `FUN_1400f20f0`): the cells are
    /// free again.
    pub(crate) fn area_complete(work: &mut Work, param: i32) {
        let (WorkKind::PlantArea(a) | WorkKind::Harvest(a)) = &mut work.kind else {
            return;
        };
        let (i, j) = (param & 0xffff, (param >> 16) & 0xffff);
        a.claims.remove(&i);
        if (j as usize) < a.need.len() {
            a.claims.remove(&j);
        }
    }

    /// Slot `+0x38` (`FUN_1400f9370` / `FUN_1400f2030`): the cells are to do again.
    pub(crate) fn area_fail(work: &mut Work, param: i32) {
        let (WorkKind::PlantArea(a) | WorkKind::Harvest(a)) = &mut work.kind else {
            return;
        };
        let (i, j) = (param & 0xffff, (param >> 16) & 0xffff);
        a.claims.remove(&i);
        if let Some(n) = a.need.get_mut(i as usize) {
            *n = true;
        }
        if (j as usize) < a.need.len() {
            a.need[j as usize] = true;
            a.claims.remove(&j);
        }
    }

    /// Slot `+0x48` (`FUN_1400f9140` / `FUN_1400f1e00`): cells whose worker lost the task are
    /// to do again.
    pub(crate) fn area_refresh(&mut self, work: &mut Work) {
        let id = work.id;
        let (WorkKind::PlantArea(a) | WorkKind::Harvest(a)) = &mut work.kind else {
            return;
        };
        let lost: Vec<i32> = a
            .claims
            .iter()
            .filter(|(_, &c)| self.characters[c].stack.holds_task(id).is_none())
            .map(|(&i, _)| i)
            .collect();
        for i in lost {
            a.claims.remove(&i);
            if let Some(n) = a.need.get_mut(i as usize) {
                *n = true;
            }
        }
    }

    /// The workers of an area work, for the cancel slot (`FUN_1400f9930` / `FUN_1400f2740`).
    pub(crate) fn area_workers(work: &Work) -> BTreeSet<usize> {
        match &work.kind {
            WorkKind::PlantArea(a) | WorkKind::Harvest(a) => a.claims.values().copied().collect(),
            _ => BTreeSet::new(),
        }
    }
}

/// [`Sim::crop_ripeness`] after the cells' refresh (`FUN_14011fef0`).
pub fn ripeness(field: &CropField, placed: &[crate::nature::seeding::Placed]) -> f32 {
    if field.cells.is_empty() {
        return 0.0;
    }
    let sum: f32 = field
        .cells
        .iter()
        .filter(|c| c.state == 1)
        .filter_map(|c| c.entity)
        .map(|o| placed[o].growth.min(1.0))
        .sum();
    sum / field.cells.len() as f32
}
