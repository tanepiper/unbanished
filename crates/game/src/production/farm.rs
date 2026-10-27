//! Crop fields: CropFieldComponent, and the works `PlantArea`, `TendArea` and the harvest work. See
//! docs/FARMING.md.
//!
//! A field waits for spring (state 2), plants its crop on every cell (9), tends it while it grows
//! (10) and harvests it when ripe or in autumn (11). The ClearArea controller's state 0x15 then
//! clears what is left before the field waits again.
//!
//! Original: `FUN_140120620`, `0x140167cb0`, `0x140167db0`, `0x1401678c0`.
mod harvest;
mod planting;

// vtable 0x1401692f0

use std::collections::{BTreeMap, BTreeSet};

use crate::behaviour::character::{Action, Component, Message, MoveTo};
use crate::pathfind::{nearest_neighbour, WALKABLE};
use crate::scheduler::{seconds, ticks};
use crate::town::Sim;
use crate::work::construction::Callbacks;
use crate::work::jobs::{Work, WorkKind};

/// `DAT_1401788d4`: the warmth planting and the autumn harvest need (°F).
const WARM: f32 = 33.0;

// +0x58
/// One cell of a field (0x20 bytes).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CropCell {
    // +0x00
    /// The map cell.
    pub cell: i32,
    // +0x04
    /// 0 to plant, 1 growing, 2 done (harvested or lost).
    pub state: i32,
    // +0x08
    /// The crop entity.
    pub entity: Option<usize>,
    // +0x18
    /// Harvesting this cell leaves a pile.
    pub pile: bool,
}

// +0x48..+0x60
/// CropFieldComponent.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CropField {
    // +0x48
    /// Farming on (the field's toggle).
    pub enabled: bool,
    // +0x49
    /// The crop leaves piles (its natural resource names a raw material).
    pub piles: bool,
    // +0x50
    /// The crop (natural-resource template), chosen in the field's window.
    pub crop: Option<usize>,
    /// The field's cells.
    pub cells: Vec<CropCell>,
}

// +0x24; +0x20
/// CropFieldDescription (versioned): a tending visit's growth (as a share of the crop's growing
/// time) and, from version 1, the stock slot.
///
/// Original: `FUN_140120740`.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CropFieldTemplate {
    /// A tending visit's growth, as a share of the crop's growing time.
    pub tend: f32,
    /// The stock slot (version 1).
    pub group: i32,
}

// +0x48; +0x58; +0x68
/// The work of planting (`PlantArea`) or harvesting a field: the cells claimed (index → worker),
/// the cells still to do, and, for planting, the crop.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AreaWork {
    /// The cells claimed: cell index → worker.
    pub claims: BTreeMap<i32, usize>,
    /// The cells still to do.
    pub need: Vec<bool>,
    /// The crop, for planting.
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
        // jb at 0x14011fd6b: a NaN temperature is too cold to plant.
        #[allow(clippy::neg_cmp_op_on_partial_ord)]
        if !self.field(e).is_some_and(|f| f.enabled) || !(WARM <= self.temperature) {
            return;
        }
        let month = self.month();
        if month >= 0 && 8 < (month - 3) as u32 {
            self.crop_plant_now(e);
        }
    }

    /// Plant now (also the field's message 3 in state 2).
    ///
    /// Original: `FUN_14011fc80`.
    pub fn crop_plant_now(&mut self, e: usize) {
        let on = self.field(e).is_some_and(|f| f.enabled);
        self.set_workplace_enabled(e, on);
        self.machine(e).request(9);
    }

    // +0x58
    /// CropUI slot.
    ///
    /// Original: `FUN_1400b7df0`.
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

    // +0x70
    /// CropFieldComponent slot: 0 sends the machine message 3 (handled in state 2: plant now), 1
    /// message 4 (state 10: harvest now), 2 switches farming on or off.
    ///
    /// Original: `FUN_140120190`.
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

    /// The field's message 4 in state 10: harvest now.
    ///
    /// Original: `FUN_14011f3d0`.
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

    // +0x2d
    /// CropFieldComponent's message slot. Message 0: cell `index` was planted (its crop keeps the
    /// cell's pile flag, NaturalResource); message 1: a tending visit advances the cell's crop by
    /// `growing time × 300 s × tend`.
    ///
    /// Original: `FUN_140120020`.
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
}

/// `Sim::crop_ripeness` after the cells' refresh.
///
/// Original: `FUN_14011fef0`.
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
