//! Construction work: carrying materials to a site and building it (GatherRawMaterials, Build).

use super::*;

impl Sim<'_, '_> {
    /// `FUN_1400c4c60`: fetch `amount` of `material` from a storage and deliver it to `site`.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn haul_sequence(
        &mut self,
        c: usize,
        work: &mut Work,
        site: usize,
        target: [f32; 3],
        source_storage: usize,
        source_entity: usize,
        source: [f32; 3],
        material: usize,
        amount: i32,
        param: i32,
    ) {
        let site_storage = self.sites[&site].storage;
        self.haul(
            c,
            work,
            Haul {
                target_entity: site,
                target_storage: site_storage,
                target,
                source_storage,
                source_entity,
                source,
                material,
                amount,
                param,
                partial: false,
                complete_on_pickup: false,
                ..Default::default()
            },
        );
    }

    /// `FUN_1400c4c60` with its flags: `partial` (`param_11`, the pickup may take less),
    /// `complete_on_pickup` (`param_12`, the CompleteTask follows the pickup rather than the
    /// delivery) and `tool` (`param_14`, tool use and the faster loaded walk). `param_13` (a
    /// Citizen message first, `FUN_1400d06a0`) is set only by the home fetches
    /// (`FUN_140111160`, `FUN_1401108b0`), which `crate::people::needs` builds with the message itself.
    pub(crate) fn haul(&mut self, c: usize, work: &mut Work, h: Haul) {
        let Haul {
            target_entity: site,
            target_storage: site_storage,
            target,
            source_storage,
            source_entity,
            source,
            material,
            amount,
            param,
            partial,
            complete_on_pickup,
            tool,
            items,
        } = h;
        self.clear_citizen(c, Some(work));
        self.on_empty_think(c);
        let face_target =
            crate::behaviour::character::work_facing(self.facing_centre(site), target);
        let face_source =
            crate::behaviour::character::work_facing(self.facing_centre(source_entity), source);
        self.push(c, self.perform(c, face_target, 7, 0.75, tool));
        self.push(c, Action::ShowTool(-1));
        if !complete_on_pickup {
            self.push(
                c,
                Action::CompleteTask {
                    work: work.id,
                    param,
                },
            );
        }
        let several = !items.is_empty();
        self.push(
            c,
            Action::DropOff {
                storage: site_storage,
                material: if several { usize::MAX } else { material },
                count: if several { -1 } else { amount },
                redirect: true,
            },
        );
        self.push(c, self.perform(c, face_target, 6, 0.75, tool));
        let walk = MoveTo::new(
            target,
            4 + usize::from(tool),
            0,
            WALKABLE,
            0,
            -1.0,
            self.rng,
        );
        self.push(c, Action::move_to(walk));
        self.push(c, self.perform(c, face_source, 7, 0.75, tool));
        self.push(c, Action::ShowTool(-2));
        // `FUN_1400cf9f0` reserves at construction; without a count, as much as the citizen
        // carries, capped at the unreserved stock.
        let pickup = if several {
            for &(m, n) in &items {
                self.storages[source_storage].reserve(m, n);
            }
            items
        } else {
            let amount = if amount < 0 {
                let carried = &self.states[c].carried;
                (carried.capacity / self.materials[material].size.max(1))
                    .min(self.storages[source_storage].available(material))
            } else {
                amount
            };
            self.storages[source_storage].reserve(material, amount);
            vec![(material, amount)]
        };
        self.push(
            c,
            Action::PickupStoredRawMaterial {
                storage: source_storage,
                items: pickup,
                partial,
            },
        );
        if complete_on_pickup {
            self.push(
                c,
                Action::CompleteTask {
                    work: work.id,
                    param,
                },
            );
        }
        self.push(c, self.perform(c, face_source, 6, 0.75, tool));
        let walk = MoveTo::new(source, 1, 0, WALKABLE, 0, -1.0, self.rng);
        self.push(c, Action::move_to(walk));
    }

    /// `FUN_1400c5b60`: walk to a build point and work there once.
    pub(super) fn build_sequence(
        &mut self,
        c: usize,
        work: &mut Work,
        site: usize,
        point: [f32; 3],
        index: i32,
    ) {
        let facing = crate::behaviour::character::work_facing(self.facing_centre(site), point);
        self.clear_citizen(c, Some(work));
        self.on_empty_think(c);
        let r = self.rng.rand_below(2);
        self.push(
            c,
            Action::CompleteTask {
                work: work.id,
                param: index,
            },
        );
        // A destroy work messages the DestroyComponent instead.
        let destroy = matches!(&work.kind, WorkKind::Build(b) if b.destroy);
        self.push(
            c,
            Action::SendMessage(Message {
                component: if destroy {
                    Component::Destroy
                } else {
                    Component::Build
                },
                message: 0,
                entity: Some(site),
                a: 0,
                b: 0,
            }),
        );
        self.push(c, Action::ShowTool(-1));
        self.push(
            c,
            self.perform(c, facing, if r != 0 { 13 } else { 16 }, 3.0, true),
        );
        let walk = MoveTo::new(point, 2, 0, WALKABLE, 0, -1.0, self.rng);
        self.push(c, Action::move_to(walk));
        self.push(c, Action::ShowTool(if r != 0 { 4 } else { 7 }));
    }

    /// `FUN_1400fc3c0`: may this citizen take work this far away?
    pub(crate) fn patience(
        &mut self,
        work: &mut Work,
        c: usize,
        a: i32,
        b: i32,
        time: i64,
    ) -> bool {
        work.patience = work
            .patience
            .wrapping_add(time.wrapping_sub(work.patience_base));
        if self.states[c].profession == self.professions.laborer {
            return true;
        }
        if self.states[c].workplace.is_none() {
            return true;
        }
        let rank = self.rank_raw(a, b);
        let r = self.rules;
        let limit = ticks(r.work_minutes as f32 * 300.0);
        if ((self.states.len() as i32) / 2) < work.far_count && work.patience < limit {
            work.patience = work.patience.wrapping_mul(2);
        }
        if work.patience < 0 || limit < work.patience {
            work.patience = limit;
        }
        let v = (((seconds(work.patience) / seconds(time)) / (r.work_minutes * 300) as f32)
            * (r.work_far - r.work_near) as f32) as i32
            + r.work_near;
        if v < rank {
            work.far_count += 1;
            false
        } else {
            rank <= r.work_ok || r.work_cold <= self.temperature
        }
    }

    /// `FUN_14010db00`: the site storage holds every requirement.
    pub(super) fn has_materials(&self, entity: usize) -> bool {
        let site = &self.sites[&entity];
        let t = &self.site_templates[site.template];
        t.materials
            .iter()
            .all(|&(n, m)| self.storages[site.storage].available(m) >= n * site.stages)
    }
}

impl Sim<'_, '_> {
    /// `FUN_1400f1310`: GatherRawMaterials' slot `+0x20`.
    pub(super) fn gather_try(&mut self, work: &mut Work, c: usize, time: i64) -> u8 {
        let e = work.entity;
        if !self.workforce.workplaces.get(&e).is_some_and(|w| w.enabled) {
            return 1;
        }
        let cell = self.citizen_cell(c);
        let site = self.sites[&e].clone();
        let materials = self.site_templates[site.template].materials.clone();
        for (i, &(count, material)) in materials.iter().enumerate() {
            let need = count * site.stages;
            let WorkKind::Gather(g) = &work.kind else {
                return 0;
            };
            if g.transit[i] + g.stored[i] >= need {
                continue;
            }
            let found = {
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
                let ground =
                    move |cell: i32| crate::town::floor_ground(floors, size, terrain, cell);
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
                    1,
                    false,
                    cell,
                    None,
                    &mut geometry,
                    self.rng,
                )
            };
            let Some(found) = found else { continue };
            let mut amount = need - g.transit[i] - g.stored[i];
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
            amount = amount.min(self.storages[found.storage].available(material));
            // `FUN_1401280b0`: the nearest reachable build point to the source.
            let source_cell = self.cell_of(found.target);
            let mut best = 0x7fffffff;
            let mut target = None;
            for k in 0..self.build_point_count(e) {
                let p = self.build_point(e, k);
                let pc = self.cell_of(p);
                if self.reachable(source_cell, pc) {
                    let r = self.rank_or_distance(source_cell, pc);
                    if r < best {
                        best = r;
                        target = Some(p);
                    }
                }
            }
            let Some(target) = target.filter(|_| best < 0xffff) else {
                return 1;
            };
            let target_cell = self.cell_of(target);
            if !self.patience(work, c, target_cell, cell, time) {
                return 0;
            }
            let source_entity = self.storage_sites[found.site].entity;
            self.haul_sequence(
                c,
                work,
                e,
                target,
                found.storage,
                source_entity,
                found.target,
                material,
                amount,
                (amount << 8) | i as i32,
            );
            self.took_job(c, work);
            if let WorkKind::Gather(g) = &mut work.kind {
                g.workers.insert(c);
                g.transit[i] += amount;
            }
            return 2;
        }
        1
    }

    /// `FUN_1400ea490`: BuildStructure's slot `+0x20`.
    pub(super) fn build_try(&mut self, work: &mut Work, c: usize) -> u8 {
        let e = work.entity;
        let Some(wp) = self.workforce.workplaces.get(&e) else {
            return 1;
        };
        if !wp.enabled {
            return 1;
        }
        let points = self.build_point_count(e) as i32;
        let WorkKind::Build(b) = &work.kind else {
            return 0;
        };
        if points <= b.workers.len() as i32 {
            return 1;
        }
        if wp.workers.len() as i32 == wp.max() && !wp.workers.contains(&c) {
            return 1;
        }
        let free = b.occupied.iter().filter(|&&o| !o).count();
        let cell = self.citizen_cell(c);
        let mut chosen = -1i32;
        for _ in 0..free {
            let WorkKind::Build(b) = &mut work.kind else {
                return 0;
            };
            let n = b.occupied.len() as i32;
            if n > 0 {
                let mut tries = 0;
                loop {
                    let k = b.cursor;
                    if !b.occupied[k as usize] {
                        chosen = k;
                    }
                    b.cursor = if k + 1 >= n { 0 } else { k + 1 };
                    tries += 1;
                    if chosen >= 0 || tries >= n {
                        break;
                    }
                }
            }
            let point = self.build_point(e, chosen.max(0) as usize);
            let pc = self.cell_of(point);
            if pc != 0 && self.reachable(cell, pc) {
                // Every other point's connectivity and node distance only fill the sector
                // distance cache in the original.
                let _ = self.rank_raw(cell, pc);
                for k in 0..points {
                    if k == chosen {
                        continue;
                    }
                    let other = self.cell_of(self.build_point(e, k as usize));
                    if self.reachable(cell, other) {
                        let _ = self.rank_raw(cell, other);
                    }
                }
                self.build_sequence(c, work, e, point, chosen);
                self.took_job(c, work);
                if let WorkKind::Build(b) = &mut work.kind {
                    b.workers.insert(c);
                    b.occupied[chosen as usize] = true;
                }
                return 2;
            }
        }
        1
    }
}
