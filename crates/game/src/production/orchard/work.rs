//! An orchard's work: planting trees, tending them and picking fruit.

use super::*;

impl Sim<'_, '_> {
    /// PlantOrchard slot `+0x20` (`FUN_1400f9fc0`): the first empty, unclaimed slot next to a
    /// reachable cell gets a tree.
    pub(crate) fn plant_orchard_try(&mut self, work: &mut Work, c: usize) -> u8 {
        let e = work.entity;
        self.orchard_refresh(e);
        let WorkKind::PlantOrchard(pa) = &work.kind else {
            return 0;
        };
        if self.work_max(e) < pa.claims.len() as i32 {
            return 1;
        }
        let Some(o) = self.orchard(e).cloned() else {
            return 0;
        };
        let from = self.citizen_cell(c);
        for (i, slot) in o.trees.iter().enumerate() {
            let WorkKind::PlantOrchard(pa) = &work.kind else {
                return 0;
            };
            if pa.need.get(i).copied().unwrap_or(true) || pa.claims.contains_key(&(i as i32)) {
                continue;
            }
            if slot.entity.is_some() {
                continue;
            }
            let Some(nb) = nearest_neighbour(self.size, &self.cells.types, slot.cell, from, 0xe0f)
            else {
                continue;
            };
            if !self.reachable(from, nb) {
                continue;
            }
            let Some(tree) = pa.crop else { return 1 };
            let point = self.ground_at(nb);
            let message = Message {
                component: Component::Orchard,
                message: 0,
                entity: Some(e),
                a: i as i32,
                b: 0,
            };
            self.plant_sequence(
                c,
                work,
                tree,
                point,
                slot.cell,
                Some(e),
                Some(message),
                i as i32,
            );
            self.took_job(c, work);
            if let WorkKind::PlantOrchard(pa) = &mut work.kind {
                pa.claims.insert(i as i32, c);
                pa.need[i] = true;
            }
            return 2;
        }
        1
    }

    /// PlantOrchard refresh (`FUN_1400f9cc0`): slots whose worker lost the task are free.
    pub(crate) fn plant_orchard_refresh(&mut self, work: &mut Work) {
        let id = work.id;
        let WorkKind::PlantOrchard(pa) = &mut work.kind else {
            return;
        };
        let lost: Vec<i32> = pa
            .claims
            .iter()
            .filter(|(_, &c)| self.characters[c].stack.holds_task(id).is_none())
            .map(|(&i, _)| i)
            .collect();
        for i in lost {
            pa.claims.remove(&i);
            if let Some(n) = pa.need.get_mut(i as usize) {
                *n = false;
            }
        }
    }

    /// PlantOrchard complete and fail (`FUN_1400f9f60`, `FUN_1400f9ef0`): the slot is free.
    pub(crate) fn plant_orchard_done(work: &mut Work, index: i32) {
        if let WorkKind::PlantOrchard(pa) = &mut work.kind {
            pa.claims.remove(&index);
            if let Some(n) = pa.need.get_mut(index as usize) {
                *n = false;
            }
        }
    }

    /// TendArea slot `+0x20` (`FUN_1400fa960`) for an orchard: a random tree.
    pub(crate) fn tend_orchard_try(&mut self, work: &mut Work, c: usize) -> u8 {
        let e = work.entity;
        let WorkKind::TendArea(t) = &work.kind else {
            return 0;
        };
        if self.work_max(e) < t.workers.len() as i32 {
            return 1;
        }
        let from = self.citizen_cell(c);
        let count = self.orchard(e).map_or(0, |o| o.trees.len()) as u32;
        let index = self.rng.rand_below(count) as i32;
        let Some(cell) = self
            .orchard(e)
            .and_then(|o| o.trees.get(index as usize))
            .map(|t| t.cell)
        else {
            return 0;
        };
        let Some(nb) = nearest_neighbour(self.size, &self.cells.types, cell, from, WALKABLE) else {
            return 0;
        };
        if !self.reachable(from, nb) {
            return 0;
        }
        let at = self.ground_at(cell);
        let stand = self.ground_at(nb);
        let message = Message {
            component: Component::Orchard,
            message: 2,
            entity: Some(e),
            a: index,
            b: 0,
        };
        self.tend_sequence(c, work, stand, at, message);
        self.took_job(c, work);
        if let WorkKind::TendArea(t) = &mut work.kind {
            t.workers.insert(c);
        }
        2
    }

    /// The picking work's slot `+0x20` (`FUN_1400f2d90`) after the AutoPickup offers.
    pub(crate) fn pick_fruit_try(&mut self, work: &mut Work, c: usize) -> u8 {
        let e = work.entity;
        let WorkKind::PickFruit(pf) = &work.kind else {
            return 0;
        };
        if self.work_max(e) < pf.claims.len() as i32 {
            return 1;
        }
        let group = self.orchard_desc(e).group;
        let (count, limit) = self.stock[group.clamp(0, 20) as usize];
        self.set_icon(e, crate::work::professions::icon::LIMIT, limit <= count);
        if limit <= count {
            return 1;
        }
        let Some(o) = self.orchard(e).cloned() else {
            return 0;
        };
        let from = self.citizen_cell(c);
        for (i, slot) in o.trees.iter().enumerate() {
            let WorkKind::PickFruit(pf) = &work.kind else {
                return 0;
            };
            if !pf.need[i] || pf.claims.contains_key(&(i as i32)) {
                continue;
            }
            let Some(tree) = self.cells.objects[slot.cell as usize] else {
                if let WorkKind::PickFruit(pf) = &mut work.kind {
                    pf.need[i] = false;
                }
                continue;
            };
            let Some(nb) = nearest_neighbour(self.size, &self.cells.types, slot.cell, from, 0xe0f)
            else {
                continue;
            };
            if !self.reachable(from, nb) {
                continue;
            }
            if self.templates[self.placed[tree].template].natural.is_none() {
                if let WorkKind::PickFruit(pf) = &mut work.kind {
                    pf.need[i] = false;
                }
                continue;
            }
            if self.claimed.contains(&tree) {
                continue;
            }
            let fd = self.templates[self.placed[tree].template]
                .fruit
                .unwrap_or_default();
            let amount = self.fruits.get(&tree).map_or(0.0, |f| f.ripeness);
            let point = self.ground_at(nb);
            self.pick_sequence(c, work, e, point, fd.pile, i % 2 == 0, amount, nb, i as i32);
            self.took_job(c, work);
            if let WorkKind::PickFruit(pf) = &mut work.kind {
                pf.claims.insert(i as i32, c);
                pf.need[i] = false;
            }
            return 2;
        }
        1
    }

    /// `FUN_1400c0c80`: walk to the tree, work 1 s, leave the fruit (scaled by its ripeness,
    /// every other tree only), tell the orchard, report.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn pick_sequence(
        &mut self,
        c: usize,
        work: &mut Work,
        orchard: usize,
        point: [f32; 3],
        fruit: Option<usize>,
        leave: bool,
        amount: f32,
        cell: i32,
        index: i32,
    ) {
        self.clear_citizen(c, Some(work));
        self.on_empty_think(c);
        self.push(
            c,
            Action::CompleteTask {
                work: work.id,
                param: index,
            },
        );
        self.push(
            c,
            Action::SendMessage(Message {
                component: Component::Orchard,
                message: 1,
                entity: Some(orchard),
                a: index,
                b: 0,
            }),
        );
        if leave {
            if let Some(t) = fruit {
                let batch = self.states[c].carried.capacity;
                self.push(
                    c,
                    Action::CreateRawMaterial {
                        template: t,
                        cell,
                        batch,
                        workplace: self.states[c].workplace,
                        factor: amount,
                    },
                );
            }
        }
        self.push(c, self.perform(c, -1000.0, 8, 1.0, true));
        let walk = MoveTo::new(point, 1, 0, WALKABLE, 0, -1.0, self.rng);
        self.push(c, Action::move_to(walk));
    }

    /// Picking complete and fail (`FUN_1400f2d50`, `FUN_1400f2ce0`).
    pub(crate) fn pick_done(work: &mut Work, index: i32, failed: bool) {
        if let WorkKind::PickFruit(pf) = &mut work.kind {
            pf.claims.remove(&index);
            if failed {
                if let Some(n) = pf.need.get_mut(index as usize) {
                    *n = true;
                }
            }
        }
    }
}
