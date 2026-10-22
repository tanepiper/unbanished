//! A crop field's planting and tending work.

use super::*;

impl Sim<'_, '_> {
    /// PlantArea slot `+0x20` (`FUN_1400f94e0`).
    pub(crate) fn plant_try(&mut self, work: &mut Work, c: usize) -> u8 {
        let e = work.entity;
        let WorkKind::PlantArea(pa) = &work.kind else {
            return 0;
        };
        if self.work_max(e) < pa.claims.len() as i32 {
            return 1;
        }
        let Some(field) = self.field(e).cloned() else {
            return 0;
        };
        let from = self.citizen_cell(c);
        let count = pa.need.len() as i32;
        let mut nothing = true;
        let mut i = 0;
        while i < count {
            let pa = match &work.kind {
                WorkKind::PlantArea(pa) => pa,
                _ => return 0,
            };
            if !pa.need[i as usize] {
                i += 1;
                continue;
            }
            nothing = false;
            if pa.claims.contains_key(&i) {
                i += 1;
                continue;
            }
            let j = i + 1;
            let pair = j < count && pa.need[j as usize] && !pa.claims.contains_key(&j);
            let cell_i = field.cells[i as usize].cell;
            let cell_j = if pair {
                field.cells[j as usize].cell
            } else {
                -1
            };
            let Some(nb) = nearest_neighbour(self.size, &self.cells.types, cell_i, from, 0xe0f)
            else {
                i += 1;
                continue;
            };
            if !self.reachable(from, nb) {
                i += 1;
                continue;
            }
            let crop = pa.crop;
            let point = self.ground_at(nb);
            let param = (if pair { j } else { count }) << 16 | i;
            let cells: Vec<(i32, i32)> = if pair {
                vec![(cell_i, i), (cell_j, j)]
            } else {
                vec![(cell_i, i)]
            };
            self.plant_crop_sequence(c, work, e, crop, point, &cells, param);
            self.took_job(c, work);
            if let WorkKind::PlantArea(pa) = &mut work.kind {
                pa.claims.insert(i, c);
                pa.need[i as usize] = false;
                if pair {
                    pa.claims.insert(j, c);
                    pa.need[j as usize] = false;
                }
            }
            return 2;
        }
        if nothing {
            let idle = matches!(&work.kind, WorkKind::PlantArea(pa) if pa.claims.is_empty());
            if idle {
                self.crop_planted(e);
            }
        }
        1
    }

    /// `FUN_140120860`: planting is over; cells never planted count as done.
    pub(super) fn crop_planted(&mut self, e: usize) {
        let m = &self.sites[&e].machine;
        let state = if m.pending < 0 { m.state } else { m.pending };
        if state != 9 {
            return;
        }
        if let Some(f) = self.field_mut(e) {
            for c in f.cells.iter_mut().filter(|c| c.state == 0) {
                c.state = 2;
            }
        }
    }

    /// `FUN_1400c31f0` as PlantArea calls it: take the hoe, walk to the cell, work 2.1 s,
    /// plant each cell (the field occupies it) and tell the field, put the hoe away, report.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn plant_crop_sequence(
        &mut self,
        c: usize,
        work: &mut Work,
        field: usize,
        crop: Option<usize>,
        point: [f32; 3],
        cells: &[(i32, i32)],
        param: i32,
    ) {
        self.clear_citizen(c, Some(work));
        self.on_empty_think(c);
        self.push(
            c,
            Action::CompleteTask {
                work: work.id,
                param,
            },
        );
        self.push(c, Action::ShowTool(-1));
        for &(cell, index) in cells {
            self.push(
                c,
                Action::SendMessage(Message {
                    component: Component::CropField,
                    message: 0,
                    entity: Some(field),
                    a: index,
                    b: 0,
                }),
            );
            if let Some(t) = crop {
                self.push(
                    c,
                    Action::CreateNaturalResource {
                        template: t,
                        occupant: Some(field),
                        cell,
                    },
                );
            }
        }
        self.push(c, self.perform(c, -1000.0, 0xc, 2.1, true));
        let walk = MoveTo::new(point, 3, 0, WALKABLE, 0, -1.0, self.rng);
        self.push(c, Action::move_to(walk));
        self.push(c, Action::ShowTool(3));
    }

    /// TendArea slot `+0x20` (`FUN_1400fa960`) for a crop field.
    pub(crate) fn tend_try(&mut self, work: &mut Work, c: usize) -> u8 {
        let e = work.entity;
        let WorkKind::TendArea(t) = &work.kind else {
            return 0;
        };
        if self.work_max(e) < t.workers.len() as i32 {
            return 1;
        }
        let from = self.citizen_cell(c);
        let Some(field) = self.field(e) else { return 0 };
        let count = field.cells.len() as u32;
        let index = self.rng.rand_below(count) as i32;
        let Some(cell) = self
            .field(e)
            .and_then(|f| f.cells.get(index as usize))
            .map(|c| c.cell)
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
            component: Component::CropField,
            message: 1,
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

    /// `FUN_1400c0570`: take the hoe, walk next to the cell, work 10 s facing it, tell the
    /// field, put the hoe away, report.
    pub(crate) fn tend_sequence(
        &mut self,
        c: usize,
        work: &mut Work,
        stand: [f32; 3],
        at: [f32; 3],
        message: Message,
    ) {
        self.clear_citizen(c, Some(work));
        self.on_empty_think(c);
        self.push(
            c,
            Action::CompleteTask {
                work: work.id,
                param: 0,
            },
        );
        let heading = crate::behaviour::character::work_facing(at, stand);
        self.push(c, Action::ShowTool(-1));
        self.push(c, Action::SendMessage(message));
        self.push(c, self.perform(c, heading, 0xc, 10.0, true));
        let walk = MoveTo::new(stand, 3, 0, WALKABLE, 0, -1.0, self.rng);
        self.push(c, Action::move_to(walk));
        self.push(c, Action::ShowTool(3));
    }
}
