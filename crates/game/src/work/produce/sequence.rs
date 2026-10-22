//! A producer's action sequence, the raw materials it makes, and the work callbacks.

use super::*;

impl Sim<'_, '_> {
    /// `FUN_1400c4440`: walk to the use point (with the work's tool), consume the inputs,
    /// work, then leave the output and report; a produced batch first sends the message to
    /// the building's component of the description's type `+0x38` (the quarry's dig).
    pub(super) fn produce_sequence(&mut self, c: usize, work: &mut Work, s: ProduceSequence) {
        self.clear_citizen(c, Some(work));
        self.on_empty_think(c);
        let store = self.sites[&work.entity].storage;
        self.push(
            c,
            Action::CompleteTask {
                work: work.id,
                param: s.param,
            },
        );
        if s.produce {
            let batch = self.states[c].carried.capacity;
            self.push(
                c,
                Action::CreateRawMaterial {
                    template: s.recipe.output,
                    cell: s.out_cell,
                    batch,
                    workplace: Some(work.entity),
                    factor: 1.0,
                },
            );
            if self.templates[self.placed[work.entity].template]
                .quarry
                .is_some()
            {
                self.push(
                    c,
                    Action::SendMessage(crate::behaviour::character::Message {
                        component: crate::behaviour::character::Component::Quarry,
                        message: 0,
                        entity: Some(work.entity),
                        a: 0,
                        b: 0,
                    }),
                );
            }
        }
        self.push(c, Action::ShowTool(-1));
        let anim = if s.work_type < 0xb {
            s.work_type as usize + 9
        } else {
            8
        };
        self.push(c, self.perform(c, s.facing, anim, s.duration, true));
        if s.produce && !s.recipe.inputs.is_empty() {
            self.push(
                c,
                Action::ConsumeStoredRawMaterial {
                    storage: store,
                    items: s.recipe.inputs.clone(),
                    reserved: false,
                },
            );
        }
        let walk = match s.work_type {
            0 | 1 | 4..=7 => 2,
            2 | 3 => 3,
            _ => 1,
        };
        let m = MoveTo::new(s.target, walk, 0, WALKABLE, 0, -1.0, self.rng);
        self.push(c, Action::move_to(m));
        self.push(c, Action::ShowTool(s.work_type as i32));
    }

    /// CreateRawMaterial's start (`FUN_1400cc010`): the output's yield (`+0x40`, or `+0x44`
    /// for an educated worker) in piles of at most the worker's capacity, on the create
    /// point's cell or, once it holds something, the nearest free ground (`FUN_1400a1210`,
    /// mask 4), each turned `RandTol(2) × 90°`, each counted made for the workplace
    /// (`FUN_14008e3c0`, `+0x51` always set), the pile given to its AutoPickupComponent.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn create_raw_material(
        &mut self,
        jobs: &mut Jobs,
        c: usize,
        template: usize,
        cell: i32,
        educated: bool,
        factor: f32,
        workplace: Option<usize>,
    ) {
        let (material, yields) = {
            let pt = &self.templates[template];
            (pt.pile_material, pt.pile_yield)
        };
        let Some(material) = material else { return };
        let base = if educated { yields.1 } else { yields.0 };
        let mut remaining = (base as f32 * factor) as i32;
        let per = (self.states[c].carried.capacity / self.materials[material].size.max(1)).max(1);
        let mut search = false;
        while remaining > 0 {
            let n = remaining.min(per);
            if !search && self.cells.objects[cell as usize].is_some() {
                search = true;
            }
            let target = if search {
                self.free_cell(cell, None, 4)
            } else {
                Some(cell)
            };
            if let Some(at) = target {
                let size = self.size as i32;
                let yaw = self.rng.rand_tol(2) as f32 * 90.0;
                let pile = self.spawn_pile(template, (at % size, at / size), n, yaw, None);
                self.count_made(workplace, true, material, n);
                // A workplace with an AutoPickupComponent (the hunter's lodge, orchards,
                // pastures) takes the pile's work (`FUN_140045170`, once it is registered).
                if let Some(wp) = workplace
                    .filter(|wp| self.sites.get(wp).is_some_and(|s| s.auto_pickup.is_some()))
                {
                    if let Some(p) = self.piles.get_mut(&pile) {
                        p.auto_pickup = Some(wp);
                    }
                }
            }
            remaining -= n;
        }
        self.activate_piles(jobs);
    }

    /// `FUN_1400eca00`: ConsumeProduce slot `+0x48`. Workers that lost the task free their
    /// slot; then create points under way (1) that no remaining worker holds (by
    /// `(param >> 4) & 0xf`, as the original reads it) are taken as holding output (2).
    pub(crate) fn produce_refresh(&mut self, work: &mut Work) {
        let id = work.id;
        let WorkKind::Produce(pw) = &mut work.kind else {
            return;
        };
        let mut lost = false;
        let mut held = Vec::new();
        for w in pw.workers.iter_mut() {
            let Some(c) = *w else { continue };
            match self.characters[c].stack.holds_task(id) {
                None => {
                    *w = None;
                    lost = true;
                }
                Some(param) => held.push((param >> 4) & 0xf),
            }
        }
        if lost {
            for (i, p) in pw.points.iter_mut().enumerate() {
                if !held.contains(&(i as i32)) && *p == 1 {
                    *p = 2;
                }
            }
        }
    }

    /// `FUN_1400ecc90` (complete) and `FUN_1400ecc50` (fail, without the cycle bit).
    pub(crate) fn produce_complete(work: &mut Work, param: i32, failed: bool) {
        let WorkKind::Produce(pw) = &mut work.kind else {
            return;
        };
        if let Some(w) = pw.workers.get_mut((param & 0x7f) as usize) {
            *w = None;
        }
        if param >> 15 & 1 != 0 {
            if let Some(p) = pw.points.get_mut(((param >> 8) & 0x7f) as usize) {
                *p = 2;
            }
        }
        if !failed && param & 0x10000 != 0 {
            pw.cycles += 1;
        }
    }

    /// `FUN_1400ee400`: every worker's stack is cleared.
    pub(crate) fn produce_cancel(&mut self, work: &mut Work) {
        let WorkKind::Produce(pw) = &work.kind else {
            return;
        };
        for c in pw.workers.clone().into_iter().flatten() {
            self.clear_citizen(c, Some(work));
        }
        if let WorkKind::Produce(pw) = &mut work.kind {
            pw.workers.iter_mut().for_each(|w| *w = None);
        }
    }
}
