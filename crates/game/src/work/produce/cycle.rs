//! A producer's work cycle: taking a job, resuming it, and taking each input.

use super::*;

impl Sim<'_, '_> {
    /// `FUN_1400ecd10`: ConsumeProduce slot `+0x20`, with the status icons 2 (limited) and 3
    /// (missing inputs).
    pub(crate) fn produce_try(&mut self, work: &mut Work, c: usize, _time: i64) -> u8 {
        let e = work.entity;
        let Some(t) = self.recipes(e).cloned() else {
            return 0;
        };
        let selected = self.produce_select(e);
        let Some(recipe) = t.recipes.get(selected).cloned() else {
            return 1;
        };
        let state = self.sites[&e].produce.clone().unwrap_or_default();
        // The stock limit of the recipe's group (all groups while choosing at random).
        let limited = |r: &Recipe, sim: &Sim| {
            let (count, limit) = sim.stock[r.group.clamp(0, 20) as usize];
            limit <= count
        };
        let is_limited = if state.selector < 0 {
            t.recipes.iter().any(|r| limited(r, self))
        } else {
            limited(&recipe, self)
        };
        self.set_icon(e, crate::work::professions::icon::LIMIT, is_limited);
        // `FUN_14011e870`: an exhausted workplace switches itself off.
        let alive = t.lifetime < 0 || state.produced < t.lifetime;
        if is_limited || !alive {
            if !alive {
                self.produce_disable(e);
            }
            self.set_icon(e, crate::work::professions::icon::NO_WORK, false);
            return 1;
        }
        let WorkKind::Produce(pw) = &work.kind else {
            return 0;
        };
        let slots = pw.workers.len() as u32;
        let start = self.rng.rand_below(slots) as usize;
        if slots < 1 {
            return 1;
        }
        let Some(slot) = (0..slots as usize)
            .map(|k| (start + k) % slots as usize)
            .find(|&k| pw.workers[k].is_none())
        else {
            return 1;
        };
        let store = self.sites[&e].storage;
        let citizen_cell = self.citizen_cell(c);
        // Goods the recipe does not use leave the building.
        let items: Vec<(usize, i32)> = self.storages[store]
            .items
            .iter()
            .map(|i| (i.material, i.count - i.reserved))
            .collect();
        for (material, mut amount) in items {
            if material == recipe.output_material
                || recipe.inputs.iter().any(|&(m, _)| m == material)
            {
                continue;
            }
            let size = self.materials[material].size.max(1);
            let carried = &self.states[c].carried;
            let free = carried.capacity - carried.used(self.materials);
            if free < amount * size {
                amount = free / size;
            }
            if amount == 0 {
                self.drop_off(c);
                return 2;
            }
            let Some(found) = self.find_storage_excluding(material, amount, true, work.cell, e)
            else {
                continue;
            };
            if !self.reachable(citizen_cell, work.cell) {
                return 1;
            }
            // A workplace that is itself a storage location (the tavern) hands the goods out
            // at its drop point nearest the storage found (`FUN_140132aa0`), the others at
            // their first use point.
            let source = if self.templates[self.placed[e].template].storage_location {
                let toward = self.cell_of(found.target);
                match self.storage_point(e, toward) {
                    Some(p) => p,
                    None => continue,
                }
            } else {
                self.use_point(e, 0).0
            };
            let target_entity = self.storage_sites[found.site].entity;
            self.haul(
                c,
                work,
                Haul {
                    target_entity,
                    target_storage: found.storage,
                    target: found.target,
                    source_storage: store,
                    source_entity: e,
                    source,
                    material,
                    amount,
                    param: slot as i32,
                    partial: false,
                    complete_on_pickup: true,
                    ..Default::default()
                },
            );
            return self.produce_taken(work, c, slot);
        }
        // Missing inputs: fetch them (empty-handed), else drop off what is carried.
        let mut all_inputs = true;
        for &(material, need) in &recipe.inputs {
            if self.storages[store].available(material) >= need {
                continue;
            }
            all_inputs = false;
            let Some(found) = self.find_storage_excluding(material, 1, false, citizen_cell, e)
            else {
                continue;
            };
            let source_storage = found.storage;
            if self.storages[source_storage].available(material) == 0 {
                continue;
            }
            let size = self.materials[material].size;
            let carried = &self.states[c].carried;
            let (capacity, used) = (carried.capacity, carried.used(self.materials));
            if size <= capacity - used && used < 1 {
                let (target, _) = self.use_point(e, slot);
                let source_entity = self.storage_sites[found.site].entity;
                self.haul(
                    c,
                    work,
                    Haul {
                        target_entity: e,
                        target_storage: store,
                        target,
                        source_storage,
                        source_entity,
                        source: found.target,
                        material,
                        amount: -1,
                        param: slot as i32,
                        partial: true,
                        complete_on_pickup: false,
                        ..Default::default()
                    },
                );
                return self.produce_taken(work, c, slot);
            }
            self.drop_off(c);
            return 2;
        }
        // The create points: piles gone free their point; the first free one takes output.
        let mut free = 0;
        let mut point = None;
        let n_points = match &work.kind {
            WorkKind::Produce(pw) => pw.points.len(),
            _ => 0,
        };
        for i in 0..n_points {
            let cell = self.create_cell(e, i);
            let empty = self.cells.objects[cell as usize].is_none();
            let WorkKind::Produce(pw) = &mut work.kind else {
                return 0;
            };
            if pw.points[i] == 2 && empty {
                pw.points[i] = 0;
            }
            if pw.points[i] == 0 {
                free += 1;
                point.get_or_insert((i, cell));
            }
        }
        let WorkKind::Produce(pw) = &mut work.kind else {
            return 0;
        };
        if free == n_points {
            pw.full = false;
        }
        let resume = ProduceResume {
            selected,
            slot,
            point,
            all_inputs,
        };
        if point.is_none() {
            pw.full = true;
        } else if !pw.full && all_inputs {
            return self.produce_cycle(work, c, resume);
        }
        // Haul the workplace's own output away first (`FUN_1400fdaf0` on each create point's
        // pile, then go on with the cycle).
        for i in 0..n_points {
            let cell = self.create_cell(e, i) as usize;
            let Some(o) = self.cells.objects[cell] else {
                continue;
            };
            if o == e {
                continue;
            }
            if let Some(pile) = self.piles.get(&o) {
                if pile.work >= 0 {
                    self.offers.push(pile.work);
                }
            }
        }
        self.produce_resume = Some(resume);
        crate::work::jobs::NESTED
    }

    /// The rest of `FUN_1400ecd10` once no output pile was taken.
    pub(crate) fn produce_resume(&mut self, work: &mut Work, c: usize) -> u8 {
        let Some(resume) = self.produce_resume.take() else {
            return 1;
        };
        if resume.point.is_none() {
            return 1;
        }
        self.produce_cycle(work, c, resume)
    }

    /// `LAB_1400edf97`: work a cycle at the use point when every input is present.
    pub(super) fn produce_cycle(&mut self, work: &mut Work, c: usize, resume: ProduceResume) -> u8 {
        let e = work.entity;
        let Some((point, out_cell)) = resume.point else {
            return 1;
        };
        // `WorkUnavailable` while inputs are missing.
        self.set_icon(
            e,
            crate::work::professions::icon::NO_WORK,
            !resume.all_inputs,
        );
        if !resume.all_inputs {
            return 1;
        }
        let (target, facing) = self.use_point(e, resume.slot);
        if !self.reachable(self.citizen_cell(c), self.cell_of(target)) {
            return 1;
        }
        let Some(t) = self.recipes(e).cloned() else {
            return 0;
        };
        let WorkKind::Produce(pw) = &mut work.kind else {
            return 0;
        };
        let produce = t.cycles <= pw.cycles;
        if produce {
            pw.cycles -= t.cycles;
            if let Some(state) = self.sites.get_mut(&e).and_then(|s| s.produce.as_mut()) {
                state.produced += 1;
            }
        }
        let mut recipe = t.recipes.get(resume.selected).cloned().unwrap_or_default();
        // `FUN_1400c4440` selects again for the inputs it consumes (a second draw when
        // choosing at random).
        recipe.inputs = if produce {
            let again = self.produce_select(e);
            t.recipes
                .get(again)
                .map(|r| r.inputs.clone())
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let param = (if produce { 0x8000 } else { 0 })
            | (((point as i32) | 0x100) << 8)
            | resume.slot as i32;
        self.produce_sequence(
            c,
            work,
            ProduceSequence {
                target,
                facing,
                recipe,
                produce,
                out_cell,
                work_type: t.work_type,
                duration: t.duration,
                param,
            },
        );
        if produce {
            if let WorkKind::Produce(pw) = &mut work.kind {
                pw.points[point] = 1;
            }
        }
        self.produce_taken(work, c, resume.slot)
    }

    /// The citizen writes of every assignment, and the worker slot.
    pub(super) fn produce_taken(&mut self, work: &mut Work, c: usize, slot: usize) -> u8 {
        self.took_job(c, work);
        if let WorkKind::Produce(pw) = &mut work.kind {
            pw.workers[slot] = Some(c);
        }
        2
    }
}
