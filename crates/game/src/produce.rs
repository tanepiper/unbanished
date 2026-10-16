//! Workplace production: ResourceProduceComponent's states (`FUN_140130700`) and the
//! `ConsumeProduce` work (vtable `0x140167640`) of ConsumeProduceComponent: the wood cutter,
//! blacksmith, tailor, tavern, quarry and mine. See docs/JOBS.md "Production".
//!
//! A worker of the workplace's profession takes the work at a free "use" point. In order it
//! hauls goods the current recipe does not use out of the building, fetches missing inputs,
//! hauls the workplace's own output piles away while its "create" points are full, and
//! otherwise works one cycle at its use point. Every `cycles` completed cycles the next cycle
//! also consumes the inputs and leaves the output as a pile on a free create point.

use crate::character::{Action, MoveTo};
use crate::construction::{ProduceWork, Recipe};
use crate::jobs::{Jobs, Work, WorkKind};
use crate::pathfind::WALKABLE;
use crate::town::{Haul, Sim};

/// `FUN_14008e460`: the stock-manager slot a raw material counts towards (production limits).
pub fn production_group(flags: u32) -> Option<usize> {
    const ORDER: [(u32, usize); 21] = [
        (10, 0),
        (11, 1),
        (12, 2),
        (7, 3),
        (8, 9),
        (9, 4),
        (5, 5),
        (13, 6),
        (14, 7),
        (17, 8),
        (16, 10),
        (18, 11),
        (19, 12),
        (20, 13),
        (21, 14),
        (22, 15),
        (23, 16),
        (24, 17),
        (25, 18),
        (26, 19),
        (27, 20),
    ];
    ORDER
        .iter()
        .find(|(bit, _)| flags >> bit & 1 != 0)
        .map(|&(_, g)| g)
}

/// What the try decided before offering the output piles' pickups ([`Sim::produce_resume`]).
#[derive(Debug, Clone, Copy)]
pub(crate) struct ProduceResume {
    /// The recipe selected at the start of the try (its output is the one produced).
    selected: usize,
    slot: usize,
    point: Option<(usize, i32)>,
    all_inputs: bool,
}

impl Sim<'_, '_> {
    fn recipes(&self, e: usize) -> Option<&crate::construction::ProduceTemplate> {
        let site = self.sites.get(&e)?;
        self.site_templates[site.template].produce.as_ref()
    }

    /// `FUN_14011e940`: the selected recipe; selector −1 draws `RandFloat` over the weights.
    pub(crate) fn produce_select(&mut self, e: usize) -> usize {
        let Some(state) = self.sites.get(&e).and_then(|s| s.produce.as_ref()) else {
            return 0;
        };
        if state.selector >= 0 {
            return state.selector as usize;
        }
        if state.selector != -1 {
            return 0;
        }
        let weights = state.weights.clone();
        let total: f32 = weights.iter().sum();
        let r = self.rng.rand_float();
        let mut acc = 0.0;
        for (i, w) in weights.iter().enumerate() {
            acc += w;
            if r * total < acc {
                return i;
            }
        }
        0
    }

    /// `FUN_1401282f0(interact, 0, i)`: use point `i` in the world and its heading (the
    /// point's yaw turned with the building; inferred).
    pub(crate) fn use_point(&self, e: usize, i: usize) -> ([f32; 3], f32) {
        let p = &self.placed[e];
        let t = &self.templates[p.template];
        let local = t.use_points.get(i).copied().unwrap_or_default();
        let mut yaw = p.yaw + t.use_yaws.get(i).copied().unwrap_or(0.0);
        while yaw > 180.0 {
            yaw -= 360.0;
        }
        while yaw <= -180.0 {
            yaw += 360.0;
        }
        (self.world_point(e, local), yaw)
    }

    /// The cell of create point `i` (`FUN_1401282f0(interact, 2, i)`).
    pub(crate) fn create_cell(&self, e: usize, i: usize) -> i32 {
        let t = &self.templates[self.placed[e].template];
        let local = t.create_points.get(i).copied().unwrap_or_default();
        self.cell_of(self.world_point(e, local))
    }

    /// ConsumeProduceComponent slot `+0x90` (`FUN_14011e1d0`) and the work's constructor
    /// (`FUN_1400ee6d0`): one worker slot per use point, one state per create point.
    pub(crate) fn produce_work(&self, e: usize) -> Work {
        let t = &self.templates[self.placed[e].template];
        let profession = self
            .workforce
            .workplaces
            .get(&e)
            .and_then(|w| w.profession)
            .unwrap_or(self.professions.laborer);
        Work::new(
            e,
            self.rect_cell(e),
            profession,
            WorkKind::Produce(ProduceWork {
                workers: vec![None; t.use_points.len()],
                points: vec![0; t.create_points.len()],
                full: false,
                cycles: 0,
            }),
        )
    }

    /// `FUN_1401307e0`: switch production off (`+0x48`) and go idle (0xe).
    pub(crate) fn produce_disable(&mut self, e: usize) {
        let Some(site) = self.sites.get_mut(&e) else {
            return;
        };
        if let Some(p) = &mut site.produce {
            p.disabled = true;
        }
        let m = &site.machine;
        let state = if m.pending < 0 { m.state } else { m.pending };
        if state != 0xe {
            site.machine.request(0xe);
        }
    }

    /// `FUN_140130460` (transition 2 → 0xd): every create point's cell becomes ground with no
    /// object, ready for output piles.
    pub(crate) fn produce_clear_points(&mut self, e: usize) {
        let n = self.templates[self.placed[e].template].create_points.len();
        for i in 0..n {
            let cell = self.create_cell(e, i) as usize;
            self.cells.types[cell] = crate::terrain::CellType(2);
            self.cells.objects[cell] = None;
        }
    }

    /// `FUN_1400a1210(map, cell, 1, owner, mask)`: the nearest cell, in rings of radius 0..=6
    /// (rows, then columns), with no object, no occupant (or `owner`), not water, no road
    /// (built or planned), whose type is in `mask`.
    pub(crate) fn free_cell(&self, cell: i32, owner: Option<usize>, mask: u32) -> Option<i32> {
        self.free_cell_where(cell, owner, |t, _| mask >> (t & 0x1f) & 1 != 0)
    }

    /// `FUN_1400a1210(map, cell, 0, none, _)` (a loose pile's cell, `FUN_1400ccd90`): as
    /// [`Sim::free_cell`], on ground (type 2) with no occupant, no flags, and the terrain's
    /// normal at the cell's middle within `0.97` of up.
    pub(crate) fn free_flat_cell(&self, cell: i32) -> Option<i32> {
        let n = self.size as i32;
        self.free_cell_where(cell, None, |t, i| {
            if t != 2 || self.cells.occupants[i].is_some() || self.cells.flags[i] != 0 {
                return false;
            }
            let half = (n >> 1) as f32 - 0.5;
            let (x, y) = ((i as i32 % n) as f32 - half, (i as i32 / n) as f32 - half);
            let mut normal = [0.0; 3];
            self.terrain.ground(x, y, &mut normal);
            0.97 <= normal[2]
        })
    }

    fn free_cell_where(
        &self,
        cell: i32,
        owner: Option<usize>,
        ok: impl Fn(u32, usize) -> bool,
    ) -> Option<i32> {
        let n = self.size as i32;
        let (cx, cy) = (cell % n, cell / n);
        for r in 0..=6 {
            for y in cy - r..=cy + r {
                for x in cx - r..=cx + r {
                    if (y - cy).abs() != r && (x - cx).abs() != r {
                        continue;
                    }
                    if x < 0 || y < 0 || x >= n || y >= n {
                        continue;
                    }
                    let i = (y * n + x) as usize;
                    if self.cells.objects[i].is_some() {
                        continue;
                    }
                    if let Some(o) = self.cells.occupants[i] {
                        if Some(o) != owner {
                            continue;
                        }
                    }
                    let t = self.cells.types[i].0 as u32;
                    // Types 0 and 1 (`1 << t & 3`), a road (`+0x2a`) or a planned one.
                    if 1u32 << (t & 0x1f) & 3 != 0
                        || self.cells.road_kind[i] != 0xff
                        || self.pending_roads.contains_key(&(i as i32))
                    {
                        continue;
                    }
                    if ok(t, i) {
                        return Some(y * n + x);
                    }
                }
            }
        }
        None
    }

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
        self.set_icon(e, crate::professions::icon::LIMIT, is_limited);
        // `FUN_14011e870`: an exhausted workplace switches itself off.
        let alive = t.lifetime < 0 || state.produced < t.lifetime;
        if is_limited || !alive {
            if !alive {
                self.produce_disable(e);
            }
            self.set_icon(e, crate::professions::icon::NO_WORK, false);
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
        crate::jobs::NESTED
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
    fn produce_cycle(&mut self, work: &mut Work, c: usize, resume: ProduceResume) -> u8 {
        let e = work.entity;
        let Some((point, out_cell)) = resume.point else {
            return 1;
        };
        // `WorkUnavailable` while inputs are missing.
        self.set_icon(e, crate::professions::icon::NO_WORK, !resume.all_inputs);
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
    fn produce_taken(&mut self, work: &mut Work, c: usize, slot: usize) -> u8 {
        self.took_job(c, work);
        if let WorkKind::Produce(pw) = &mut work.kind {
            pw.workers[slot] = Some(c);
        }
        2
    }

    /// `FUN_1400c4440`: walk to the use point (with the work's tool), consume the inputs,
    /// work, then leave the output and report; a produced batch first sends the message to
    /// the building's component of the description's type `+0x38` (the quarry's dig).
    fn produce_sequence(&mut self, c: usize, work: &mut Work, s: ProduceSequence) {
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
                    Action::SendMessage(crate::character::Message {
                        component: crate::character::Component::Quarry,
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
    /// (`FUN_14008e3c0`, `+0x51` always set). Not ported: AutoPickupComponent.
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
                self.spawn_pile(template, (at % size, at / size), n, yaw, None);
                self.count_made(workplace, true, material, n);
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

/// `FUN_1400c4440`'s arguments.
struct ProduceSequence {
    target: [f32; 3],
    facing: f32,
    recipe: Recipe,
    produce: bool,
    out_cell: i32,
    work_type: u32,
    duration: f32,
    param: i32,
}
