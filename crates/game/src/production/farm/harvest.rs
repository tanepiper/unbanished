//! A crop field's harvest work and the area work's callbacks.

use super::*;

impl Sim<'_, '_> {
    /// The harvest work's slot `+0x20` (`FUN_1400f21a0`) after the AutoPickup offers.
    pub(crate) fn harvest_try(&mut self, work: &mut Work, c: usize) -> u8 {
        let e = work.entity;
        let group = self.site_templates[self.sites[&e].template]
            .crop_field
            .map_or(0, |d| d.group);
        let (count_s, limit) = self.stock[group.clamp(0, 20) as usize];
        self.set_icon(e, crate::work::professions::icon::LIMIT, limit <= count_s);
        if limit <= count_s {
            return 1;
        }
        let WorkKind::Harvest(hw) = &work.kind else {
            return 0;
        };
        if self.work_max(e) < hw.claims.len() as i32 {
            return 1;
        }
        let Some(field) = self.field(e).cloned() else {
            return 0;
        };
        let from = self.citizen_cell(c);
        let count = field.cells.len() as i32;
        for i in 0..count {
            let hw = match &work.kind {
                WorkKind::Harvest(hw) => hw,
                _ => return 0,
            };
            if !hw.need[i as usize] || hw.claims.contains_key(&i) {
                continue;
            }
            let j = i + 1;
            let mut pair = j < count && hw.need[j as usize] && !hw.claims.contains_key(&j);
            let Some(first) = self.cells.objects[field.cells[i as usize].cell as usize] else {
                if let WorkKind::Harvest(hw) = &mut work.kind {
                    hw.need[i as usize] = false;
                }
                continue;
            };
            let second = if pair {
                self.cells.objects[field.cells[j as usize].cell as usize]
            } else {
                None
            };
            if second.is_none() {
                pair = false;
            }
            let Some(nb) = nearest_neighbour(
                self.size,
                &self.cells.types,
                field.cells[i as usize].cell,
                from,
                0xe0f,
            ) else {
                continue;
            };
            if !self.reachable(from, nb) {
                continue;
            }
            if self.claimed.contains(&first) {
                continue;
            }
            if pair && second.is_some_and(|s| self.claimed.contains(&s)) {
                pair = false;
            }
            let high = if pair { j } else { count };
            let point = self.ground_at(nb);
            let mut crops = vec![first];
            if pair {
                crops.extend(second);
            }
            self.harvest_sequence(c, work, &crops, point, high << 16 | i);
            self.took_job(c, work);
            if let WorkKind::Harvest(hw) = &mut work.kind {
                hw.claims.insert(i, c);
                hw.need[i as usize] = false;
                if pair {
                    hw.claims.insert(j, c);
                    hw.need[j as usize] = false;
                }
            }
            return 2;
        }
        1
    }

    /// `FUN_1400c6a00`: the removal sequence over one or two crops (no falling animation),
    /// with the auto-pickup flag.
    pub(super) fn harvest_sequence(
        &mut self,
        c: usize,
        work: &mut Work,
        crops: &[usize],
        point: [f32; 3],
        param: i32,
    ) {
        let Some(nr) = self.templates[self.placed[crops[0]].template]
            .natural
            .clone()
        else {
            return;
        };
        let facing = crate::behaviour::character::work_facing(self.facing_centre(crops[0]), point);
        self.clear_citizen(c, Some(work));
        self.on_empty_think(c);
        let tool = nr.value as i32;
        let clip = if nr.value < 11 { tool + 9 } else { 8 } as usize;
        self.push(
            c,
            Action::CompleteTask {
                work: work.id,
                param,
            },
        );
        self.push(c, Action::ShowTool(-1));
        for &o in crops {
            self.push(
                c,
                Action::RemoveResource {
                    entity: o,
                    flag: true,
                },
            );
            self.claimed.insert(o);
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
    }

    /// PlantArea and harvest slot `+0x30` (`FUN_1400f9430` / `FUN_1400f20f0`): the cells are
    /// free again.
    pub(crate) fn area_complete(work: &mut Work, param: i32) {
        let (WorkKind::PlantArea(a) | WorkKind::Harvest(a)) = &mut work.kind else {
            return;
        };
        let (i, j) = (param & 0xffff, (param >> 16) & 0xffff);
        a.claims.remove(&i);
        if (j as usize) < a.need.len() {
            a.claims.remove(&j);
        }
    }

    /// Slot `+0x38` (`FUN_1400f9370` / `FUN_1400f2030`): the cells are to do again.
    pub(crate) fn area_fail(work: &mut Work, param: i32) {
        let (WorkKind::PlantArea(a) | WorkKind::Harvest(a)) = &mut work.kind else {
            return;
        };
        let (i, j) = (param & 0xffff, (param >> 16) & 0xffff);
        a.claims.remove(&i);
        if let Some(n) = a.need.get_mut(i as usize) {
            *n = true;
        }
        if (j as usize) < a.need.len() {
            a.need[j as usize] = true;
            a.claims.remove(&j);
        }
    }

    /// Slot `+0x48` (`FUN_1400f9140` / `FUN_1400f1e00`): cells whose worker lost the task are
    /// to do again.
    pub(crate) fn area_refresh(&mut self, work: &mut Work) {
        let id = work.id;
        let (WorkKind::PlantArea(a) | WorkKind::Harvest(a)) = &mut work.kind else {
            return;
        };
        let lost: Vec<i32> = a
            .claims
            .iter()
            .filter(|(_, &c)| self.characters[c].stack.holds_task(id).is_none())
            .map(|(&i, _)| i)
            .collect();
        for i in lost {
            a.claims.remove(&i);
            if let Some(n) = a.need.get_mut(i as usize) {
                *n = true;
            }
        }
    }

    /// The workers of an area work, for the cancel slot (`FUN_1400f9930` / `FUN_1400f2740`).
    pub(crate) fn area_workers(work: &Work) -> BTreeSet<usize> {
        match &work.kind {
            WorkKind::PlantArea(a) | WorkKind::Harvest(a) => a.claims.values().copied().collect(),
            _ => BTreeSet::new(),
        }
    }
}
