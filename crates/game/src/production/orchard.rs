//! Orchards: OrchardComponent, FruitComponent on the trees, and the works PlantOrchard, TendArea
//! (as an orchard's) and fruit picking. See docs/FARMING.md "Orchards".
//!
//! Original: `FUN_140052a40`, `0x14015f5a0`, `0x140167d30`, `0x140167940`.
mod states;
mod work;

// vtable 0x1401602a0

use std::collections::BTreeMap;

use crate::behaviour::character::{Action, Component, Message, MoveTo};
use crate::pathfind::{nearest_neighbour, WALKABLE};
use crate::production::farm::AreaWork;
use crate::scheduler::{seconds, ticks};
use crate::town::Sim;
use crate::work::construction::Callbacks;
use crate::work::jobs::{Jobs, Work, WorkKind};

// +0x24; +0x28; +0x2c; +0x20
/// OrchardDescription (versioned): the tree spacing, a tending visit's ripening (share of the
/// ripening time), the stock slot (version 1).
///
/// Original: `FUN_140052c60`.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct OrchardTemplate {
    /// The tree spacing.
    pub spacing: (i32, i32),
    /// A tending visit's ripening, as a share of the ripening time.
    pub tend: f32,
    /// The stock slot (version 1).
    pub group: i32,
}

// +0x50
/// One tree slot (0x28 bytes).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TreeSlot {
    // +0x00
    /// The map cell.
    pub cell: i32,
    // +0x04
    /// 0 to plant, 1 standing, 2 lost.
    pub state: i32,
    // +0x08
    /// The tree.
    pub entity: Option<usize>,
    // +0x20
    /// Picked this harvest.
    pub picked: bool,
}

// +0x48..+0x60
/// OrchardComponent.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Orchard {
    // +0x48
    /// Tending on.
    pub enabled: bool,
    // +0x4c
    /// The calendar year of the last harvest.
    pub year: u32,
    /// The tree slots.
    pub trees: Vec<TreeSlot>,
    // +0x60
    /// The tree (natural-resource template), chosen in the orchard's window.
    pub tree: Option<usize>,
}

// +0x30; +0x34
/// FruitComponent ripeness (0–1) and picked this season.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Fruit {
    /// The ripeness, from 0 to 1.
    pub ripeness: f32,
    /// Whether it was picked this season.
    pub picked: bool,
}

// +0x20; +0x24; +0x28; +0x30
/// FruitDescription: ripening time (× 300 s), the best temperature and its range, the fruit's raw
/// material (pile template).
///
/// Original: `FUN_14004ab40`.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct FruitTemplate {
    /// The ripening time, in months of 300 seconds.
    pub months: f32,
    /// The best temperature.
    pub best: f32,
    /// The range of temperatures around the best one.
    pub range: f32,
    /// The fruit's pile template.
    pub pile: Option<usize>,
}

/// FruitComponent's scheduled update. A picked tree waits for the next season (months 7–11 keep it
/// picked). A grown tree ripens at `1 / (months × 300)` per second scaled by `1 − ((t − best) /
/// range)²` (0 outside the range, −10 at or below 32 °F).
///
/// Measured: `tools/emu/ripen.py` (the original on 2,000 trees, NaN growths and ranges of 0
/// among them).
///
/// Original: `FUN_14004a740`.
pub fn ripen(f: &mut Fruit, d: &FruitTemplate, growth: f32, dt: i64, calendar: f32, t: f32) {
    let rate = 1.0 / (d.months * 300.0);
    let month = (calendar / 3600.0 * 12.0) as i32;
    if f.picked && 4 < (month - 7) as u32 {
        f.picked = false;
    }
    let distance = (t - d.best).abs() / d.range;
    // ja at 0x14004a841 skips only when 1 > growth: a NaN growth counts as grown.
    #[allow(clippy::neg_cmp_op_on_partial_ord)]
    let weight = if !f.picked && !(growth < 1.0) {
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
        self.enable_workplace(e, false);
    }

    /// State 2 update (`FUN_140051e20`) and message 3 (`FUN_140051db0`): switched on, plant.
    pub(crate) fn orchard_update2(&mut self, e: usize) {
        if self.orchard(e).is_some_and(|o| o.enabled) {
            self.machine(e).request(9);
        }
    }

    /// Message 5: clear the orchard (0x14).
    ///
    /// Original: `FUN_140051d40`.
    pub fn orchard_clear_now(&mut self, e: usize) {
        self.machine(e).request(0x14);
    }

    /// Message 4: harvest now; nothing is picked yet this harvest.
    ///
    /// Original: `FUN_140051260`.
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
        self.orchard(e)
            .map_or(0.0, |o| ripeness(o, &self.cells.objects, self.fruits))
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

    /// OrchardComponent's message slot. 0: slot `index` was planted (a tree with fruit stands, else
    /// it is lost); 1: the tree was picked (its fruit starts again); 2: a tending visit ripens its
    /// fruit by `ripening time × 300 s × tend`.
    ///
    /// Original: `FUN_1400522f0`.
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

    /// The orchard's buttons: 0 plant now (message 3, state 2), 1 harvest now (message 4, states 10
    /// and 12), 2 tending on/off, 3 clear (message 5, states 9–12).
    ///
    /// Original: `FUN_140052500`.
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
                self.enable_workplace(e, on);
            }
            3 if matches!(state, 9..=0xc) => self.orchard_clear_now(e),
            _ => {}
        }
    }

    // +0x58
    /// The orchard's tree list (as CropUI's slot): choosing the first tree switches tending on.
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
}

/// `Sim::orchard_ripeness` of an orchard: with no trees, 1.
pub fn ripeness(
    o: &Orchard,
    objects: &[Option<usize>],
    fruits: &std::collections::BTreeMap<usize, Fruit>,
) -> f32 {
    if o.trees.is_empty() {
        return 1.0;
    }
    let sum: f32 = o
        .trees
        .iter()
        .filter(|t| t.state == 1 && t.entity.is_some())
        .filter(|t| objects[t.cell as usize] == t.entity)
        .map(|t| {
            let x = t.entity.unwrap();
            fruits.get(&x).map_or(0.0, |f| f.ripeness.min(1.0))
        })
        .sum();
    sum / o.trees.len() as f32
}
