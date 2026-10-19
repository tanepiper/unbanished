//! Clearing an area (`ClearArea`): removing trees, rocks and other resources before building.

use super::*;

impl Sim<'_, '_> {
    /// The entity on a cell as the job code reads it: occupant (`+0x00`), else object (`+0x08`).
    pub(super) fn entity_on(&self, cell: i32) -> Option<usize> {
        self.cells.entity_at(cell as usize)
    }

    /// `FUN_14011ce30`'s registration: count the footprint's objects into a category mask
    /// (bit 0, plus logs/stone/iron from their raw materials), keep `+0x48` = `FUN_14011d800`
    /// and register a ClearArea work.
    pub(super) fn register_clear(&mut self, jobs: &mut Jobs, e: usize) {
        let site = &self.sites[&e];
        let Some(clear) = self.site_templates[site.template].clear.clone() else {
            return;
        };
        let n = self.size as i32;
        let (x, y) = self.placed[e].cell;
        let (w, h) = self.placed[e].footprint;
        let mut mask = 1u32;
        for cy in y..y + h {
            for cx in x..x + w {
                let Some(o) = self.cells.objects[(cy * n + cx) as usize] else {
                    continue;
                };
                let t = &self.templates[self.placed[o].template];
                if t.natural.is_none() {
                    continue;
                }
                if let Some(m) = t.pile.and_then(|p| self.templates[p].pile_material) {
                    mask |= clear_mask(self.materials[m].flags);
                }
            }
        }
        let total = self.clear_remaining(e);
        if let Some(site) = self.sites.get_mut(&e) {
            site.clear_total = total;
        }
        let work = Work::new(
            e,
            self.rect_cell(e),
            self.professions.laborer,
            WorkKind::Clear(ClearWork {
                filter: clear.reference.clone(),
                clear_resources: clear.clear_resources,
                clear_cells: clear.clear_cells,
                clear_piles: clear.clear_piles,
                ..Default::default()
            }),
        );
        self.set_work(jobs, e, work, mask, clear.keep_priority);
    }

    /// WorkComponent `+0x30` (most workers): the WorkPlaceComponent's, else `FUN_140133a90`
    /// since nothing else sets it.
    pub(crate) fn work_max(&self, e: usize) -> i32 {
        match self.workforce.workplaces.get(&e) {
            Some(w) => w.max(),
            None => self.sites.get(&e).map_or(0, |s| {
                self.site_templates[s.template].default_max_workers(self.placed[e].footprint)
            }),
        }
    }

    /// The footprint cells holding another object.
    ///
    /// Original: `FUN_14011d800`.
    pub fn clear_remaining(&self, e: usize) -> i32 {
        clear_remaining(&self.cells.objects, self.size as i32, &self.placed[e], e)
    }

    /// `FUN_1400eafe0`: drop resource claims whose citizen lost the task; a lost cell claim
    /// returns before anything is dropped.
    pub(super) fn clear_refresh(&mut self, work: &mut Work) {
        let id = work.id;
        let WorkKind::Clear(cl) = &mut work.kind else {
            return;
        };
        let lost: Vec<usize> = cl
            .resources
            .iter()
            .filter(|(_, &c)| self.characters[c].stack.holds_task(id).is_none())
            .map(|(&r, _)| r)
            .collect();
        if cl
            .cells
            .values()
            .any(|&c| self.characters[c].stack.holds_task(id).is_none())
        {
            return;
        }
        for r in lost {
            cl.resources.remove(&r);
        }
    }

    /// `FUN_1400eb3b0`: ClearArea's slot `+0x20`.
    pub(super) fn clear_try(&mut self, work: &mut Work, c: usize, time: i64) -> u8 {
        let e = work.entity;
        let max = self.work_max(e);
        let WorkKind::Clear(cl) = &work.kind else {
            return 0;
        };
        if max <= (cl.resources.len() + cl.cells.len()) as i32 {
            return 1;
        }
        if self
            .workforce
            .workplaces
            .get(&e)
            .is_some_and(|w| !w.enabled)
        {
            return 1;
        }
        // `FUN_1400a50d0`: the footprint's objects (and, clearing cells, its road cells with
        // nothing on them), then `FUN_1400a2c90`: squared distance to the citizen, CRT qsort.
        let n = self.size as i32;
        let (x, y) = self.placed[e].cell;
        let (w, h) = self.placed[e].footprint;
        let from = self.citizen_cell(c);
        let clear_cells = cl.clear_cells;
        let mut entries: Vec<(i32, i32, Option<usize>)> = Vec::new();
        for cy in y..y + h {
            for cx in x..x + w {
                let cell = cy * n + cx;
                let i = cell as usize;
                let (dy, dx) = (cell / n - from / n, cell % n - from % n);
                if let Some(o) = self.cells.objects[i] {
                    entries.push((cell, dy * dy + dx * dx, Some(o)));
                }
                if clear_cells {
                    let t = self.cells.types[i].0;
                    let road = t == 0
                        || t == 1
                        || self.cells.road_kind[i] != crate::work::road::NONE
                        || self.pending_roads.contains_key(&cell);
                    if road {
                        entries.push((cell, dy * dy + dx * dx, None));
                    }
                }
            }
        }
        crate::behaviour::ai::crt_qsort_by(&mut entries, |a, b| a.1 - b.1);
        let (resources, filter, clear_piles) =
            (cl.clear_resources, cl.filter.clone(), cl.clear_piles);
        if resources {
            for &(cell, _, o) in &entries {
                let Some(o) = o else { continue };
                let WorkKind::Clear(cl) = &work.kind else {
                    return 0;
                };
                if cl.resources.contains_key(&o) {
                    continue;
                }
                let t = &self.templates[self.placed[o].template];
                if let Some(f) = &filter {
                    if !t.reference.eq_ignore_ascii_case(&f.replace('/', "\\")) {
                        continue;
                    }
                }
                if t.natural.is_none() {
                    continue;
                }
                let Some(nb) = nearest_neighbour(self.size, &self.cells.types, cell, from, 0xe07)
                else {
                    continue;
                };
                if !self.reachable(nb, from) {
                    continue;
                }
                if !self.patience(work, c, nb, from, time) {
                    return 0;
                }
                if self.claimed.contains(&o) {
                    continue;
                }
                let point = self.ground_at(nb);
                self.removal_sequence(c, work, o, point, false);
                self.took_job(c, work);
                if let WorkKind::Clear(cl) = &mut work.kind {
                    cl.resources.insert(o, c);
                }
                return 2;
            }
        }
        // The road cells (`+0x71`): laid back to ground (`FUN_1400c54a0` with type 2, the
        // task's parameter the cell).
        if clear_cells {
            for &(cell, _, o) in &entries {
                if o.is_some() {
                    continue;
                }
                let WorkKind::Clear(cl) = &work.kind else {
                    return 0;
                };
                if cl.cells.contains_key(&cell) || !self.reachable(cell, from) {
                    continue;
                }
                let target = self.ground_at(cell);
                self.road_clear_sequence(c, work, target, cell);
                self.took_job(c, work);
                if let WorkKind::Clear(cl) = &mut work.kind {
                    cl.cells.insert(cell, c);
                }
                return 2;
            }
        }
        if clear_piles {
            // The piles' own Pickup works (`FUN_1400fdaf0` without a second try), in order.
            self.offers = entries
                .iter()
                .filter_map(|&(_, _, o)| o.and_then(|o| self.piles.get(&o)).map(|p| p.work))
                .filter(|&id| id >= 0)
                .collect();
            if !self.offers.is_empty() {
                return NESTED;
            }
        }
        1
    }

    /// `FUN_1400c7380`: walk next to a natural resource, work it with its tool, let it fall
    /// (resources with the animation flag), work it again, remove it and report.
    pub(crate) fn removal_sequence(
        &mut self,
        c: usize,
        work: &mut Work,
        resource: usize,
        point: [f32; 3],
        auto_pickup: bool,
    ) {
        let t = &self.templates[self.placed[resource].template];
        let Some(nr) = t.natural.clone() else {
            return;
        };
        let facing = crate::behaviour::character::work_facing(self.facing_centre(resource), point);
        self.clear_citizen(c, Some(work));
        self.on_empty_think(c);
        let tool = nr.value as i32;
        let clip = if (nr.value) < 11 { tool + 9 } else { 8 } as usize;
        self.push(
            c,
            Action::CompleteTask {
                work: work.id,
                param: 0,
            },
        );
        self.push(c, Action::ShowTool(-1));
        self.push(
            c,
            Action::RemoveResource {
                entity: resource,
                flag: auto_pickup,
            },
        );
        if nr.flag {
            self.push(c, self.perform(c, facing, clip, seconds(nr.time_b), false));
            self.push(
                c,
                Action::AnimateResource {
                    entity: resource,
                    anim: 1,
                    wait: false,
                },
            );
            self.push(
                c,
                Action::AnimateResource {
                    entity: resource,
                    anim: 0,
                    wait: true,
                },
            );
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
        self.claimed.insert(resource);
    }

    /// A removal sequence was aborted.
    ///
    /// Original: `FUN_1400cff70`.
    pub fn unclaim(&mut self, resource: usize) {
        self.claimed.remove(&resource);
    }

    /// A resource with the animation flag plays clip `anim`.
    ///
    /// Original: `FUN_14012b910`.
    pub fn animate_resource(&mut self, resource: usize, anim: usize) {
        let p = &mut self.placed[resource];
        let t = &self.templates[p.template];
        if t.natural.as_ref().is_some_and(|n| n.flag)
            && t.animations
                .get(p.variant)
                .and_then(Option::as_ref)
                .is_some_and(|g| g.clips.get(anim).is_some())
        {
            p.animation = Some(crate::behaviour::animation::Playback::new(anim, 0));
        }
    }

    /// The start of `RemoveNaturalResource`: queue the resource for removal; unless
    /// another profession should have harvested it, leave a pile of `yield × min(growth, 1)` units
    /// (the educated yield for educated citizens), turned `RandTol(2) × 90°`. `priority` is the
    /// priority of a work at the resource's cell.
    ///
    /// Original: `FUN_1400cffe0`, `FUN_1400fe220`, `FUN_14012b5c0`.
    pub fn remove_resource(
        &mut self,
        c: usize,
        resource: usize,
        educated: bool,
        priority: Option<i32>,
        auto_pickup: bool,
    ) {
        self.claimed.remove(&resource);
        if !self.placed[resource].alive {
            return;
        }
        let template = self.placed[resource].template;
        let Some(nr) = self.templates[template].natural.clone() else {
            return;
        };
        // `+0x2d`: as a crop field set it (else kept), and not kept when a related profession
        // other than the laborer should harvest it.
        let mut keep = self.keep_piles.remove(&resource).unwrap_or(true);
        if let Some(r) = nr.related.as_deref() {
            let r = r.replace('/', "\\");
            let related = self
                .professions
                .list
                .iter()
                .position(|p| p.reference.eq_ignore_ascii_case(&r));
            if let Some(p) = related {
                if p != self.professions.laborer && self.states[c].profession != p {
                    keep = false;
                }
            }
        }
        // `FUN_1400c8370`: the entity leaves at the end of the iteration (the port removes it
        // on the next animation pass).
        let growth = self.placed[resource].growth;
        let cell = self.placed[resource].cell;
        {
            let p = &mut self.placed[resource];
            p.animation = None;
            p.removal_requested = true;
        }
        *self.models_dirty = true;
        if !keep {
            return;
        }
        let Some(pile_t) = self.templates[template].pile else {
            return;
        };
        let pt = &self.templates[pile_t];
        let base = if educated {
            pt.pile_yield.1
        } else {
            pt.pile_yield.0
        };
        let amount = (base as f32 * growth.min(1.0)) as i32;
        if pt.pile_material.is_none() || amount <= 0 {
            return;
        }
        let yaw = self.rng.rand_tol(2) as f32 * 90.0;
        let pile = self.spawn_pile(pile_t, cell, amount, yaw, priority);
        // `FUN_14008e3c0`: made, for the citizen's workplace (`+0x248`) when the action's
        // `+0x30` is set.
        if let Some(m) = self.templates[pile_t].pile_material {
            let workplace = self.states[c].workplace;
            self.count_made(workplace, auto_pickup, m, amount);
        }
        // `FUN_140045170` once the pile's work is registered ([`Sim::activate_piles`]).
        if auto_pickup {
            if let (Some(wp), Some(p)) = (self.states[c].workplace, self.piles.get_mut(&pile)) {
                p.auto_pickup = Some(wp);
            }
        }
    }

    /// `FUN_14011d230`: every other entity on the footprint's cells that is not already going
    /// goes now (`FUN_1400c8370`): its work and its stamp are lifted at once and it is removed
    /// with the entities queued this iteration. A pile goes with its work.
    pub(super) fn clear_instantly(&mut self, jobs: &mut Jobs, e: usize) {
        let n = self.size as i32;
        let (x, y) = self.placed[e].cell;
        let (w, h) = self.placed[e].footprint;
        for cy in y..y + h {
            for cx in x..x + w {
                let Some(o) = self.cells.objects[(cy * n + cx) as usize] else {
                    continue;
                };
                if o == e || !self.placed[o].alive || self.placed[o].removal_requested {
                    continue;
                }
                if self.piles.contains_key(&o) {
                    self.destroy_pile(jobs, o);
                    continue;
                }
                if self.templates[self.placed[o].template].natural.is_some() {
                    // The natural resource's removal (`FUN_140125a30` → `FUN_1400a33b0`) frees
                    // its cell now; the growth map follows when the entity goes.
                    let (ox, oy) = self.placed[o].cell;
                    let i = (oy * n + ox) as usize;
                    if self.cells.objects[i] == Some(o) {
                        self.cells.objects[i] = None;
                        self.cells
                            .set_type(i, self.terrain.natural_cell_type(self.size, i));
                    }
                } else {
                    self.deactivate(jobs, o);
                }
                let p = &mut self.placed[o];
                p.animation = None;
                p.removal_requested = true;
                *self.models_dirty = true;
            }
        }
    }
}
