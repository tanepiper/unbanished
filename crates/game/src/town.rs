//! The simulation context jobs and construction run in: [`Sim`] implements the job manager's
//! work slots ([`WorkHost`]) and the building state callbacks ([`Callbacks`]), over the
//! citizens, the map, the storages and the construction state. See docs/JOBS.md.

use std::collections::BTreeMap;

use banished_formats::types::AnimationGroup;

use crate::activities::{EntityView, StorageGeometry, TownView};
use crate::agents::{CitizenRulesAi, CitizenState};
use crate::character::{Action, Character, Component, Message, MoveTo};
use crate::construction::{
    run, BuildWork, Callback, Callbacks, Gather, Machine, Site, SiteTemplate,
};
use crate::jobs::{JobEvent, Jobs, Work, WorkHost, WorkKind};
use crate::pathfind::WALKABLE;
use crate::placement::{PlacementRules, Rect};
use crate::professions::{Professions, Worker, Workforce};
use crate::random::Random;
use crate::scheduler::{seconds, ticks};
use crate::sectors::SectorGraph;
use crate::seeding::{Cells, Placed, Template};
use crate::stock::{find_material_storage, RawMaterial, Storage, StorageSite};
use crate::terrain::HeightGrid;

/// `0x2001`: the construction works' registration mask (category 13).
pub const CONSTRUCTION_MASK: u32 = 0x2001;

/// The world the citizens' update borrows (everything but the citizens themselves).
pub struct Env<'a, 't> {
    pub cells: &'a mut Cells,
    pub placed: &'a mut [Placed],
    pub templates: &'a [Template],
    pub terrain: &'a HeightGrid<'t>,
    pub materials: &'a [RawMaterial],
    pub storages: &'a mut [Storage],
    pub sites: &'a [StorageSite],
    pub rng: &'a mut Random,
    pub construction: &'a mut crate::construction::Construction,
    /// The stock manager's `(count, limit)` per slot from the last recount.
    pub stock: [(i32, i32); 21],
    /// The environment's temperature (`Game +0x130 → +0x18`).
    pub temperature: f32,
}

/// Everything a work slot or a state callback touches.
pub struct Sim<'s, 't> {
    pub characters: &'s mut [Character],
    pub states: &'s mut [CitizenState],
    pub groups: &'s [Option<&'s AnimationGroup>],
    /// Adult and not at school (the citizens assignment counts).
    pub adult: &'s [bool],
    pub rules: CitizenRulesAi,
    pub size: usize,
    pub cells: &'s mut Cells,
    pub regions: &'s [u16],
    pub sectors: &'s mut SectorGraph,
    pub terrain: &'s HeightGrid<'t>,
    pub placed: &'s mut [Placed],
    pub templates: &'s [Template],
    pub entities: &'s [EntityView],
    pub storages: &'s mut [Storage],
    pub storage_sites: &'s [StorageSite],
    pub materials: &'s [RawMaterial],
    pub rng: &'s mut Random,
    pub stock: [(i32, i32); 21],
    pub temperature: f32,
    pub professions: &'s Professions,
    pub workforce: &'s mut Workforce,
    pub site_templates: &'s [SiteTemplate],
    pub sites: &'s mut BTreeMap<usize, Site>,
    pub site_list: &'s mut Vec<usize>,
    pub events: Vec<JobEvent>,
}

/// FUN_1400a48b0 without bridge picking.
fn cell_ground(size: usize, terrain: &HeightGrid, cell: i32) -> [f32; 3] {
    let n = size as i32;
    let h = (n >> 1) as f32 - 0.5;
    let (x, y) = ((cell % n) as f32 - h, (cell / n) as f32 - h);
    let mut normal = [0.0; 3];
    [x, y, terrain.ground(x, y, &mut normal)]
}

impl Sim<'_, '_> {
    fn cell_of(&self, p: [f32; 3]) -> i32 {
        let n = self.size as i32;
        let half = (n >> 1) as f32;
        let x = ((p[0] + half) as i32).clamp(0, n - 1);
        let y = ((p[1] + half) as i32).clamp(0, n - 1);
        n * y + x
    }

    fn citizen_cell(&self, c: usize) -> i32 {
        self.cell_of(self.characters[c].position)
    }

    fn town(&self) -> TownView<'_> {
        TownView {
            size: self.size,
            types: &self.cells.types,
            regions: self.regions,
            occupants: &self.cells.occupants,
            objects: &self.cells.objects,
            entities: self.entities,
            happiness: &[],
        }
    }

    fn reachable(&self, a: i32, b: i32) -> bool {
        self.town().reachable(WALKABLE, a, b)
    }

    /// `FUN_1400a4ce0(map, a, b, 1)`: −1 when not connected.
    fn rank_raw(&mut self, a: i32, b: i32) -> i32 {
        let reachable = self.reachable(a, b);
        self.sectors.rebuild(&self.cells.types);
        self.sectors
            .distance(a, b, reachable, true, &self.cells.types)
    }

    /// Path rank with the callers' fallback `n + (int)sqrtf(dx² + dy²)`.
    fn rank_or_distance(&mut self, a: i32, b: i32) -> i32 {
        let d = self.rank_raw(a, b);
        if d >= 0 {
            return d;
        }
        let n = self.size as i32;
        let (dx, dy) = ((a % n - b % n) as f32, (a / n - b / n) as f32);
        n + (dx * dx + dy * dy).sqrt() as i32
    }

    /// An entity's model-space point in the world (`ModelComponent` transform).
    fn world_point(&self, entity: usize, local: [f32; 3]) -> [f32; 3] {
        let p = &self.placed[entity];
        let (s, c) = p.yaw.to_radians().sin_cos();
        [
            p.position[0] + c * local[0] - s * local[1],
            p.position[1] + s * local[0] + c * local[1],
            p.position[2] + local[2],
        ]
    }

    /// `FUN_1401282f0(interact, 1, i)`: build point `i` in the world.
    fn build_point(&self, entity: usize, i: usize) -> [f32; 3] {
        let t = &self.templates[self.placed[entity].template];
        self.world_point(entity, t.build_points.get(i).copied().unwrap_or_default())
    }

    fn build_point_count(&self, entity: usize) -> usize {
        self.templates[self.placed[entity].template]
            .build_points
            .len()
    }

    /// The point `FUN_1400c7e70` faces: the model's position, else the map-rectangle centre.
    fn facing_centre(&self, entity: usize) -> [f32; 3] {
        let p = &self.placed[entity];
        if self.templates[p.template].model.is_some() {
            p.position
        } else {
            let n = self.size as i32;
            let (x, y) = p.cell;
            let (w, h) = p.footprint;
            cell_ground(self.size, self.terrain, (y + (h >> 1)) * n + x + (w >> 1))
        }
    }

    /// `Work +0x28`: the owner's map-rectangle centre cell.
    fn rect_cell(&self, entity: usize) -> i32 {
        let p = &self.placed[entity];
        let n = self.size as i32;
        (p.cell.1 + (p.footprint.1 >> 1)) * n + (p.footprint.0 >> 1) + p.cell.0
    }

    pub fn workers(&self) -> Vec<Worker> {
        self.states
            .iter()
            .zip(self.adult)
            .map(|(s, &adult)| Worker {
                adult,
                profession: s.profession,
                workplace: s.workplace,
                home: s.home,
            })
            .collect()
    }

    pub fn set_workers(&mut self, workers: &[Worker]) {
        for (s, w) in self.states.iter_mut().zip(workers) {
            s.profession = w.profession;
            s.workplace = w.workplace;
        }
    }

    /// `FUN_1400ca8d0` on a citizen: abort everything; aborted CompleteTasks of `current` go
    /// straight to its fail slot, the others to the manager's queue.
    fn clear_citizen(&mut self, c: usize, mut current: Option<&mut Work>) {
        let groups = self.groups;
        let Some(group) = groups.get(c).copied().flatten() else {
            return;
        };
        let ch = &mut self.characters[c];
        let mut stack = std::mem::take(&mut ch.stack);
        stack.clear(ch, group, self.storages);
        let outbox = std::mem::take(&mut stack.outbox);
        ch.stack = stack;
        for m in outbox {
            let e = JobEvent {
                work: m.a,
                citizen: c,
                param: m.b,
                complete: m.message == 0,
            };
            match current.as_deref_mut() {
                Some(w) if w.id == e.work => self.fail(w, c, e.param),
                _ => self.events.push(e),
            }
        }
    }

    fn push(&mut self, c: usize, action: Action) {
        let groups = self.groups;
        let Some(group) = groups.get(c).copied().flatten() else {
            return;
        };
        let ch = &mut self.characters[c];
        let mut stack = std::mem::take(&mut ch.stack);
        stack.push(ch, group, action);
        ch.stack = stack;
    }

    fn on_empty_think(&mut self, c: usize) {
        self.characters[c]
            .stack
            .set_on_empty(vec![Action::SendMessage(Message {
                component: Component::CitizenAi,
                message: 0,
                entity: None,
                a: 0,
                b: 0,
            })]);
    }

    /// `FUN_1400cebf0`: PerformWork, slower without a tool when it uses one.
    fn perform(&self, c: usize, facing: f32, anim: usize, secs: f32, tool: bool) -> Action {
        let mut duration = ticks(secs);
        if tool && self.states[c].tool_uses < 1 {
            duration = ticks(seconds(duration) * self.rules.no_tool_factor);
        }
        Action::PerformWork {
            facing,
            anim,
            duration,
            elapsed: 0,
            consume: tool,
        }
    }

    /// The writes every successful assignment makes on the citizen.
    fn took_job(&mut self, c: usize, work: &Work) {
        let s = &mut self.states[c];
        s.health_events |= 4;
        s.job = work.id;
        s.supply_activity = 0;
        if work.profession == s.profession && work.id >= 0 {
            s.flags |= 0x20000;
        }
    }

    /// `FUN_1400c4c60`: fetch `amount` of `material` from a storage and deliver it to `site`.
    #[allow(clippy::too_many_arguments)]
    fn haul_sequence(
        &mut self,
        c: usize,
        work: &mut Work,
        site: usize,
        target: [f32; 3],
        source_storage: usize,
        source_entity: usize,
        source: [f32; 3],
        material: usize,
        amount: i32,
        param: i32,
    ) {
        self.clear_citizen(c, Some(work));
        self.on_empty_think(c);
        let face_target = crate::character::work_facing(self.facing_centre(site), target);
        let face_source = crate::character::work_facing(self.facing_centre(source_entity), source);
        let site_storage = self.sites[&site].storage;
        self.push(c, self.perform(c, face_target, 7, 0.75, false));
        self.push(c, Action::ShowTool(-1));
        self.push(
            c,
            Action::CompleteTask {
                work: work.id,
                param,
            },
        );
        self.push(
            c,
            Action::DropOff {
                storage: site_storage,
                material,
                count: amount,
            },
        );
        self.push(c, self.perform(c, face_target, 6, 0.75, false));
        let walk = MoveTo::new(target, 4, 0, WALKABLE, 0, -1.0, self.rng);
        self.push(c, Action::move_to(walk));
        self.push(c, self.perform(c, face_source, 7, 0.75, false));
        self.push(c, Action::ShowTool(-2));
        // `FUN_1400cf9f0` reserves at construction.
        self.storages[source_storage].reserve(material, amount);
        self.push(
            c,
            Action::PickupStoredRawMaterial {
                storage: source_storage,
                items: vec![(material, amount)],
                partial: false,
            },
        );
        self.push(c, self.perform(c, face_source, 6, 0.75, false));
        let walk = MoveTo::new(source, 1, 0, WALKABLE, 0, -1.0, self.rng);
        self.push(c, Action::move_to(walk));
    }

    /// `FUN_1400c5b60`: walk to a build point and work there once.
    fn build_sequence(
        &mut self,
        c: usize,
        work: &mut Work,
        site: usize,
        point: [f32; 3],
        index: i32,
    ) {
        let facing = crate::character::work_facing(self.facing_centre(site), point);
        self.clear_citizen(c, Some(work));
        self.on_empty_think(c);
        let r = self.rng.rand_below(2);
        self.push(
            c,
            Action::CompleteTask {
                work: work.id,
                param: index,
            },
        );
        // A destroy work messages the DestroyComponent instead (not ported).
        self.push(
            c,
            Action::SendMessage(Message {
                component: Component::Build,
                message: 0,
                entity: Some(site),
                a: 0,
                b: 0,
            }),
        );
        self.push(c, Action::ShowTool(-1));
        self.push(
            c,
            self.perform(c, facing, if r != 0 { 13 } else { 16 }, 3.0, true),
        );
        let walk = MoveTo::new(point, 2, 0, WALKABLE, 0, -1.0, self.rng);
        self.push(c, Action::move_to(walk));
        self.push(c, Action::ShowTool(if r != 0 { 4 } else { 7 }));
    }

    /// `FUN_1400fc3c0`: may this citizen take work this far away?
    fn patience(&mut self, work: &mut Work, c: usize, a: i32, b: i32, time: i64) -> bool {
        work.patience = work
            .patience
            .wrapping_add(time.wrapping_sub(work.patience_base));
        if self.states[c].profession == self.professions.laborer {
            return true;
        }
        if self.states[c].workplace.is_none() {
            return true;
        }
        let rank = self.rank_raw(a, b);
        let r = self.rules;
        let limit = ticks(r.work_minutes as f32 * 300.0);
        if ((self.states.len() as i32) / 2) < work.far_count && work.patience < limit {
            work.patience = work.patience.wrapping_mul(2);
        }
        if work.patience < 0 || limit < work.patience {
            work.patience = limit;
        }
        let v = (((seconds(work.patience) / seconds(time)) / (r.work_minutes * 300) as f32)
            * (r.work_far - r.work_near) as f32) as i32
            + r.work_near;
        if v < rank {
            work.far_count += 1;
            false
        } else {
            rank <= r.work_ok || r.work_cold <= self.temperature
        }
    }

    /// `FUN_14011d910` for a footprint whose description clears resources and piles: every
    /// cell's object `+0x08` is empty (`FUN_1400a2f90`). Not ported: the `+0x29` empty-cell
    /// test (`FUN_1400a2d30`) and the reference-filtered test (`FUN_1400a2e00`).
    fn is_clear(&self, entity: usize) -> bool {
        let p = &self.placed[entity];
        let n = self.size as i32;
        let (x, y) = p.cell;
        let (w, h) = p.footprint;
        (y..y + h)
            .all(|cy| (x..x + w).all(|cx| self.cells.objects[(cy * n + cx) as usize].is_none()))
    }

    /// `FUN_14010db00`: the site storage holds every requirement.
    fn has_materials(&self, entity: usize) -> bool {
        let site = &self.sites[&entity];
        let t = &self.site_templates[site.template];
        t.materials
            .iter()
            .all(|&(n, m)| self.storages[site.storage].available(m) >= n * site.stages)
    }

    /// `FUN_140134130`: register a work for the site's WorkComponent, replacing any.
    fn set_work(&mut self, jobs: &mut Jobs, entity: usize, work: Work, mask: u32, keep: bool) {
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

    /// `FUN_140134020`: keep the priority, remove the work.
    fn remove_work(&mut self, jobs: &mut Jobs, entity: usize) {
        let id = self.sites[&entity].work_id;
        if let Some(w) = jobs.works.get(&id) {
            self.sites.get_mut(&entity).unwrap().work_priority = Some(w.priority);
            jobs.remove(id, self);
        }
    }

    fn set_mesh(&mut self, entity: usize, mesh: i32) {
        self.placed[entity].mesh = if mesh < 0 { usize::MAX } else { mesh as usize };
    }

    fn meshes(&self, entity: usize) -> i32 {
        let site = &self.sites[&entity];
        let v = self.placed[entity].variant;
        self.site_templates[site.template]
            .meshes
            .get(v)
            .copied()
            .unwrap_or(1) as i32
    }

    /// `FUN_14010d4c0`: the building's own storage replaces the material store.
    fn finish_storage(&mut self, entity: usize) {
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
    fn stamp(&mut self, entity: usize) {
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
        crate::placement::stamp(self.cells, &rules, rect, degrees, entity);
    }

    /// Activate the sites placed since the last iteration (`FUN_1400c9d70` → activate).
    pub fn activate(&mut self, jobs: &mut Jobs, entity: usize) {
        run(self, jobs, entity, false, false);
    }

    /// The bit-4 and scheduled StateMachine updates of every site.
    pub fn update_sites(&mut self, jobs: &mut Jobs) {
        let ids: Vec<usize> = self.sites.keys().copied().collect();
        for e in &ids {
            run(self, jobs, *e, false, true);
        }
        // Deviation: every site's scheduled update runs each iteration (bucket 3's quota of
        // five visits each entity every iteration while it holds at most five).
        for e in ids {
            run(self, jobs, e, true, true);
        }
    }
}

impl Callbacks for Sim<'_, '_> {
    fn machine(&mut self, site: usize) -> &mut Machine {
        &mut self.sites.get_mut(&site).unwrap().machine
    }

    fn call(&mut self, jobs: &mut Jobs, e: usize, f: Callback) -> bool {
        match f {
            Callback::None | Callback::Missing => false,
            Callback::ClearEnter0 => {
                // Not ported: the ClearArea work (`FUN_1400ebe30`); an uncleared site waits.
                if self.is_clear(e) {
                    self.machine(e).request(1);
                }
                true
            }
            Callback::ClearUpdate0 => {
                if self.is_clear(e) {
                    self.machine(e).request(1);
                }
                false
            }
            Callback::ClearExit0 | Callback::ClearExit15 => true,
            Callback::ClearEnter1 => {
                self.machine(e).request(2);
                true
            }
            Callback::ClearEnter2 => {
                // Entity flag 1: the MapComponent stamps the footprint.
                self.stamp(e);
                true
            }
            Callback::WorkPlaceEnter0 => {
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
                    self.workforce.set_max_workers(&mut workers, e, max);
                    // `FUN_1401350d0(1)`. Not ported: its status icons.
                    self.workforce.workplaces.get_mut(&e).unwrap().enabled = true;
                }
                self.set_workers(&workers);
                false
            }
            Callback::WorkPlaceEnter2 => {
                let builder = self.professions.builder;
                let mut workers = self.workers();
                let template = self.sites[&e].template;
                let id = self.sites[&e].work_id;
                if let Some(w) = self.workforce.workplaces.get(&e) {
                    if w.profession == Some(builder) {
                        self.workforce.unregister(e, builder);
                        let desc = self.site_templates[template].profession;
                        let max = self.site_templates[template].max_workers;
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
                false
            }
            Callback::WorkPlaceEnter3 => false,
            Callback::BuildEnter1 => {
                self.sites.get_mut(&e).unwrap().stages = 1;
                // Not ported: MultiObject stage counts and the ClearArea's terrain flattening
                // (`FUN_14011d740` → `FUN_1400a35b0`).
                if !self.site_list.contains(&e) {
                    self.site_list.push(e);
                }
                if !self.has_materials(e) {
                    let site = &self.sites[&e];
                    let t = &self.site_templates[site.template];
                    let stored: Vec<i32> = t
                        .materials
                        .iter()
                        .map(|&(_, m)| self.storages[site.storage].available(m))
                        .collect();
                    let gather = Gather {
                        transit: vec![0; stored.len()],
                        stored,
                        workers: Default::default(),
                    };
                    let work = Work::new(
                        e,
                        self.rect_cell(e),
                        self.professions.laborer,
                        WorkKind::Gather(gather),
                    );
                    self.set_work(jobs, e, work, CONSTRUCTION_MASK, true);
                    let site = &self.sites[&e];
                    let t = &self.site_templates[site.template];
                    let (mut capacity, mut accepts) = (0i32, 0u32);
                    for &(n, m) in &t.materials {
                        accepts |= self.materials[m].flags;
                        capacity += n * self.materials[m].size * site.stages;
                    }
                    let s = &mut self.storages[site.storage];
                    s.items.clear();
                    s.capacity = capacity;
                    s.accepts = accepts;
                } else {
                    self.machine(e).request(8);
                }
                true
            }
            Callback::BuildUpdate1 => {
                if self.has_materials(e) {
                    self.remove_work(jobs, e);
                    self.machine(e).request(8);
                }
                true
            }
            Callback::BuildEnter8 => {
                // Not ported: the decal's alpha.
                self.set_mesh(e, 0);
                let work = Work::new(
                    e,
                    self.rect_cell(e),
                    self.professions.builder,
                    WorkKind::Build(BuildWork {
                        occupied: vec![false; self.build_point_count(e)],
                        ..Default::default()
                    }),
                );
                self.set_work(jobs, e, work, CONSTRUCTION_MASK, true);
                true
            }
            Callback::BuildUpdate8 => {
                let site = &self.sites[&e];
                let total = self.site_templates[site.template].work * site.stages;
                let meshes = self.meshes(e);
                if site.progress < total {
                    let mut m = ((site.progress as f32 / total as f32) * meshes as f32) as i32;
                    if m == meshes - 1 {
                        m = meshes - 2;
                    }
                    self.set_mesh(e, m);
                } else {
                    self.remove_work(jobs, e);
                    self.set_mesh(e, meshes - 1);
                    self.machine(e).request(2);
                }
                true
            }
            Callback::BuildExit8 => {
                self.site_list.retain(|&s| s != e);
                true
            }
            Callback::BuildFinish => {
                self.finish_storage(e);
                self.sites.get_mut(&e).unwrap().complete = true;
                false
            }
            Callback::ResidenceEnter2 => {
                self.workforce.residences.push(e);
                false
            }
        }
    }
}

impl WorkHost for Sim<'_, '_> {
    fn profession(&self, c: usize) -> usize {
        self.states[c].profession
    }
    fn workplace(&self, c: usize) -> Option<usize> {
        self.states[c].workplace
    }
    fn cell(&self, c: usize) -> i32 {
        self.citizen_cell(c)
    }
    fn has_emergency(&self, _: usize) -> bool {
        // Not ported: emergencies (fires).
        false
    }
    fn works_anywhere(&self, p: usize) -> bool {
        self.professions.works_anywhere(p)
    }
    fn rank(&mut self, a: i32, b: i32) -> i32 {
        self.rank_or_distance(a, b)
    }
    fn stock(&self, slot: usize) -> (i32, i32) {
        self.stock[slot]
    }
    fn laborer(&self) -> usize {
        self.professions.laborer
    }
    fn builder(&self) -> usize {
        self.professions.builder
    }
    fn rng(&mut self) -> &mut Random {
        self.rng
    }
    fn take_events(&mut self) -> Vec<JobEvent> {
        std::mem::take(&mut self.events)
    }

    /// `FUN_1400f1020` / `FUN_1400ea0b0`.
    fn refresh(&mut self, work: &mut Work) {
        let id = work.id;
        let gone: Vec<usize> = match &work.kind {
            WorkKind::Gather(g) => g.workers.iter().copied(),
            WorkKind::Build(b) => b.workers.iter().copied(),
            WorkKind::Test(_) => return,
        }
        .filter(|&c| self.characters[c].stack.holds_task(id).is_none())
        .collect();
        if gone.is_empty() {
            return;
        }
        match &mut work.kind {
            WorkKind::Gather(g) => {
                for c in &gone {
                    g.workers.remove(c);
                }
            }
            WorkKind::Build(b) => {
                for c in &gone {
                    b.workers.remove(c);
                }
                let held: Vec<i32> = b
                    .workers
                    .iter()
                    .filter_map(|&c| self.characters[c].stack.holds_task(id))
                    .collect();
                for (k, o) in b.occupied.iter_mut().enumerate() {
                    *o = held.contains(&(k as i32));
                }
            }
            WorkKind::Test(_) => {}
        }
        if let WorkKind::Gather(_) = work.kind {
            self.cancel(work);
            if let WorkKind::Gather(g) = &mut work.kind {
                g.transit.iter_mut().for_each(|t| *t = 0);
            }
        }
    }

    fn try_assign(&mut self, work: &mut Work, c: usize, time: i64) -> u8 {
        match work.kind {
            WorkKind::Gather(_) => self.gather_try(work, c, time),
            WorkKind::Build(_) => self.build_try(work, c),
            WorkKind::Test(_) => 0,
        }
    }

    fn complete(&mut self, work: &mut Work, c: usize, param: i32) -> bool {
        match &mut work.kind {
            WorkKind::Gather(g) => {
                g.workers.remove(&c);
                let i = (param & 0xff) as usize;
                if let Some(t) = g.transit.get_mut(i) {
                    *t -= param >> 8;
                }
                if let Some(s) = g.stored.get_mut(i) {
                    *s += param >> 8;
                }
            }
            WorkKind::Build(b) => {
                b.workers.remove(&c);
                if let Some(o) = b.occupied.get_mut(param as usize) {
                    *o = false;
                }
            }
            WorkKind::Test(_) => {}
        }
        false
    }

    fn fail(&mut self, work: &mut Work, c: usize, param: i32) {
        match &mut work.kind {
            WorkKind::Gather(g) => {
                g.workers.remove(&c);
                if let Some(t) = g.transit.get_mut((param & 0xff) as usize) {
                    *t -= param >> 8;
                }
            }
            WorkKind::Build(b) => {
                b.workers.remove(&c);
                if let Some(o) = b.occupied.get_mut(param as usize) {
                    *o = false;
                }
            }
            WorkKind::Test(_) => {}
        }
    }

    /// `FUN_1400f1950` / `FUN_1400eac70`: clear every worker's stack (their aborts leave the
    /// set through the fail slot).
    fn cancel(&mut self, work: &mut Work) {
        loop {
            let next = match &work.kind {
                WorkKind::Gather(g) => g.workers.first().copied(),
                WorkKind::Build(b) => b.workers.first().copied(),
                WorkKind::Test(_) => None,
            };
            let Some(c) = next else { return };
            self.clear_citizen(c, Some(work));
            // A worker without the task would loop forever in the original; leave it.
            match &mut work.kind {
                WorkKind::Gather(g) => {
                    g.workers.remove(&c);
                }
                WorkKind::Build(b) => {
                    b.workers.remove(&c);
                }
                WorkKind::Test(_) => {}
            }
        }
    }
}

impl Sim<'_, '_> {
    /// `FUN_1400f1310`: GatherRawMaterials' slot `+0x20`.
    fn gather_try(&mut self, work: &mut Work, c: usize, time: i64) -> u8 {
        let e = work.entity;
        if !self.workforce.workplaces.get(&e).is_some_and(|w| w.enabled) {
            return 1;
        }
        let cell = self.citizen_cell(c);
        let site = self.sites[&e].clone();
        let materials = self.site_templates[site.template].materials.clone();
        for (i, &(count, material)) in materials.iter().enumerate() {
            let need = count * site.stages;
            let WorkKind::Gather(g) = &work.kind else {
                return 0;
            };
            if g.transit[i] + g.stored[i] >= need {
                continue;
            }
            let found = {
                let town = TownView {
                    size: self.size,
                    types: &self.cells.types,
                    regions: self.regions,
                    occupants: &self.cells.occupants,
                    objects: &self.cells.objects,
                    entities: self.entities,
                    happiness: &[],
                };
                let (size, terrain) = (self.size, self.terrain);
                let ground = move |cell: i32| cell_ground(size, terrain, cell);
                let mut geometry = StorageGeometry {
                    town: &town,
                    sectors: self.sectors,
                    ground: &ground,
                };
                find_material_storage(
                    self.storage_sites,
                    self.storages,
                    self.materials,
                    material,
                    1,
                    false,
                    cell,
                    None,
                    &mut geometry,
                    self.rng,
                )
            };
            let Some(found) = found else { continue };
            let mut amount = need - g.transit[i] - g.stored[i];
            let size = self.materials[material].size.max(1);
            let carried = &self.states[c].carried;
            let free = carried.capacity - carried.used(self.materials);
            if free < size * amount {
                amount = free / size;
            }
            if amount == 0 {
                // Not ported: the drop-off activity (`FUN_14011b330`).
                return 2;
            }
            amount = amount.min(self.storages[found.storage].available(material));
            // `FUN_1401280b0`: the nearest reachable build point to the source.
            let source_cell = self.cell_of(found.target);
            let mut best = 0x7fffffff;
            let mut target = None;
            for k in 0..self.build_point_count(e) {
                let p = self.build_point(e, k);
                let pc = self.cell_of(p);
                if self.reachable(source_cell, pc) {
                    let r = self.rank_or_distance(source_cell, pc);
                    if r < best {
                        best = r;
                        target = Some(p);
                    }
                }
            }
            let Some(target) = target.filter(|_| best < 0xffff) else {
                return 1;
            };
            let target_cell = self.cell_of(target);
            if !self.patience(work, c, target_cell, cell, time) {
                return 0;
            }
            let source_entity = self.storage_sites[found.site].entity;
            self.haul_sequence(
                c,
                work,
                e,
                target,
                found.storage,
                source_entity,
                found.target,
                material,
                amount,
                (amount << 8) | i as i32,
            );
            self.took_job(c, work);
            if let WorkKind::Gather(g) = &mut work.kind {
                g.workers.insert(c);
                g.transit[i] += amount;
            }
            return 2;
        }
        1
    }

    /// `FUN_1400ea490`: BuildStructure's slot `+0x20`.
    fn build_try(&mut self, work: &mut Work, c: usize) -> u8 {
        let e = work.entity;
        let Some(wp) = self.workforce.workplaces.get(&e) else {
            return 1;
        };
        if !wp.enabled {
            return 1;
        }
        let points = self.build_point_count(e) as i32;
        let WorkKind::Build(b) = &work.kind else {
            return 0;
        };
        if points <= b.workers.len() as i32 {
            return 1;
        }
        if wp.workers.len() as i32 == wp.max_workers && !wp.workers.contains(&c) {
            return 1;
        }
        let free = b.occupied.iter().filter(|&&o| !o).count();
        let cell = self.citizen_cell(c);
        let mut chosen = -1i32;
        for _ in 0..free {
            let WorkKind::Build(b) = &mut work.kind else {
                return 0;
            };
            let n = b.occupied.len() as i32;
            if n > 0 {
                let mut tries = 0;
                loop {
                    let k = b.cursor;
                    if !b.occupied[k as usize] {
                        chosen = k;
                    }
                    b.cursor = if k + 1 >= n { 0 } else { k + 1 };
                    tries += 1;
                    if chosen >= 0 || tries >= n {
                        break;
                    }
                }
            }
            let point = self.build_point(e, chosen.max(0) as usize);
            let pc = self.cell_of(point);
            if pc != 0 && self.reachable(cell, pc) {
                // Every other point's connectivity and node distance only fill the sector
                // distance cache in the original.
                let _ = self.rank_raw(cell, pc);
                for k in 0..points {
                    if k == chosen {
                        continue;
                    }
                    let other = self.cell_of(self.build_point(e, k as usize));
                    if self.reachable(cell, other) {
                        let _ = self.rank_raw(cell, other);
                    }
                }
                self.build_sequence(c, work, e, point, chosen);
                self.took_job(c, work);
                if let WorkKind::Build(b) = &mut work.kind {
                    b.workers.insert(c);
                    b.occupied[chosen as usize] = true;
                }
                return 2;
            }
        }
        1
    }
}
