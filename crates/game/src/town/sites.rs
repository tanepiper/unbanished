//! Building sites and buildings: becoming a site or a building, marking them on the map, and destroying them.

use super::*;

impl Sim<'_, '_> {
    /// `FUN_1401350d0` on `e`'s workplace: its workers move on when it is switched off.
    pub(crate) fn enable_workplace(&mut self, e: usize, on: bool) {
        // The state machine's pending state, else its state (`+0x52`, `+0x50`): 0, 1 or 8
        // is being built.
        let building = self.sites.get(&e).is_some_and(|s| {
            let state = if s.machine.pending < 0 {
                s.machine.state
            } else {
                s.machine.pending
            } as u8;
            state & 0xf6 == 0 && state != 9
        });
        self.with_search(|w, workers, search| w.set_enabled(workers, e, on, building, search));
    }

    /// `FUN_1401318c0` on `e`'s StatusIconComponent (its workplace's).
    pub(crate) fn set_icon(&mut self, e: usize, bit: u8, on: bool) {
        if let Some(w) = self.workforce.workplaces.get_mut(&e) {
            w.set_icon(bit, on);
        }
    }

    /// `FUN_14011d910`: the footprint is clear for its ClearAreaDescription: no road where
    /// it clears cells (`FUN_1400a2d30`), then no natural resource of the filter's template
    /// (`FUN_1400a2e00`) or, clearing resources and piles, no object at all (`FUN_1400a2f90`).
    pub(crate) fn is_clear(&self, entity: usize) -> bool {
        let p = &self.placed[entity];
        let n = self.size as i32;
        let (x, y) = p.cell;
        let (w, h) = p.footprint;
        let cells =
            || (y..y + h).flat_map(move |cy| (x..x + w).map(move |cx| (cy * n + cx) as usize));
        let desc = self
            .sites
            .get(&entity)
            .and_then(|s| self.site_templates[s.template].clear.clone());
        // `FUN_1400a2d30` (clearing cells): no road, planned road or road plan.
        let roads_clear = !desc.as_ref().is_some_and(|d| d.clear_cells)
            || cells().all(|i| {
                let t = self.cells.types[i].0;
                t != 0
                    && t != 1
                    && self.cells.road_kind[i] == crate::work::road::NONE
                    && !self.pending_roads.contains_key(&(i as i32))
            });
        if !roads_clear {
            return false;
        }
        match desc {
            // `FUN_1400a2e00`: no natural resource (of the filter's template).
            Some(d) if d.clear_resources && !d.clear_piles => cells().all(|i| {
                self.cells.objects[i].is_none_or(|o| {
                    let t = &self.templates[self.placed[o].template];
                    t.natural.is_none()
                        || d.reference.as_deref().is_some_and(|f| {
                            !t.reference.eq_ignore_ascii_case(&f.replace('/', "\\"))
                        })
                })
            }),
            // `FUN_1400a2f90`: no object.
            _ => cells().all(|i| self.cells.objects[i].is_none()),
        }
    }

    /// [`crate::work::professions::Workforce::demolishing`] from the sites' state machines.
    pub(crate) fn refresh_demolishing(&mut self) {
        let ids: Vec<usize> = self.workforce.workplaces.keys().copied().collect();
        for e in ids {
            if self.sites.contains_key(&e) && self.is_demolishing(e) {
                self.workforce.demolishing.insert(e);
            } else {
                self.workforce.demolishing.remove(&e);
            }
        }
    }

    /// `FUN_1401349f0` (WorkPlaceComponent::BecomeSite): the workers leave and builders work
    /// here.
    pub(crate) fn become_site(&mut self, jobs: &mut Jobs, e: usize) {
        self.refresh_demolishing();
        let builder = self.professions.builder;
        let mut workers = self.workers();
        self.workforce.clear_workers(&mut workers, e);
        let id = self.sites[&e].work_id;
        if let Some(old) = self.workforce.workplaces.get(&e).and_then(|w| w.profession) {
            jobs.change_profession(id, old, builder);
        }
        if let Some(w) = self.workforce.workplaces.get_mut(&e) {
            w.profession = Some(builder);
            self.workforce.register(e, builder);
            let max = self.build_point_count(e) as i32;
            // Nobody to move: the workers were let go above.
            self.set_workers(&workers);
            self.with_search(|w, ws, search| w.set_max_workers(ws, e, max, search));
            workers = self.workers();
            // `FUN_1401350d0(1)`: the builders' profession is not its own, so no
            // workers icon.
            let w = self.workforce.workplaces.get_mut(&e).unwrap();
            w.enabled = true;
            w.set_icon(crate::work::professions::icon::WORK_DISABLED, false);
            w.set_icon(crate::work::professions::icon::BUILD_PAUSED, false);
        }
        self.set_workers(&workers);
    }

    /// `FUN_140134b40` (WorkPlaceComponent::BecomeBuilding): back to its own profession.
    pub(crate) fn become_building(&mut self, jobs: &mut Jobs, e: usize) {
        let builder = self.professions.builder;
        let mut workers = self.workers();
        let template = self.sites[&e].template;
        let id = self.sites[&e].work_id;
        if let Some(w) = self.workforce.workplaces.get(&e) {
            if w.profession == Some(builder) {
                self.workforce.unregister(e, builder);
                let desc = self.site_templates[template].profession;
                let footprint = self.placed[e].footprint;
                let max = self.site_templates[template].default_max_workers(footprint);
                if let Some(p) = desc {
                    jobs.change_profession(id, builder, p);
                }
                let w = self.workforce.workplaces.get_mut(&e).unwrap();
                w.max_workers = max;
                w.profession = desc;
                self.workforce.clear_workers(&mut workers, e);
            }
            if let Some(p) = self.workforce.workplaces[&e].profession {
                self.workforce.register(e, p);
            }
        }
        self.set_workers(&workers);
    }

    /// `FUN_140134130`: register a work for the site's WorkComponent, replacing any.
    pub(crate) fn set_work(
        &mut self,
        jobs: &mut Jobs,
        entity: usize,
        work: Work,
        mask: u32,
        keep: bool,
    ) {
        self.remove_work(jobs, entity);
        let keep = if keep {
            self.sites[&entity].work_priority
        } else {
            None
        };
        let id = jobs.add(work, mask, keep);
        let priority = jobs.works[&id].priority;
        let site = self.sites.get_mut(&entity).unwrap();
        site.work_id = id;
        site.work_priority = Some(priority);
    }

    /// `FUN_140134020`: keep the priority, remove the work; the limit and work icons go.
    pub(crate) fn remove_work(&mut self, jobs: &mut Jobs, entity: usize) {
        let id = self.sites[&entity].work_id;
        if let Some(w) = jobs.works.get(&id) {
            self.sites.get_mut(&entity).unwrap().work_priority = Some(w.priority);
            jobs.remove(id, self);
            self.set_icon(entity, crate::work::professions::icon::LIMIT, false);
            self.set_icon(entity, crate::work::professions::icon::NO_WORK, false);
        }
    }

    /// `ClearAreaComponent::Stamp` (`FUN_14011d740`): with the description's `+0x2c`, the
    /// terrain under the map rectangle and one cell beyond is flattened (`FUN_1400a35b0`;
    /// entity event 5 re-adds the decal, which follows the terrain here).
    pub(crate) fn clear_area_stamp(&mut self, entity: usize) {
        let Some(site) = self.sites.get(&entity) else {
            return;
        };
        if !self.site_templates[site.template]
            .clear
            .as_ref()
            .is_some_and(|c| c.stamp)
        {
            return;
        }
        let p = &self.placed[entity];
        let (x, y) = p.cell;
        let (w, h) = p.footprint;
        self.flatten_requests.push([x, y, x + w, y + h]);
    }

    /// QuarryComponent's message (`FUN_1400554e0`): the pit goes to the floor between the
    /// description's shallow and deep by the share of the ConsumeProduce lifetime produced,
    /// and the quarry claims its steep cells.
    pub(crate) fn quarry_message(&mut self, e: usize) {
        let Some(q) = &self.templates[self.placed[e].template].quarry else {
            return;
        };
        let Some(site) = self.sites.get(&e) else {
            return;
        };
        let produced = site.produce.as_ref().map_or(0, |p| p.produced);
        let lifetime = self.site_templates[site.template]
            .produce
            .as_ref()
            .map_or(1, |p| p.lifetime);
        let t = produced as f32 / lifetime as f32;
        let floor = (1.0 - t) * q.shallow + t * q.deep;
        self.terraform_requests
            .push(crate::world::terraform::Terraform::Quarry {
                entity: e,
                floor,
                claim: true,
            });
    }

    /// DecalComponent event 1 (`FUN_140120c30`): alpha 1.
    pub(crate) fn decal_opaque(&mut self, entity: usize) {
        let d = self.decals.entry(entity).or_default();
        if !d.opaque {
            d.opaque = true;
            *self.models_dirty = true;
        }
    }

    pub(super) fn set_mesh(&mut self, entity: usize, mesh: i32) {
        self.placed[entity].mesh = if mesh < 0 { usize::MAX } else { mesh as usize };
    }

    pub(super) fn meshes(&self, entity: usize) -> i32 {
        let site = &self.sites[&entity];
        let v = self.placed[entity].variant;
        self.site_templates[site.template]
            .meshes
            .get(v)
            .copied()
            .unwrap_or(1) as i32
    }

    /// `FUN_14010d4c0`: the materials built in count as used (`+0xc`); the building's own
    /// storage replaces the material store.
    pub(super) fn finish_storage(&mut self, entity: usize) {
        let site = &self.sites[&entity];
        let items: Vec<(usize, i32)> = self.storages[site.storage]
            .items
            .iter()
            .map(|i| (i.material, i.count))
            .collect();
        for (m, n) in items {
            self.count_used(m, n);
        }
        let site = &self.sites[&entity];
        let (w, h) = self.placed[entity].footprint;
        let s = &mut self.storages[site.storage];
        s.items.clear();
        if let Some((accepts, capacity, per_cell)) = self.site_templates[site.template].storage {
            s.accepts = accepts;
            s.capacity = capacity * if per_cell { w * h } else { 1 };
        } else {
            s.accepts = 0;
            s.capacity = 0;
        }
    }

    /// `FUN_1400a30f0` through the MapComponent (entity flag 1).
    pub(crate) fn stamp(&mut self, entity: usize) {
        let p = &self.placed[entity];
        let Some(map) = &self.templates[p.template].map else {
            return;
        };
        if map.no_stamp {
            return;
        }
        let rules = PlacementRules {
            mask: 0,
            masks: Vec::new(),
            mask_chars: String::new(),
            footprint: map.footprint.clone(),
            width: 0,
            height: 0,
            cell_type: map.cell_type as u8,
        };
        let rect = Rect {
            x0: p.cell.0,
            y0: p.cell.1,
            x1: p.cell.0 + p.footprint.0 - 1,
            y1: p.cell.1 + p.footprint.1 - 1,
        };
        let degrees = p.yaw as i32;
        crate::world::placement::stamp(self.cells, &rules, rect, degrees, entity);
    }

    /// `FUN_140129c20` → `FUN_1400a33b0`: the MapComponent's footprint is lifted. Stamped
    /// cells (`#`, `_`, or all with an empty footprint) get their natural type back (ground
    /// where water lies under another occupant), and every cell's object is cleared.
    pub(crate) fn unstamp(&mut self, entity: usize) {
        let p = &self.placed[entity];
        let Some(map) = &self.templates[p.template].map else {
            return;
        };
        let n = self.size as i32;
        let (x0, y0) = p.cell;
        let (w, h) = p.footprint;
        let fp = map.footprint.as_bytes();
        let degrees = p.yaw as i32;
        for y in y0..y0 + h {
            for x in x0..x0 + w {
                let i = (n * y + x) as usize;
                let stamped = fp.is_empty() || {
                    let c = fp
                        .get(crate::world::placement::footprint_index(
                            x - x0,
                            y - y0,
                            w,
                            h,
                            degrees,
                        ))
                        .copied()
                        .unwrap_or(b'.');
                    c == b'#' || c == b'_'
                };
                if stamped {
                    let mut ty = self.terrain.natural_cell_type(self.size, i);
                    let water = (1u32 << (ty.0 & 0x1f)) & 0x1e0 != 0;
                    if water && self.cells.occupants[i].is_some_and(|o| o != entity) {
                        ty = crate::terrain::CellType(2);
                    }
                    self.cells.set_type(i, ty);
                }
                self.cells.objects[i] = None;
            }
        }
    }

    /// Activate the sites placed since the last iteration (`FUN_1400c9d70` → activate).
    pub fn activate(&mut self, jobs: &mut Jobs, entity: usize) {
        self.trade_activate(entity);
        run(self, jobs, entity, false, false);
    }

    /// The sites' StateMachine updates: every site's bit-4 update with the iteration's time
    /// `dt`, then the scheduled update of each site its bucket visited this iteration
    /// (`visits`, in visiting order) with the time since its last visit (`FUN_1400c9560`).
    pub fn update_sites(&mut self, jobs: &mut Jobs, dt: i64, visits: &[(usize, i64)]) {
        let ids: Vec<usize> = self.sites.keys().copied().collect();
        self.refresh_demolishing();
        self.site_dt = dt;
        for e in &ids {
            run(self, jobs, *e, false, true);
        }
        for &(e, elapsed) in visits {
            if self.sites.get(&e).is_some_and(|s| !s.destroyed) {
                self.site_dt = elapsed;
                run(self, jobs, e, true, true);
            }
        }
        self.site_dt = dt;
        let destroyed: Vec<usize> = self
            .sites
            .iter()
            .filter(|(_, s)| s.destroyed)
            .map(|(&e, _)| e)
            .collect();
        for e in destroyed {
            self.destroy_entity(jobs, e);
        }
    }

    /// The entity manager's removal of a destroyed site entity (`FUN_1400c8370`, then each
    /// component's remove slot): its work goes (citizens doing it abort), the ZoneComponent
    /// frees its cells and the entity is gone.
    pub fn destroy_entity(&mut self, jobs: &mut Jobs, e: usize) {
        if !self.sites.contains_key(&e) {
            return;
        }
        self.remove_work(jobs, e);
        // The components' removal slots (`FUN_14004b380`, `FUN_14012ccb0`).
        self.happiness_leave(e);
        // A torn-down building's WorkPlace and Residence components (they left the town's
        // registries in state 3).
        if self.workforce.workplaces.contains_key(&e) {
            let mut workers = self.workers();
            self.workforce.clear_workers(&mut workers, e);
            self.set_workers(&workers);
            if let Some(p) = self.workforce.workplaces.get(&e).and_then(|w| w.profession) {
                self.workforce.unregister(e, p);
            }
            self.workforce.workplaces.remove(&e);
        }
        if self.homes.contains_key(&e) {
            self.residence_leave(e);
        }
        if self.wildlife.pastures.contains_key(&e) {
            self.pasture_leave(e);
            self.wildlife.pastures.remove(&e);
        }
        // MultiObjectComponent's (`FUN_14012ac90`): its parts go too, lifting their stamps and
        // their floors (`FUN_140124770`).
        for part in self.sites[&e].parts.clone() {
            self.unstamp(part);
            let size = self.size;
            self.cells.floors.remove(size, part);
            self.placed[part].alive = false;
        }
        // TerraformComponent removal (`FUN_140057cb0`): the entity's, then its parts'.
        let parts = self.sites[&e].parts.clone();
        for x in std::iter::once(e).chain(parts) {
            if self.templates[self.placed[x].template].terraform.is_some() {
                self.terraform_requests
                    .push(crate::world::terraform::Terraform::Undo(x));
            }
        }
        self.sites.remove(&e);
        self.decals.remove(&e);
        self.site_list.retain(|&s| s != e);
        let n = self.size as i32;
        let p = &mut self.placed[e];
        p.alive = false;
        let (x, y) = p.cell;
        let (w, h) = p.footprint;
        for cy in y..y + h {
            for cx in x..x + w {
                let i = (cy * n + cx) as usize;
                if self.cells.occupants[i] == Some(e) {
                    self.cells.occupants[i] = None;
                }
            }
        }
        *self.models_dirty = true;
    }
}
