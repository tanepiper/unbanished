//! Services: AttendanceComponent (schools, the chapel, the doctor), HealerComponent (the
//! herbalist), and `LocationWork` (vtable `0x140167ab0`), the work that keeps their staff at
//! the building. See docs/CITIZEN-LIFE.md.

use crate::behaviour::character::{Action, MoveTo};
use crate::pathfind::WALKABLE;
use crate::scheduler::ticks;
use crate::town::Sim;
use crate::work::construction::Attendance;
use crate::work::jobs::{Jobs, Work, WorkKind};
use crate::work::stock::StorageSearchMap;

/// `LocationWork`: one staff member at a time stands at the building.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LocationWork {
    /// `+0x48`.
    pub worker: Option<usize>,
}

impl Sim<'_, '_> {
    /// `FUN_1400445f0`: register (`+0x60`), set a LocationWork, enable the workplace.
    pub(crate) fn attendance_enter(&mut self, jobs: &mut Jobs, e: usize) {
        let site = &self.sites[&e];
        let capacity = self.site_templates[site.template].attendance.unwrap_or(15);
        let profession = self.workforce.workplaces.get(&e).and_then(|w| w.profession);
        self.attendance.entry(e).or_insert(Attendance {
            capacity,
            attendees: Vec::new(),
            profession,
        });
        if !self.attendance_list.contains(&e) {
            self.attendance_list.push(e);
        }
        let work = Work::new(
            e,
            self.rect_cell(e),
            profession.unwrap_or(self.professions.laborer),
            WorkKind::Location(LocationWork::default()),
        );
        self.set_work(jobs, e, work, 1, false);
        if let Some(w) = self.workforce.workplaces.get_mut(&e) {
            w.enabled = true;
        }
    }

    /// `FUN_140044530`: unregister, send everyone away, disable the workplace.
    pub(crate) fn attendance_leave(&mut self, e: usize) {
        self.attendance_list.retain(|&x| x != e);
        self.evict(e);
        self.enable_workplace(e, false);
    }

    /// `FUN_1400448c0`: every attendee leaves (school, doctor or chapel).
    pub(crate) fn evict(&mut self, e: usize) {
        let attendees = self
            .attendance
            .get_mut(&e)
            .map(|a| std::mem::take(&mut a.attendees))
            .unwrap_or_default();
        for c in attendees {
            let s = &mut self.states[c];
            if s.school == Some(e) {
                s.school = None;
            }
            if s.doctor == Some(e) {
                s.doctor = None;
            }
            if s.priest == Some(e) {
                s.priest = None;
            }
        }
    }

    /// `FUN_140044cd0` (the attendance's scheduled update): nobody works there, everyone goes.
    pub fn update_attendance(&mut self, visits: &[(usize, i64)]) {
        let empty: Vec<usize> = visits
            .iter()
            .filter_map(|&(e, _)| Some((e, self.attendance.get(&e)?)))
            .filter(|(e, a)| {
                !a.attendees.is_empty()
                    && self
                        .workforce
                        .workplaces
                        .get(e)
                        .is_none_or(|w| w.workers.is_empty())
            })
            .map(|(e, _)| e)
            .collect();
        for e in empty {
            self.evict(e);
        }
    }

    /// `FUN_140044710` for the status icon 2 at `FUN_140044590`: every registered attendance
    /// with the same profession is full.
    pub(crate) fn attendance_full(&self, e: usize) -> bool {
        let profession = |x: &usize| self.workforce.workplaces.get(x).and_then(|w| w.profession);
        !self.attendance_list.iter().any(|x| {
            profession(x) == profession(&e)
                && self
                    .attendance
                    .get(x)
                    .is_some_and(|a| (a.attendees.len() as i32) < a.capacity)
        })
    }

    /// LocationWork slot `+0x20` (`FUN_1400f4d50`).
    pub(crate) fn location_try(&mut self, work: &mut Work, c: usize) -> u8 {
        let WorkKind::Location(l) = &work.kind else {
            return 0;
        };
        if l.worker.is_some() {
            return 1;
        }
        if self.states[c].carried.used(self.materials) > 0 {
            self.drop_off(c);
        }
        // `FUN_1401280b0`: the nearest reachable use point (category 0).
        let from = self.citizen_cell(c);
        let Some(target) = self.nearest_use_point(work.entity, from) else {
            return 0;
        };
        // `FUN_1400c0960` with a CompleteTask.
        self.clear_citizen(c, Some(work));
        self.on_empty_think(c);
        self.push(
            c,
            Action::CompleteTask {
                work: work.id,
                param: 0,
            },
        );
        self.push(
            c,
            Action::PerformWork {
                facing: -1000.0,
                anim: 8,
                duration: ticks(1.0),
                elapsed: 0,
                consume: false,
            },
        );
        let walk = MoveTo::new(target, 1, 0, WALKABLE, 0, -1.0, self.rng);
        self.push(c, Action::move_to(walk));
        let s = &mut self.states[c];
        s.health_events |= 4;
        s.job = work.id;
        s.supply_activity = 0;
        if work.profession == s.profession {
            s.flags |= 0x20000;
        }
        if let WorkKind::Location(l) = &mut work.kind {
            l.worker = Some(c);
        }
        2
    }

    /// `FUN_1401280b0` over an entity's use points: the one with the lowest path rank from
    /// `from`.
    pub(crate) fn nearest_use_point(&mut self, e: usize, from: i32) -> Option<[f32; 3]> {
        let points = self.entities.get(e)?.use_points.clone();
        let mut best = 0xffff;
        let mut found = None;
        for p in points {
            let to = self.cell_of(p);
            let town = crate::behaviour::activities::TownView {
                size: self.size,
                types: &self.cells.types,
                regions: self.regions,
                occupants: &self.cells.occupants,
                objects: &self.cells.objects,
                floors: &self.cells.floors,
                entities: self.entities,
                happiness: self.happiness,
            };
            if !town.reachable(WALKABLE, from, to) {
                continue;
            }
            let rank = self.geometry_rank(from, to);
            if rank < best {
                best = rank;
                found = Some(p);
            }
        }
        found
    }

    /// The path-rank distance (`FUN_1400a4ce0`, else map width plus the straight distance).
    pub(crate) fn geometry_rank(&mut self, a: i32, b: i32) -> i32 {
        let (size, terrain) = (self.size, self.terrain);
        let floors = &self.cells.floors;
        let ground = move |c: i32| crate::town::floor_ground(floors, size, terrain, c);
        let town = crate::behaviour::activities::TownView {
            size: self.size,
            types: &self.cells.types,
            regions: self.regions,
            occupants: &self.cells.occupants,
            objects: &self.cells.objects,
            floors: &self.cells.floors,
            entities: self.entities,
            happiness: self.happiness,
        };
        let mut g = crate::behaviour::activities::StorageGeometry {
            town: &town,
            sectors: self.sectors,
            ground: &ground,
        };
        g.rank(a, b)
    }
}
