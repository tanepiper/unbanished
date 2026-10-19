//! Taking goods to storage: picking up piles, finding a storage, dropping off, and emptying a storage (Pickup, ClearStorage).

use super::*;

impl Sim<'_, '_> {
    /// `FUN_1400f82f0`: a claimer that lost the task frees the pile.
    pub(super) fn pickup_refresh(&mut self, work: &mut Work) {
        let id = work.id;
        if let Some(p) = self.piles.get_mut(&work.entity) {
            if let Some(c) = p.claimer {
                if self.characters[c].stack.holds_task(id).is_none() {
                    p.claimer = None;
                    p.state = 0;
                }
            }
        }
    }

    /// `FUN_140092890`: does any registered storage take `count` of `material`?
    pub(super) fn any_storage_fits(&self, material: usize, count: i32) -> bool {
        self.storage_sites.iter().any(|s| {
            let st = &self.storages[s.storage];
            st.accepts & self.materials[material].flags != 0
                && st.fits(self.materials, material, count)
        })
    }

    /// `FUN_1400f83d0`: Pickup's slot `+0x20`. Takes this pile and every unclaimed pile within
    /// five cells that fits the same storage, nearest first, while they fit the carried
    /// capacity.
    pub(super) fn pickup_try(&mut self, work: &mut Work, c: usize, time: i64) -> u8 {
        let e = work.entity;
        let Some(pile) = self.piles.get(&e).cloned() else {
            return 1;
        };
        if pile.state != 0 {
            return 1;
        }
        let size = |m: usize, s: &Self| s.materials[m].size;
        let carried = &self.states[c].carried;
        let mut free = carried.capacity - carried.used(self.materials);
        if free < size(pile.material, self) * pile.count {
            self.drop_off(c);
            return 2;
        }
        let from = self.citizen_cell(c);
        let cell = work.cell;
        if !self.any_storage_fits(pile.material, pile.count) {
            return 1;
        }
        let Some(nb) = nearest_neighbour(self.size, &self.cells.types, cell, from, 0xe07) else {
            return 1;
        };
        if !self.reachable(nb, from) {
            return 1;
        }
        if !self.patience(work, c, nb, from, time) {
            return 0;
        }
        let found = self.find_storage(pile.material, pile.count, true, nb);
        let Some(found) = found else {
            return 1;
        };
        let first_rank = self.rank_or_distance(from, cell);
        let mut list: Vec<(i32, i32, i32, i32, usize)> = vec![(cell, nb, first_rank, work.id, e)];
        let n = self.size as i32;
        let (px, py) = (cell % n, cell / n);
        for dy in -5..=5 {
            for dx in -5..=5 {
                if dx == 0 && dy == 0 {
                    continue;
                }
                let (x, y) = (px + dx, py + dy);
                if x < 0 || y < 0 || x >= n || y >= n {
                    continue;
                }
                let k = y * n + x;
                let Some(o) = self.entity_on(k) else { continue };
                let Some(other) = self.piles.get(&o).cloned() else {
                    continue;
                };
                if !self.storages[found.storage].fits(self.materials, other.material, 1) {
                    continue;
                }
                let Some(nb2) = nearest_neighbour(self.size, &self.cells.types, k, from, 0xe07)
                else {
                    continue;
                };
                if !self.reachable(nb2, from) {
                    continue;
                }
                if other.work < 0 || other.state != 0 {
                    continue;
                }
                let r = self.rank_or_distance(from, k);
                let id = other.work;
                list.push((k, nb2, r, id, o));
            }
        }
        crate::behaviour::ai::crt_qsort_by(&mut list, |a, b| a.2 - b.2);
        let mut first = true;
        for (k, nb2, _, id, o) in list {
            let p = &self.piles[&o];
            let w = size(p.material, self) * p.count;
            if w > free {
                continue;
            }
            free -= w;
            let point = self.ground_at(nb2);
            self.pickup_sequence(
                c,
                if first { Some(&mut *work) } else { None },
                id,
                o,
                point,
                k,
            );
            first = false;
            let p = self.piles.get_mut(&o).unwrap();
            p.state = 1;
            p.claimer = Some(c);
        }
        self.took_job(c, work);
        if first {
            1
        } else {
            2
        }
    }

    /// `FUN_140092a60` from a cell (`put` finds a storage that takes the units).
    pub(crate) fn find_storage(
        &mut self,
        material: usize,
        count: i32,
        put: bool,
        cell: i32,
    ) -> Option<crate::work::stock::StorageMatch> {
        let town = TownView {
            size: self.size,
            types: &self.cells.types,
            regions: self.regions,
            occupants: &self.cells.occupants,
            objects: &self.cells.objects,
            floors: &self.cells.floors,
            entities: self.entities,
            happiness: self.happiness,
        };
        let (size, terrain) = (self.size, self.terrain);
        let floors = &self.cells.floors;
        let ground = move |cell: i32| crate::town::floor_ground(floors, size, terrain, cell);
        let mut geometry = StorageGeometry {
            town: &town,
            sectors: self.sectors,
            ground: &ground,
        };
        find_material_storage(
            self.storage_sites,
            self.storages,
            self.materials,
            material,
            count,
            put,
            cell,
            None,
            &mut geometry,
            self.rng,
        )
    }

    /// [`Sim::find_storage`] passing over `exclude`'s own storage.
    pub(crate) fn find_storage_excluding(
        &mut self,
        material: usize,
        count: i32,
        put: bool,
        cell: i32,
        exclude: usize,
    ) -> Option<crate::work::stock::StorageMatch> {
        let town = TownView {
            size: self.size,
            types: &self.cells.types,
            regions: self.regions,
            occupants: &self.cells.occupants,
            objects: &self.cells.objects,
            floors: &self.cells.floors,
            entities: self.entities,
            happiness: self.happiness,
        };
        let (size, terrain) = (self.size, self.terrain);
        let floors = &self.cells.floors;
        let ground = move |cell: i32| crate::town::floor_ground(floors, size, terrain, cell);
        let mut geometry = StorageGeometry {
            town: &town,
            sectors: self.sectors,
            ground: &ground,
        };
        find_material_storage(
            self.storage_sites,
            self.storages,
            self.materials,
            material,
            count,
            put,
            cell,
            Some(exclude),
            &mut geometry,
            self.rng,
        )
    }

    /// `FUN_1400c5fa0`: walk next to a pile, pick it up and report. The first pile's sequence
    /// clears the stack (`current` is the work being tried).
    pub(super) fn pickup_sequence(
        &mut self,
        c: usize,
        current: Option<&mut Work>,
        work: i32,
        pile: usize,
        point: [f32; 3],
        pile_cell: i32,
    ) {
        if let Some(w) = current {
            self.clear_citizen(c, Some(w));
        }
        let facing = crate::behaviour::character::work_facing(self.facing_centre(pile), point);
        let carrying = self.states[c].carried.used(self.materials) > 0;
        self.push(c, Action::ShowTool(-1));
        self.on_empty_think(c);
        self.push(c, self.perform(c, facing, 7, 0.75, false));
        if !carrying {
            self.push(c, Action::ShowTool(-2));
        }
        self.push(c, Action::PickupPile { entity: pile });
        self.push(c, Action::CompleteTask { work, param: 0 });
        self.push(c, self.perform(c, facing, 6, 0.75, false));
        let mask = WALKABLE | crate::pathfind::bit(self.cells.types[pile_cell as usize]);
        let m = MoveTo::new(
            point,
            if carrying { 4 } else { 1 },
            0,
            mask,
            0,
            -1.0,
            self.rng,
        );
        self.push(c, Action::move_to(m));
        if carrying {
            self.push(c, Action::ShowTool(-2));
        }
    }

    /// `FUN_14011b330`: carry everything to storages that take it: one drop-off per storage
    /// (nearest found per material, reusing a storage already chosen), then a final drop-off
    /// of whatever is left.
    pub fn drop_off(&mut self, c: usize) {
        let from = self.citizen_cell(c);
        let items: Vec<(usize, i32)> = self.states[c]
            .carried
            .items
            .iter()
            .filter(|i| i.count != 0)
            .map(|i| (i.material, i.count))
            .collect();
        let mut chosen: Vec<(usize, [f32; 3], usize)> = Vec::new();
        for (material, count) in items {
            if chosen
                .iter()
                .any(|&(s, _, _)| self.storages[s].fits(self.materials, material, count))
            {
                continue;
            }
            if let Some(f) = self.find_storage(material, count, true, from) {
                let entity = self.storage_sites[f.site].entity;
                chosen.push((f.storage, f.target, entity));
            }
        }
        if chosen.is_empty() {
            return;
        }
        self.clear_citizen(c, None);
        self.on_empty_think(c);
        // `Sequences::DropOffAt(.., redirect, first)`: the final drop-off has no storage, so
        // whatever is left goes on the ground.
        self.push(
            c,
            Action::DropOff {
                storage: usize::MAX,
                material: usize::MAX,
                count: -1,
                redirect: true,
            },
        );
        let mask = WALKABLE | crate::pathfind::bit(self.cells.types[from as usize]);
        for (storage, target, entity) in chosen {
            let facing =
                crate::behaviour::character::work_facing(self.facing_centre(entity), target);
            self.push(c, self.perform(c, facing, 7, 0.75, false));
            self.push(c, Action::ShowTool(-2));
            self.push(
                c,
                Action::DropOff {
                    storage,
                    material: usize::MAX,
                    count: -1,
                    redirect: false,
                },
            );
            self.push(c, self.perform(c, facing, 6, 0.75, false));
            let m = MoveTo::new(target, 4, 0, mask, 0, -1.0, self.rng);
            self.push(c, Action::move_to(m));
        }
        let s = &mut self.states[c];
        s.job = -1;
        s.health_events |= 4;
        s.supply_activity = 0;
    }
}

impl Sim<'_, '_> {
    /// The residence update's work (`FUN_14012ee40`): a house nobody lives in that still
    /// holds goods gets a ClearStorage work (mask 1); an empty storage has it removed.
    pub fn residence_clear_storage(&mut self, jobs: &mut Jobs, visits: &[(usize, i64)]) {
        for &(e, _) in visits {
            if !self.homes.contains_key(&e) {
                continue;
            }
            let Some(site) = self.sites.get(&e) else {
                continue;
            };
            let storage = site.storage;
            let registered = site.work_id >= 0 && jobs.works.contains_key(&site.work_id);
            let residents: usize = self.homes[&e].families.iter().map(Vec::len).sum();
            let used = self.storages[storage].used(self.materials);
            if residents == 0 && used != 0 && !registered {
                let work = Work::new(
                    e,
                    self.rect_cell(e),
                    self.professions.laborer,
                    WorkKind::ClearStorage(crate::production::herbalist::HerbalistWork::default()),
                );
                self.set_work(jobs, e, work, 1, false);
            } else if registered && used == 0 {
                self.remove_work(jobs, e);
            }
        }
    }

    /// ClearStorage's slot `+0x20` (`FUN_1400ec120`): the first item still unclaimed that a
    /// town storage takes, as much as the hauler can carry, is carried there from the house's
    /// first use point (a full hauler drops off first).
    pub(crate) fn clear_storage_try(&mut self, work: &mut Work, c: usize, time: i64) -> u8 {
        let e = work.entity;
        let Some(storage) = self.sites.get(&e).map(|s| s.storage) else {
            return 1;
        };
        let here = self.citizen_cell(c);
        let items: Vec<(usize, i32)> = self.storages[storage]
            .items
            .iter()
            .map(|i| (i.material, i.count - i.reserved))
            .collect();
        for (k, (material, left)) in items.into_iter().enumerate() {
            if left <= 0 {
                continue;
            }
            let mut amount = left;
            let size = self.materials[material].size.max(1);
            let carried = &self.states[c].carried;
            let free = carried.capacity - carried.used(self.materials);
            if free < size * amount {
                amount = free / size;
            }
            if amount == 0 {
                // The drop-off activity (`FUN_14011b330`).
                self.drop_off(c);
                return 2;
            }
            let Some(found) = self.find_storage_excluding(material, amount, true, work.cell, e)
            else {
                continue;
            };
            if !self.reachable(here, work.cell) {
                return 1;
            }
            // The house has no StorageLocation: its first use point.
            if self.use_point_count(e) == 0 {
                return 1;
            }
            let (source, _) = self.use_point(e, 0);
            let source_cell = self.cell_of(source);
            if !self.patience(work, c, source_cell, here, time) {
                return 0;
            }
            let target_entity = self.storage_sites[found.site].entity;
            self.haul(
                c,
                work,
                Haul {
                    target_entity,
                    target_storage: found.storage,
                    target: found.target,
                    source_storage: storage,
                    source_entity: e,
                    source,
                    material,
                    amount,
                    param: (amount << 8) | k as i32,
                    partial: false,
                    complete_on_pickup: true,
                    ..Default::default()
                },
            );
            self.took_job(c, work);
            if let WorkKind::ClearStorage(h) = &mut work.kind {
                h.workers.insert(c);
            }
            return 2;
        }
        1
    }
}
