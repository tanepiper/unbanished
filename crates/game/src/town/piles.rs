//! Loose piles of goods on the ground (`RawMaterialComponent`): making, joining, taking and ageing them.

use super::*;

impl Sim<'_, '_> {
    /// `FUN_1400ccd90`: `count` of `material` (already out of citizen `c`'s storage) as a
    /// loose pile on the nearest flat free ground around the citizen, turned `RandTol(2) ×
    /// 90°`; with no such cell within six, the goods are lost.
    pub(crate) fn drop_loose_pile(&mut self, c: usize, material: usize, count: i32) {
        let Some(at) = self.free_flat_cell(self.citizen_cell(c)) else {
            return;
        };
        // The material's own template (a material is its pile's template).
        let reference = &self.materials[material].reference;
        let Some(template) = self
            .templates
            .iter()
            .position(|t| &t.reference == reference && t.pile_material == Some(material))
            .or_else(|| {
                self.templates
                    .iter()
                    .position(|t| t.pile_material == Some(material))
            })
        else {
            return;
        };
        let yaw = self.rng.rand_tol(2) as f32 * 90.0;
        let n = self.size as i32;
        self.spawn_pile(template, (at % n, at / n), count, yaw, None);
        *self.models_dirty = true;
    }

    /// A RawMaterialComponent pile entity (`FUN_1400c8bf0` of a pile template, `FUN_14012ec30`
    /// places it, turned `yaw`): stamped on its cell and activated with the next
    /// [`Sim::activate_piles`].
    pub(crate) fn spawn_pile(
        &mut self,
        pile_t: usize,
        cell: (i32, i32),
        amount: i32,
        yaw: f32,
        priority: Option<i32>,
    ) -> usize {
        let Some(material) = self.templates[pile_t].pile_material else {
            return usize::MAX;
        };
        let pt = &self.templates[pile_t];
        let n = self.size as i32;
        let index = cell.1 * n + cell.0;
        let id = self.placed.len();
        let mut yaw = yaw as i32;
        while yaw < 0 {
            yaw += 360;
        }
        let (variant, mesh) = pt.model.as_ref().map_or((0, 0), |m| {
            (m.variant.max(0) as usize, m.mesh.max(0) as usize)
        });
        self.placed.push(Placed {
            template: pile_t,
            bucket: pt.bucket,
            cell,
            footprint: (1, 1),
            position: self.ground_at(index),
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
        // The pile's MapComponent stamps its cell on activation (`MapDescription +0x30`).
        self.stamp(id);
        self.piles.insert(
            id,
            Pile {
                material,
                count: amount,
                age: 0,
                work: -1,
                mask: pile_mask(self.materials[material].flags),
                priority,
                auto_pickup: None,
                state: 0,
                claimer: None,
            },
        );
        self.new_piles.push(id);
        id
    }

    /// RawMaterialComponent activation (`FUN_14012e6a0`): register each new pile's Pickup work.
    pub fn activate_piles(&mut self, jobs: &mut Jobs) {
        for id in std::mem::take(self.new_piles) {
            let Some(pile) = self.piles.get(&id) else {
                continue;
            };
            let (mask, priority, auto_pickup) = (pile.mask, pile.priority, pile.auto_pickup);
            // RawMaterialComponent's setup (`FUN_14012e790`): its material becomes known.
            self.discover(pile.material);
            let cell = self.rect_cell(id);
            let work = Work::new(
                id,
                cell,
                self.professions.laborer,
                WorkKind::Pickup(PickupWork),
            );
            let w = jobs.add(work, mask, priority);
            self.piles.get_mut(&id).unwrap().work = w;
            if let Some(wp) = auto_pickup {
                self.auto_pickup_add(wp, w);
            }
        }
    }

    /// Destroy a pile (picked up or decayed): its entity, its cell and its work.
    pub fn destroy_pile(&mut self, jobs: &mut Jobs, id: usize) {
        let Some(pile) = self.piles.remove(&id) else {
            return;
        };
        if pile.work >= 0 {
            jobs.remove(pile.work, self);
        }
        let p = &mut self.placed[id];
        p.alive = false;
        let (x, y) = p.cell;
        let i = (y as usize) * self.size + x as usize;
        if self.cells.objects[i] == Some(id) {
            self.cells.objects[i] = None;
            self.cells
                .set_type(i, self.terrain.natural_cell_type(self.size, i));
        }
        *self.models_dirty = true;
    }

    /// `FUN_14010ae20`: every pile on the footprint (by rows) that the site's storage can take
    /// goes into it.
    pub(crate) fn absorb_piles(&mut self, jobs: &mut Jobs, e: usize) {
        let Some(storage) = self.sites.get(&e).map(|s| s.storage) else {
            return;
        };
        let n = self.size as i32;
        let (x, y) = self.placed[e].cell;
        let (w, h) = self.placed[e].footprint;
        for cy in y..y + h {
            for cx in x..x + w {
                let Some(o) = self.cells.objects[(cy * n + cx) as usize] else {
                    continue;
                };
                let Some(pile) = self.piles.get(&o) else {
                    continue;
                };
                let (material, count) = (pile.material, pile.count);
                if self.storages[storage].fits(self.materials, material, count) {
                    self.storages[storage].add(material, count);
                    self.destroy_pile(jobs, o);
                }
            }
        }
    }

    /// PickupRawMaterial start (`FUN_1400ceda0`): the pile goes into the carried storage.
    pub fn take_pile(&mut self, jobs: &mut Jobs, c: usize, id: usize) {
        let Some(pile) = self.piles.get(&id) else {
            return;
        };
        let (material, count) = (pile.material, pile.count);
        self.states[c].carried.add(material, count);
        self.destroy_pile(jobs, id);
    }

    /// `FUN_14012e600` (RawMaterialComponent's scheduled update, for the piles visited this
    /// iteration): piles older than an hour disappear.
    pub fn age_piles(&mut self, jobs: &mut Jobs, visits: &[(usize, i64)]) {
        let old: Vec<usize> = visits
            .iter()
            .filter_map(|&(id, elapsed)| {
                let p = self.piles.get_mut(&id)?;
                p.age += elapsed;
                (p.age > ticks(3600.0)).then_some(id)
            })
            .collect();
        for id in old {
            self.destroy_pile(jobs, id);
        }
    }
}
