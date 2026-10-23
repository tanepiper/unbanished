//! Cemeteries and burial: CemeteryComponent and TombstoneComponent. See docs/CITIZEN-LIFE.md
//! "Burial".
//!
//! A cemetery lays out its graves on every other row inside its fence, clear of the gate. A citizen
//! who dies is buried in the nearest reachable cemetery with a free grave, which gets a tombstone
//! for five to seven years.
//!
//! Original: `FUN_140048100`, `0x140160aa0`.
// vtable 0x14015f1c0

use crate::scheduler::ticks;
use crate::terrain::CellType;
use crate::town::Sim;

/// `DAT_140178934`.
const PERIOD: f32 = 300.0;

// +0x20
/// CemeteryDescription (the tombstone template) with its TombstoneDescription: a grave lasts `(life
/// + RandFloat × range)` × 300 s.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CemeteryTemplate {
    /// The tombstone template.
    pub tombstone: usize,
    /// How long a grave lasts, in months of 300 seconds.
    pub life: f32,
    /// The random part of how long it lasts.
    pub range: f32,
}

// TombstoneDescription +0x20 life, +0x24 range; the result goes to TombstoneComponent +0x28.
/// How long a tombstone stays: `(RandFloat × range + life)` × 300 s. The original draws it in the
/// TombstoneComponent's setup, while the citizen dies.
///
/// Original: `FUN_1400581c0`.
pub fn tombstone_life(t: &CemeteryTemplate, rng: &mut crate::random::Random) -> i64 {
    ticks((rng.rand_float() * t.range + t.life) * PERIOD)
}

impl Sim<'_, '_> {
    /// Activation (`FUN_140047b60`): the cells inside the fence (one in from the map
    /// rectangle) on every other row along the gate's side, except the two lines in front of
    /// the gate (the first MultiObject part).
    fn lay_graves(&self, e: usize) -> Vec<i32> {
        let n = self.size as i32;
        let (x0, y0) = self.placed[e].cell;
        let (w, h) = self.placed[e].footprint;
        let yaw = self.placed[e].yaw as i32;
        let along = yaw == 0 || yaw == 180;
        let gate = self.sites.get(&e).and_then(|s| s.parts.first().copied());
        let off = gate.map_or(-10, |g| {
            let (gx, gy) = self.placed[g].cell;
            if along {
                gx - x0
            } else {
                gy - y0
            }
        });
        let (x1, y1, w2, h2) = (x0 + 1, y0 + 1, w - 2, h - 2);
        let mut graves = Vec::new();
        if along {
            for row in (0..h2).step_by(2) {
                for col in 0..w2 {
                    if col != off + 1 && col != off {
                        graves.push((row + y1) * n + col + x1);
                    }
                }
            }
        } else {
            for col in (0..w2).step_by(2) {
                for row in 0..h2 {
                    if row != off + 1 && row != off {
                        graves.push((row + y1) * n + col + x1);
                    }
                }
            }
        }
        graves
    }

    /// State 2 enter (`FUN_140047920`): join the town's cemeteries; a grave without a
    /// tombstone is plain ground.
    pub(crate) fn cemetery_enter2(&mut self, e: usize) {
        self.cemeteries.push(e);
        if self.sites.get(&e).is_some_and(|s| s.graves.is_empty()) {
            let graves = self.lay_graves(e);
            self.sites.get_mut(&e).unwrap().graves = graves;
        }
        for cell in self.sites[&e].graves.clone() {
            let i = cell as usize;
            if self.cells.objects[i].is_none_or(|o| !self.tombstones.contains_key(&o)) {
                self.cells.set_type(i, CellType(2));
                self.cells.objects[i] = None;
            }
        }
    }

    /// The burials decided this update: the grave becomes plain ground and gets a tombstone turned
    /// as the cemetery, which lasts the time drawn at the death and stamps its cell.
    ///
    /// Original: `FUN_1400473f0`.
    pub fn place_burials(&mut self) {
        for (cemetery, cell, life) in std::mem::take(self.burials) {
            let Some(t) = self
                .sites
                .get(&cemetery)
                .and_then(|s| self.site_templates[s.template].cemetery)
            else {
                continue;
            };
            let i = cell as usize;
            self.cells.set_type(i, CellType(2));
            self.cells.objects[i] = None;
            let n = self.size as i32;
            let template = &self.templates[t.tombstone];
            let (variant, mesh) = template.model.as_ref().map_or((0, 0), |m| {
                (m.variant.max(0) as usize, m.mesh.max(0) as usize)
            });
            let mut yaw = self.placed[cemetery].yaw as i32;
            while yaw < 0 {
                yaw += 360;
            }
            let id = self.placed.len();
            self.placed.push(crate::nature::seeding::Placed {
                template: t.tombstone,
                bucket: template.bucket,
                cell: (cell % n, cell / n),
                footprint: (1, 1),
                position: self.ground_at(cell),
                yaw: yaw as f32,
                variant,
                mesh,
                growth: 0.0,
                growth_state: None,
                spawn_state: None,
                alive: true,
                removal_requested: false,
                animation: None,
                scale: [1.0; 4],
            });
            self.tombstones.insert(id, life);
            self.stamp(id);
            *self.models_dirty = true;
        }
    }
}

/// TombstoneComponent's scheduled update: the time left runs down; at none the tombstone goes (its
/// cell is plain ground again). Returns whether it went.
///
/// Original: `FUN_140058130`.
pub fn tombstone_visit(
    tombstones: &mut std::collections::BTreeMap<usize, i64>,
    cells: &mut crate::nature::seeding::Cells,
    placed: &mut [crate::nature::seeding::Placed],
    id: usize,
    elapsed: i64,
) -> bool {
    let Some(left) = tombstones.get_mut(&id) else {
        return false;
    };
    *left -= elapsed;
    if 0 < *left {
        return false;
    }
    tombstones.remove(&id);
    let p = &mut placed[id];
    p.alive = false;
    let i = (p.cell.1 as usize) * cells.size + p.cell.0 as usize;
    if cells.objects[i] == Some(id) {
        cells.objects[i] = None;
        cells.set_type(i, CellType(2));
    }
    true
}
