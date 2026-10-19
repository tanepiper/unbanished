//! Roads: planning a road's cells, BuildRoad, CompleteRoad and taking a planned road back. See
//! docs/ROADS.md.
//!
//! A planned cell gets its road kind at once and a BuildRoad work (a builder's, mask `0x2000`); the
//! builder clears what lies on it, brings a stone for a stone road, works it four seconds and the
//! cell becomes road. Clearing a road (a building's ClearArea or Remove Roads' marker) lays it back
//! to ground the same way.
//!
//! Original: `FUN_1400a1e30`, `FUN_1400ea020`, `FUN_1400cad20`, `FUN_1400a1b10`, `FUN_1400a1bd0`.
// vtable 0x140167430; vtable 0x140165f80; +0x2a

use crate::behaviour::character::{Action, MoveTo};
use crate::pathfind::WALKABLE;
use crate::terrain::CellType;
use crate::town::Sim;
use crate::work::jobs::{Jobs, Work, WorkKind};

/// `0x2000`: BuildRoad's registration mask (category 13, construction).
const MASK: u32 = 0x2000;
// +0x2a; +0x2b
/// No road, and while a road is being built / once it is.
pub const NONE: u8 = 0xff;
/// The bit of a road kind that says the road is being built.
pub const BUILDING: u8 = 0x80;
/// The cell type a cleared road goes back to.
pub const GROUND: u8 = 2;

// +0x48; +0x50; +0x58; +0x5c; +0x60..+0x68; +0x2b
/// BuildRoad's state: its builder, the material a cell takes (stone roads), the cell, the cell type
/// it becomes, the cell's road kind, byte and type before (restored when the plan is taken back).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RoadWork {
    /// The builder, if any.
    pub builder: Option<usize>,
    /// The raw material the cell takes (stone roads).
    pub material: Option<usize>,
    /// The cell.
    pub cell: i32,
    /// The cell type it becomes.
    pub ty: u8,
    /// The cell's road kind before.
    pub old_kind: u8,
    /// The cell's built byte before.
    pub old_built: u8,
    /// The cell's type before.
    pub old_type: u8,
}

impl Sim<'_, '_> {
    // +0x28; +0x2b
    /// Plan a better road (a lower kind) than the cell has: a plan already there is taken back; a
    /// cell that is neither road (types 0, 1) nor planned counts one more; a `BuildRoad` work is
    /// registered, and the cell keeps the new kind while it is built.
    ///
    /// Original: `FUN_1400a1e30(cell, kind, type, material)`.
    pub fn plan_road(
        &mut self,
        jobs: &mut Jobs,
        cell: i32,
        kind: u8,
        ty: u8,
        material: Option<usize>,
    ) {
        let i = cell as usize;
        if self.cells.road_kind[i] <= kind {
            return;
        }
        if self.pending_roads.contains_key(&cell) {
            self.unplan_road(jobs, cell);
        }
        let t = self.cells.types[i].0;
        if t != 0
            && t != 1
            && self.cells.road_kind[i] == NONE
            && !self.pending_roads.contains_key(&cell)
        {
            self.cells.flags[i] = self.cells.flags[i].wrapping_add(1);
        }
        let road = RoadWork {
            builder: None,
            material,
            cell,
            ty,
            old_kind: self.cells.road_kind[i],
            old_built: self.cells.road_built[i],
            old_type: t,
        };
        let work = Work::new(
            usize::MAX,
            cell,
            self.professions.builder,
            WorkKind::Road(road),
        );
        let id = jobs.add(work, MASK, None);
        self.pending_roads.insert(cell, id);
        self.cells.road_kind[i] = kind;
        self.cells.road_built[i] = BUILDING;
        self.cells.road_revision += 1;
    }

    /// A planned road's work goes and the cell gets back what it had (true); a cell without a road
    /// kind becomes ground at once (false); a road built stays (true, for the caller to clear).
    ///
    /// Original: `FUN_1400a1bd0`.
    pub fn unplan_road(&mut self, jobs: &mut Jobs, cell: i32) -> bool {
        let i = cell as usize;
        let mut restored = None;
        if let Some(id) = self.pending_roads.remove(&cell) {
            if let Some(WorkKind::Road(r)) = jobs.works.get(&id).map(|w| w.kind.clone()) {
                restored = Some((r.old_kind, r.old_built, r.old_type));
            }
            jobs.remove(id, self);
        }
        self.cells.road_revision += 1;
        if self.cells.road_kind[i] == NONE {
            self.set_cell_type(cell, GROUND);
            return false;
        }
        if let Some((kind, built, ty)) = restored {
            if kind == NONE {
                self.cells.flags[i] = self.cells.flags[i].wrapping_sub(1);
            }
            self.cells.road_kind[i] = kind;
            self.cells.road_built[i] = built;
            self.set_cell_type(cell, ty);
            if kind == NONE {
                return false;
            }
            *self.models_dirty = true;
        }
        true
    }

    /// `FUN_1400a47c0`: the cell's type (the path grid follows it).
    fn set_cell_type(&mut self, cell: i32, ty: u8) {
        self.cells.set_type(cell as usize, CellType(ty));
    }

    // +0x28
    /// The start of `CompleteRoad`: to ground (type 2) the cell
    /// loses its road (one less, no kind); else it becomes the road type. Either way it leaves the
    /// planned set. A road flattens the terrain at its cell's corners; the road decals follow
    /// `road_revision`.
    ///
    /// Original: `FUN_1400a1b10(cell, kind byte, type)`; `FUN_1400a35b0`, `FUN_1400b4840`.
    pub fn complete_road(&mut self, cell: i32, built: u8, ty: u8) {
        let i = cell as usize;
        if ty == GROUND {
            self.cells.flags[i] = self.cells.flags[i].wrapping_sub(1);
            self.cells.road_kind[i] = NONE;
            self.cells.road_built[i] = 0;
        } else {
            self.cells.road_built[i] = built;
            let n = self.size as i32;
            let (x, y) = (cell % n, cell / n);
            self.flatten_requests.push([x, y, x + 1, y + 1]);
        }
        self.set_cell_type(cell, ty);
        self.pending_roads.remove(&cell);
        self.cells.road_revision += 1;
        *self.models_dirty = true;
    }

    /// BuildRoad's slot `+0x20` (`FUN_1400e9660`): one builder at a time, the cell reachable.
    /// A pile on it is picked up first (its Pickup work offered), a resource not being removed
    /// is removed from the nearest free neighbour; else the cell is built: a dirt road at once,
    /// a stone road with a stone carried or fetched (dropping off first when the builder cannot
    /// take one).
    pub(crate) fn road_try(&mut self, work: &mut Work, c: usize, time: i64) -> u8 {
        let WorkKind::Road(r) = &work.kind else {
            return 0;
        };
        if r.builder.is_some() {
            return 1;
        }
        let (cell, ty, material) = (r.cell, r.ty, r.material);
        let here = self.citizen_cell(c);
        if !self.reachable(here, cell) {
            return 1;
        }
        if let Some(o) = self.cells.objects[cell as usize] {
            if let Some(pile) = self.piles.get(&o) {
                self.offers = vec![pile.work];
                return crate::work::jobs::NESTED;
            }
            if self.templates[self.placed[o].template].natural.is_some()
                && !self.claimed.contains(&o)
            {
                let Some(nb) = crate::pathfind::nearest_neighbour(
                    self.size,
                    &self.cells.types,
                    cell,
                    here,
                    0xe07,
                ) else {
                    return 1;
                };
                if !self.reachable(nb, here) {
                    return 1;
                }
                let point = self.ground_at(nb);
                self.removal_sequence(c, work, o, point, false);
                self.took_job(c, work);
                if let WorkKind::Road(r) = &mut work.kind {
                    r.builder = Some(c);
                }
                return 2;
            }
            return 1;
        }
        let target = self.ground_at(cell);
        match material {
            None => self.road_sequence(c, work, target, cell, ty, None, None),
            Some(m) => {
                if 0 < self.states[c].carried.available(m) {
                    self.road_sequence(c, work, target, cell, ty, Some(m), None);
                } else {
                    let Some(found) = self.find_storage(m, 1, false, here) else {
                        return 1;
                    };
                    let size = self.materials[m].size;
                    let carried = &self.states[c].carried;
                    if carried.capacity - carried.used(self.materials) < size {
                        self.drop_off(c);
                        return 2;
                    }
                    let entity = self.storage_sites[found.site].entity;
                    self.road_sequence(
                        c,
                        work,
                        target,
                        cell,
                        ty,
                        Some(m),
                        Some((found.storage, found.target, entity)),
                    );
                }
            }
        }
        let _ = time;
        self.took_job(c, work);
        if let WorkKind::Road(r) = &mut work.kind {
            r.builder = Some(c);
        }
        2
    }

    /// ClearArea's road cells: `FUN_1400c54a0` to ground (type 2, `+0x2b` 0), the task's
    /// parameter the cell.
    pub(crate) fn road_clear_sequence(
        &mut self,
        c: usize,
        work: &mut Work,
        target: [f32; 3],
        cell: i32,
    ) {
        self.clear_citizen(c, Some(work));
        self.on_empty_think(c);
        self.push(
            c,
            Action::CompleteTask {
                work: work.id,
                param: cell,
            },
        );
        self.push(
            c,
            Action::CompleteRoad {
                cell,
                ty: GROUND,
                built: 0,
            },
        );
        self.push(c, Action::ShowTool(-1));
        self.push(c, self.perform(c, -1000.0, 10, 4.0, true));
        let walk = MoveTo::new(target, 2, 0, WALKABLE, 0, -1.0, self.rng);
        self.push(c, Action::move_to(walk));
        self.push(c, Action::ShowTool(1));
    }

    /// `FUN_1400c54a0`: (fetch: walk to the storage, take what can be carried of the material)
    /// → the shovel → walk to the cell → work it 4 s (clip 10, not turning) → (use one of the
    /// material) → CompleteRoad → the task (parameter 1, which ends the work).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn road_sequence(
        &mut self,
        c: usize,
        work: &mut Work,
        target: [f32; 3],
        cell: i32,
        ty: u8,
        material: Option<usize>,
        fetch: Option<(usize, [f32; 3], usize)>,
    ) {
        self.clear_citizen(c, Some(work));
        self.on_empty_think(c);
        self.push(
            c,
            Action::CompleteTask {
                work: work.id,
                param: 1,
            },
        );
        self.push(
            c,
            Action::CompleteRoad {
                cell,
                ty,
                built: NONE,
            },
        );
        self.push(c, Action::ShowTool(-1));
        if let Some(m) = material {
            self.push(
                c,
                Action::ConsumeStoredRawMaterial {
                    storage: usize::MAX,
                    items: vec![(m, 1)],
                    reserved: false,
                },
            );
        }
        self.push(c, self.perform(c, -1000.0, 10, 4.0, true));
        let walk = MoveTo::new(target, 2, 0, WALKABLE, 0, -1.0, self.rng);
        self.push(c, Action::move_to(walk));
        self.push(c, Action::ShowTool(1));
        if let (Some(m), Some((storage, source, entity))) = (material, fetch) {
            let facing =
                crate::behaviour::character::work_facing(self.facing_centre(entity), source);
            self.push(c, self.perform(c, facing, 7, 0.75, false));
            let carried = &self.states[c].carried;
            let amount = (carried.capacity / self.materials[m].size.max(1))
                .min(self.storages[storage].available(m));
            self.storages[storage].reserve(m, amount);
            self.push(
                c,
                Action::PickupStoredRawMaterial {
                    storage,
                    items: vec![(m, amount)],
                    partial: true,
                },
            );
            self.push(c, self.perform(c, facing, 6, 0.75, false));
            let walk_clip = if 0 < self.states[c].carried.used(self.materials) {
                4
            } else {
                1
            };
            let walk = MoveTo::new(source, walk_clip, 0, WALKABLE, 0, -1.0, self.rng);
            self.push(c, Action::move_to(walk));
        }
    }
}
