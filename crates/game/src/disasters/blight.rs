//! Blight: DiseaseComponent (vtable `0x14015f3b0`) on crop fields, orchards and pastures. See
//! docs/FARMING.md "Blight".
//!
//! In its season a field may catch it, more likely when a cell grew the same thing last
//! time. While sick it kills a crop, tree or animal now and then, and passes to fields nearby
//! growing the same thing, until it runs its course or nothing is left.

use banished_formats::game::DiseaseDescription;

use crate::nature::wildlife::HUNTED;
use crate::scheduler::ticks;
use crate::town::Sim;

/// `DAT_140178934`.
const PERIOD: f32 = 300.0;
/// Event type 12: a field or pasture caught a disease.
const CAUGHT: i32 = 0xc;
/// `Game +0xd0` bit 5: blight is off.
pub const NO_BLIGHT: u32 = 0x20;

/// DiseaseComponent: `+0x28` towards the next check, `+0x30` time left (sick while above
/// zero), `+0x38` towards the next death, `+0x40` towards the next spread, `+0x48` on a
/// pasture (setup `FUN_1400495b0`), `+0x50` what grew when it last ended.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Blight {
    pub check: i64,
    pub left: i64,
    pub kill: i64,
    pub spread: i64,
    pub pasture: bool,
    pub last: Option<usize>,
}

impl Sim<'_, '_> {
    fn blight_desc(&self, e: usize) -> Option<DiseaseDescription> {
        self.site_templates[self.sites.get(&e)?.template].disease
    }

    fn blight(&mut self, e: usize) -> Option<&mut Blight> {
        self.sites.get_mut(&e)?.disease.as_mut()
    }

    /// `FUN_1400a50d0(map, rect, out, 0)`: the objects on the entity's cells, by rows.
    fn objects_in(&self, e: usize) -> Vec<usize> {
        let n = self.size as i32;
        let (x, y) = self.placed[e].cell;
        let (w, h) = self.placed[e].footprint;
        let mut out = Vec::new();
        for cy in y..y + h {
            for cx in x..x + w {
                if let Some(o) = self.cells.objects[(cy * n + cx) as usize] {
                    out.push(o);
                }
            }
        }
        out
    }

    fn is_resource(&self, o: usize) -> bool {
        self.placed[o].alive && self.templates[self.placed[o].template].natural.is_some()
    }

    /// `FUN_1400488e0`: what grows there: the pasture's kind; else the first natural resource
    /// on its cells, or, with nothing on them, the field's crop or the orchard's tree.
    fn grown(&self, e: usize) -> Option<usize> {
        if let Some(p) = self.wildlife.pastures.get(&e) {
            return p.kind;
        }
        let objects = self.objects_in(e);
        if objects.is_empty() {
            let site = self.sites.get(&e)?;
            if let Some(f) = &site.crop_field {
                return f.crop;
            }
            return site.orchard.as_ref().and_then(|o| o.tree);
        }
        objects
            .into_iter()
            .find(|&o| self.is_resource(o))
            .map(|o| self.placed[o].template)
    }

    /// `FUN_140048b10`: nothing left to fall sick (an empty pasture, no natural resource on
    /// the cells).
    fn nothing_to_blight(&self, e: usize) -> bool {
        if let Some(p) = self.wildlife.pastures.get(&e) {
            return p.livestock.is_empty();
        }
        !self.objects_in(e).into_iter().any(|o| self.is_resource(o))
    }

    /// `FUN_140048d00`: a built field with something growing falls sick (event 12 at severity
    /// 3, with the toolbar name). Its particles follow `left` in the app (docs/PARTICLES.md).
    fn catch_blight(&mut self, e: usize) {
        if self.nothing_to_blight(e) {
            return;
        }
        // The state machine's pending state, else its state (`+0x52`, `+0x50`), past 1.
        let state = self.sites.get(&e).map_or(0, |s| {
            if s.machine.pending < 0 {
                s.machine.state
            } else {
                s.machine.pending
            }
        });
        if state <= 1 {
            return;
        }
        let Some(d) = self.blight_desc(e) else { return };
        self.site_events.push((CAUGHT, 3, e));
        if let Some(b) = self.blight(e) {
            b.left = ticks(d.duration * PERIOD);
            b.kill = 0;
            b.spread = 0;
        }
    }

    /// `FUN_140048c80`: it is over; what grows now is remembered.
    fn end_blight(&mut self, e: usize) {
        let last = self.grown(e);
        if let Some(b) = self.blight(e) {
            b.check = 0;
            b.left = 0;
            b.kill = 0;
            b.spread = 0;
            b.last = last;
        }
    }

    /// DiseaseComponent's scheduled update (`FUN_1400494f0`) of every site that has one.
    pub fn update_blight(&mut self, visits: &[(usize, i64)]) {
        for &(e, dt) in visits {
            if !self.sites.get(&e).is_some_and(|s| s.disease.is_some()) {
                continue;
            }
            let Some(d) = self.blight_desc(e) else {
                continue;
            };
            let Some(left) = self.blight(e).map(|b| b.left) else {
                continue;
            };
            if left < 1 {
                self.blight_chance(e, &d, dt);
                continue;
            }
            let b = self.blight(e).unwrap();
            b.left -= dt;
            b.kill += dt;
            b.spread += dt;
            if b.left < 0 || self.nothing_to_blight(e) {
                self.end_blight(e);
                continue;
            }
            let period = ticks(d.kill * PERIOD);
            while 0 < period && period < self.blight(e).map_or(0, |b| b.kill) {
                self.blight(e).unwrap().kill -= period;
                if self.blight(e).is_some_and(|b| b.pasture) {
                    self.blight_animal(e);
                } else {
                    self.blight_crop(e);
                }
            }
            let period = ticks(d.spread * PERIOD);
            while 0 < period && period < self.blight(e).map_or(0, |b| b.spread) {
                self.blight(e).unwrap().spread -= period;
                self.spread_blight(e, &d);
            }
        }
    }

    /// `FUN_140048f70`: in its months, a check every `check` × 300 s: odds of one in
    /// `12 / check × odds`, or `odds_same` when a cell last grew what grew here.
    /// Off with `Game +0xd0` bit 5 (no disasters, or the debug flag).
    fn blight_chance(&mut self, e: usize, d: &DiseaseDescription, dt: i64) {
        if self.game_flags & NO_BLIGHT != 0 {
            return;
        }
        let mut month = (self.calendar / 3600.0 * 12.0) as i32;
        if 11 < month {
            month = 0;
        }
        if month < d.months.0 || d.months.1 < month {
            return;
        }
        let period = ticks(d.check * PERIOD);
        let Some(b) = self.blight(e) else { return };
        b.check += dt;
        let last = b.last;
        while 0 < period && period < self.blight(e).map_or(0, |b| b.check) {
            self.blight(e).unwrap().check -= period;
            // `FUN_1400a5030`: a cell's last growth (`+0x10`) is what grew when it last ended.
            let n = self.size as i32;
            let (x, y) = self.placed[e].cell;
            let (w, h) = self.placed[e].footprint;
            let same = (x..x + w).any(|cx| {
                (y..y + h).any(|cy| self.cells.previous_template[(cy * n + cx) as usize] == last)
            });
            let odds = if same { d.odds_same } else { d.odds };
            let chance = 1.0 / ((12.0 / d.check) * odds);
            if self.rng.rand_float() < chance {
                self.catch_blight(e);
            }
        }
    }

    /// `FUN_1400486d0`: up to 50 draws for an animal no one has taken; it dies.
    fn blight_animal(&mut self, e: usize) {
        let Some(herd) = self.wildlife.pastures.get(&e).map(|p| p.livestock.clone()) else {
            return;
        };
        if herd.is_empty() {
            return;
        }
        for _ in 0..50 {
            let a = herd[self.rng.rand_below(herd.len() as u32) as usize];
            let Some(animal) = self.wildlife.animals.get(&a) else {
                continue;
            };
            if animal.flags & HUNTED == 0 {
                // Already going (entity flag `0x400`): nothing more.
                if !animal.dying {
                    self.wildlife.kill(a);
                }
                return;
            }
        }
    }

    /// `FUN_1400484a0`: up to 50 draws among the objects for a natural resource no one is
    /// removing; it withers (no growth, no pile, removal with clip 2 when it has one).
    fn blight_crop(&mut self, e: usize) {
        let objects = self.objects_in(e);
        if objects.is_empty() {
            return;
        }
        for _ in 0..50 {
            let o = objects[self.rng.rand_below(objects.len() as u32) as usize];
            if self.is_resource(o) && !self.claimed.contains(&o) {
                self.placed[o].growth = 0.0;
                self.keep_piles.insert(o, false);
                self.placed[o].removal_requested = true;
                self.animate_resource(o, 2);
                *self.models_dirty = true;
                return;
            }
        }
    }

    /// The spread: a random cell within `radius` of the centre; the field or pasture there,
    /// growing the same thing, falls sick.
    fn spread_blight(&mut self, e: usize, d: &DiseaseDescription) {
        let n = self.size as i32;
        let (x, y) = self.placed[e].cell;
        let (w, h) = self.placed[e].footprint;
        let centre = ((h >> 1) + y) * n + (w >> 1) + x;
        let cx = (centre % n + self.rng.rand_tol(d.radius)).max(0);
        let cy = (centre / n + self.rng.rand_tol(d.radius)).max(0);
        let (cx, cy) = (cx.min(n - 1), cy.min(n - 1));
        let i = (cy * n + cx) as usize;
        let Some(other) = self.cells.occupants[i].or(self.cells.objects[i]) else {
            return;
        };
        if other == e || self.sites.get(&other).is_none_or(|s| s.disease.is_none()) {
            return;
        }
        if self.grown(other) == self.grown(e) {
            self.catch_blight(other);
        }
    }
}
