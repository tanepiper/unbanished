//! The simulation context jobs and construction run in: [`Sim`] implements the job manager's
//! work slots ([`WorkHost`]) and the building state callbacks ([`Callbacks`]), over the
//! citizens, the map, the storages and the construction state. See docs/JOBS.md.

use std::collections::{BTreeMap, BTreeSet};

use banished_formats::types::AnimationGroup;

use crate::activities::{EntityView, StorageGeometry, TownView};
use crate::agents::{CitizenRulesAi, CitizenState};
use crate::character::{Action, Character, Component, Message, MoveTo};
use crate::construction::{
    run, BuildWork, Callback, Callbacks, ClearWork, Gather, Machine, PickupWork, Pile, Site,
    SiteTemplate,
};
use crate::jobs::{JobEvent, Jobs, Work, WorkHost, WorkKind, NESTED};
use crate::pathfind::nearest_neighbour;
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

/// [`Sim::haul`]'s endpoints and flags.
#[derive(Default)]
pub(crate) struct Haul {
    pub target_entity: usize,
    pub target_storage: usize,
    pub target: [f32; 3],
    pub source_storage: usize,
    pub source_entity: usize,
    pub source: [f32; 3],
    pub material: usize,
    pub amount: i32,
    pub param: i32,
    pub partial: bool,
    pub complete_on_pickup: bool,
    /// `param_14`: the hauler uses a tool (its work steps spend one, its loaded walk is
    /// clip 5).
    pub tool: bool,
    /// Several materials at once (`material` null, `FUN_1400cf700` adds each to the pickup):
    /// the delivery drops everything carried.
    pub items: Vec<(usize, i32)>,
}

/// The world the citizens' update borrows (everything but the citizens themselves).
pub struct Env<'a, 't> {
    /// Wild animals (hunting).
    pub wildlife: &'a mut crate::wildlife::Wildlife,
    pub cells: &'a mut Cells,
    pub placed: &'a mut Vec<Placed>,
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
    pub placed: &'s mut Vec<Placed>,
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
    pub homes: &'s mut BTreeMap<usize, crate::life::Home>,
    pub attendance: &'s mut BTreeMap<usize, crate::construction::Attendance>,
    pub attendance_list: &'s mut Vec<usize>,
    pub healers: &'s mut Vec<usize>,
    pub happiness: &'s mut Vec<usize>,
    /// The calendar's seconds into the year.
    pub calendar: f32,
    pub year: u32,
    pub fruits: &'s mut BTreeMap<usize, crate::orchard::Fruit>,
    /// The animals (livestock and their pastures).
    pub wildlife: &'s mut crate::wildlife::Wildlife,
    pub keep_piles: &'s mut BTreeMap<usize, bool>,
    pub site_events: &'s mut Vec<(i32, i32, usize)>,
    pub storage_changes: &'s mut Vec<(usize, bool)>,
    pub rebuilds: &'s mut Vec<crate::destroy::Rebuild>,
    /// The town's water sources (wells and shore points).
    pub water: &'s mut crate::fire::Water,
    /// `Game +0xd0`: the debug flags (all of `0xf0` with disasters off).
    pub game_flags: u32,
    /// The sky's state is precipitation (environment `+0x10` = 0).
    pub precipitation: bool,
    /// Town manager `+0xa8`.
    pub population: i32,
    /// Town manager `+0x1a8` / `+0x1a0`.
    pub visitors: &'s mut (f32, i64),
    /// Map `+0xe8`: planned road cells → BuildRoad works.
    pub pending_roads: &'s mut BTreeMap<i32, i32>,
    pub terraform_requests: &'s mut Vec<crate::terraform::Terraform>,
    pub tornadoes: &'s mut BTreeMap<usize, crate::tornado::Tornado>,
    pub tornado_kills: &'s mut Vec<usize>,
    pub trackers: &'s mut BTreeMap<usize, crate::tracker::Tracker>,
    pub counters: &'s mut crate::tracker::StockCounters,
    /// Town manager `+0x30` and `+0x1ac`.
    pub trading_posts: &'s mut Vec<usize>,
    pub trade_value: &'s mut i32,
    /// Town manager `+0x110` (known raw materials, `FUN_140097330`) and `+0x120` (known crops,
    /// orchard trees and livestock, `FUN_140096ef0`).
    pub known_materials: &'s mut Vec<usize>,
    pub known: &'s mut Vec<usize>,
    pub trade_goods: &'s [crate::trade::TradeGood],
    pub river: &'s [[f32; 2]],
    /// The path manager (the trader boats' water paths).
    pub pathfinder: &'s mut crate::pathfind::Pathfinder,
    /// The citizens' name databases (the merchants' names).
    pub names: Option<&'s (
        banished_formats::names::NameDatabase,
        banished_formats::names::NameDatabase,
    )>,
    /// The scheduled update's elapsed time (ticks) for the site callbacks.
    pub site_dt: i64,
    pub cemeteries: &'s mut Vec<usize>,
    pub tombstones: &'s mut BTreeMap<usize, i64>,
    pub burials: &'s mut Vec<(usize, i32)>,
    pub site_list: &'s mut Vec<usize>,
    pub events: Vec<JobEvent>,
    pub piles: &'s mut BTreeMap<usize, Pile>,
    pub claimed: &'s mut std::collections::BTreeSet<usize>,
    pub new_piles: &'s mut Vec<usize>,
    pub models_dirty: &'s mut bool,
    /// Works a ClearArea try asked the manager to offer ([`WorkHost::take_offers`]).
    pub offers: Vec<i32>,
    /// Scratch for [`Sim::seeder`] (planting removes nothing and plays no sounds).
    pub removed: Vec<usize>,
    pub animation_events: Vec<(usize, String)>,
    /// The offers get a second try (`FUN_1400fdaf0(.., 1)`, the AutoPickup's).
    pub offers_again: bool,
    /// A ConsumeProduce try waiting on its pile offers ([`WorkHost::resume`]).
    pub(crate) produce_resume: Option<crate::produce::ProduceResume>,
    /// A pasture try waiting on its product pickups' offers ([`Sim::pasture_try`]).
    pub(crate) pasture_stage: bool,
}

/// `FUN_14012e950`: a pile's work category mask from its material's flags.
pub fn pile_mask(flags: u32) -> u32 {
    for (bit, mask) in [
        (5, 2),
        (6, 4),
        (10, 0x10),
        (12, 0x40),
        (11, 0x20),
        (13, 0x100),
        (16, 0x200),
        (9, 8),
    ] {
        if flags >> bit & 1 != 0 {
            return mask;
        }
    }
    if flags >> 14 & 1 != 0 {
        0x80
    } else {
        1
    }
}

/// `FUN_14011ce30`'s clearing mask bit for a resource's raw material: logs `0x400`, iron
/// `0x1000`, stone `0x800`, else 1.
fn clear_mask(flags: u32) -> u32 {
    if flags >> 10 & 1 != 0 {
        0x400
    } else if flags >> 12 & 1 != 0 {
        0x1000
    } else if flags >> 11 & 1 != 0 {
        0x800
    } else {
        1
    }
}

/// `MapComponent::IsAnimating` per placed entity.
pub fn animating(placed: &[Placed], templates: &[Template]) -> Vec<bool> {
    placed
        .iter()
        .map(|p| {
            p.alive
                && p.animation.as_ref().is_some_and(|pb| {
                    templates[p.template]
                        .animations
                        .get(p.variant)
                        .and_then(Option::as_ref)
                        .and_then(|g| g.clips.get(pb.clip))
                        .is_some_and(|clip| pb.is_animating(clip))
                })
        })
        .collect()
}

/// FUN_1400a48b0 without bridge picking.
pub fn cell_ground(size: usize, terrain: &HeightGrid, cell: i32) -> [f32; 3] {
    let n = size as i32;
    let h = (n >> 1) as f32 - 0.5;
    let (x, y) = ((cell % n) as f32 - h, (cell / n) as f32 - h);
    let mut normal = [0.0; 3];
    [x, y, terrain.ground(x, y, &mut normal)]
}

impl Sim<'_, '_> {
    pub(crate) fn cell_of(&self, p: [f32; 3]) -> i32 {
        let n = self.size as i32;
        let half = (n >> 1) as f32;
        let x = ((p[0] + half) as i32).clamp(0, n - 1);
        let y = ((p[1] + half) as i32).clamp(0, n - 1);
        n * y + x
    }

    pub(crate) fn citizen_cell(&self, c: usize) -> i32 {
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
            happiness: self.happiness,
        }
    }

    pub(crate) fn reachable(&self, a: i32, b: i32) -> bool {
        self.town().reachable(WALKABLE, a, b)
    }

    /// `Map::Reachable` (`FUN_1400a55c0`) with another mask.
    pub(crate) fn reachable_by(&self, mask: u32, a: i32, b: i32) -> bool {
        self.town().reachable(mask, a, b)
    }

    /// The workforce with the workers and the workplace searches over the town; the workers'
    /// professions and workplaces are written back afterwards.
    pub(crate) fn with_search<R>(
        &mut self,
        f: impl FnOnce(
            &mut crate::professions::Workforce,
            &mut [crate::professions::Worker],
            &mut dyn crate::professions::Search,
        ) -> R,
    ) -> R {
        let mut workers = self.workers();
        let town = TownView {
            size: self.size,
            types: &self.cells.types,
            regions: self.regions,
            occupants: &self.cells.occupants,
            objects: &self.cells.objects,
            entities: self.entities,
            happiness: self.happiness,
        };
        let (size, characters) = (self.size as i32, &*self.characters);
        let cell = |c: usize| {
            let p = characters[c].position;
            let half = (size >> 1) as f32;
            let x = ((p[0] + half) as i32).clamp(0, size - 1);
            let y = ((p[1] + half) as i32).clamp(0, size - 1);
            size * y + x
        };
        let professions = self.professions;
        let anywhere = |p: usize| professions.works_anywhere(p);
        let sites = &*self.sites;
        let priority = |e: usize| {
            sites
                .get(&e)
                .and_then(|s| s.work_priority)
                .unwrap_or(i32::MAX)
        };
        let mut search = crate::activities::TownSearch {
            town: &town,
            cell: &cell,
            anywhere: &anywhere,
            priority: &priority,
            rng: &mut *self.rng,
        };
        let r = f(&mut *self.workforce, &mut workers, &mut search);
        self.set_workers(&workers);
        r
    }

    /// A storage's drop point nearest `from` (`FUN_140132aa0` through the storage search's
    /// geometry).
    pub(crate) fn storage_point(&mut self, entity: usize, from: i32) -> Option<[f32; 3]> {
        let town = TownView {
            size: self.size,
            types: &self.cells.types,
            regions: self.regions,
            occupants: &self.cells.occupants,
            objects: &self.cells.objects,
            entities: self.entities,
            happiness: self.happiness,
        };
        let (size, terrain) = (self.size, self.terrain);
        let ground = move |c: i32| cell_ground(size, terrain, c);
        let mut geometry = StorageGeometry {
            town: &town,
            sectors: self.sectors,
            ground: &ground,
        };
        crate::stock::StorageSearchMap::storage_position(&mut geometry, entity, from)
            .map(|(_, p)| p)
    }

    /// `FUN_1401350d0` on `e`'s workplace: its workers move on when it is switched off.
    pub(crate) fn enable_workplace(&mut self, e: usize, on: bool) {
        // The state machine's pending state, else its state (`+0x52`, `+0x50`): 0, 1 or 8
        // is being built.
        let building = self.sites.get(&e).is_some_and(|s| {
            let state = if s.machine.pending < 0 {
                s.machine.state
            } else {
                s.machine.pending
            } as u8;
            state & 0xf6 == 0 && state != 9
        });
        self.with_search(|w, workers, search| w.set_enabled(workers, e, on, building, search));
    }

    /// `FUN_1401318c0` on `e`'s StatusIconComponent (its workplace's).
    pub(crate) fn set_icon(&mut self, e: usize, bit: u8, on: bool) {
        if let Some(w) = self.workforce.workplaces.get_mut(&e) {
            w.set_icon(bit, on);
        }
    }

    /// `FUN_1400a4ce0(map, a, b, 1)`: −1 when not connected.
    fn rank_raw(&mut self, a: i32, b: i32) -> i32 {
        let reachable = self.reachable(a, b);
        self.sectors.rebuild(&self.cells.types);
        self.sectors
            .distance(a, b, reachable, true, &self.cells.types)
    }

    /// Path rank with the callers' fallback `n + (int)sqrtf(dx² + dy²)`.
    pub(crate) fn rank_or_distance(&mut self, a: i32, b: i32) -> i32 {
        let d = self.rank_raw(a, b);
        if d >= 0 {
            return d;
        }
        let n = self.size as i32;
        let (dx, dy) = ((a % n - b % n) as f32, (a / n - b / n) as f32);
        n + (dx * dx + dy * dy).sqrt() as i32
    }

    /// An entity's model-space point in the world (`ModelComponent` transform).
    pub(crate) fn world_point(&self, entity: usize, local: [f32; 3]) -> [f32; 3] {
        let p = &self.placed[entity];
        let (s, c) = p.yaw.to_radians().sin_cos();
        [
            p.position[0] + c * local[0] - s * local[1],
            p.position[1] + s * local[0] + c * local[1],
            p.position[2] + local[2],
        ]
    }

    /// The entity, then its MultiObject parts: whose InteractComponents hold its points (the
    /// children `FUN_140128540` and `FUN_1401282f0` count on).
    pub(crate) fn interact_entities(&self, entity: usize) -> Vec<usize> {
        let mut out = vec![entity];
        if let Some(site) = self.sites.get(&entity) {
            out.extend(site.parts.iter().copied());
        }
        out
    }

    /// `FUN_1401282f0(interact, 1, i)`: build point `i` in the world (the entity's own, then
    /// its parts').
    fn build_point(&self, entity: usize, i: usize) -> [f32; 3] {
        let mut i = i;
        for x in self.interact_entities(entity) {
            let points = &self.templates[self.placed[x].template].build_points;
            if i < points.len() {
                return self.world_point(x, points[i]);
            }
            i -= points.len();
        }
        self.world_point(entity, [0.0; 3])
    }

    pub(crate) fn build_point_count(&self, entity: usize) -> usize {
        self.interact_entities(entity)
            .iter()
            .map(|&x| self.templates[self.placed[x].template].build_points.len())
            .sum()
    }

    /// The point `FUN_1400c7e70` faces: the model's position, else the map-rectangle centre.
    pub(crate) fn facing_centre(&self, entity: usize) -> [f32; 3] {
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
    pub(crate) fn rect_cell(&self, entity: usize) -> i32 {
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
    pub(crate) fn clear_citizen(&mut self, c: usize, mut current: Option<&mut Work>) {
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

    pub(crate) fn push(&mut self, c: usize, action: Action) {
        let groups = self.groups;
        let Some(group) = groups.get(c).copied().flatten() else {
            return;
        };
        let ch = &mut self.characters[c];
        let mut stack = std::mem::take(&mut ch.stack);
        stack.push(ch, group, action);
        ch.stack = stack;
    }

    pub(crate) fn on_empty_think(&mut self, c: usize) {
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
    pub(crate) fn perform(
        &self,
        c: usize,
        facing: f32,
        anim: usize,
        secs: f32,
        tool: bool,
    ) -> Action {
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
    pub(crate) fn took_job(&mut self, c: usize, work: &Work) {
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
        let site_storage = self.sites[&site].storage;
        self.haul(
            c,
            work,
            Haul {
                target_entity: site,
                target_storage: site_storage,
                target,
                source_storage,
                source_entity,
                source,
                material,
                amount,
                param,
                partial: false,
                complete_on_pickup: false,
                ..Default::default()
            },
        );
    }

    /// `FUN_1400c4c60` with its flags: `partial` (`param_11`, the pickup may take less),
    /// `complete_on_pickup` (`param_12`, the CompleteTask follows the pickup rather than the
    /// delivery). Not ported: `param_13` (a Citizen message) and `param_14` (tool use, faster
    /// walk), which no ported caller sets.
    pub(crate) fn haul(&mut self, c: usize, work: &mut Work, h: Haul) {
        let Haul {
            target_entity: site,
            target_storage: site_storage,
            target,
            source_storage,
            source_entity,
            source,
            material,
            amount,
            param,
            partial,
            complete_on_pickup,
            tool,
            items,
        } = h;
        self.clear_citizen(c, Some(work));
        self.on_empty_think(c);
        let face_target = crate::character::work_facing(self.facing_centre(site), target);
        let face_source = crate::character::work_facing(self.facing_centre(source_entity), source);
        self.push(c, self.perform(c, face_target, 7, 0.75, tool));
        self.push(c, Action::ShowTool(-1));
        if !complete_on_pickup {
            self.push(
                c,
                Action::CompleteTask {
                    work: work.id,
                    param,
                },
            );
        }
        let several = !items.is_empty();
        self.push(
            c,
            Action::DropOff {
                storage: site_storage,
                material: if several { usize::MAX } else { material },
                count: if several { -1 } else { amount },
            },
        );
        self.push(c, self.perform(c, face_target, 6, 0.75, tool));
        let walk = MoveTo::new(
            target,
            4 + usize::from(tool),
            0,
            WALKABLE,
            0,
            -1.0,
            self.rng,
        );
        self.push(c, Action::move_to(walk));
        self.push(c, self.perform(c, face_source, 7, 0.75, tool));
        self.push(c, Action::ShowTool(-2));
        // `FUN_1400cf9f0` reserves at construction; without a count, as much as the citizen
        // carries, capped at the unreserved stock.
        let pickup = if several {
            for &(m, n) in &items {
                self.storages[source_storage].reserve(m, n);
            }
            items
        } else {
            let amount = if amount < 0 {
                let carried = &self.states[c].carried;
                (carried.capacity / self.materials[material].size.max(1))
                    .min(self.storages[source_storage].available(material))
            } else {
                amount
            };
            self.storages[source_storage].reserve(material, amount);
            vec![(material, amount)]
        };
        self.push(
            c,
            Action::PickupStoredRawMaterial {
                storage: source_storage,
                items: pickup,
                partial,
            },
        );
        if complete_on_pickup {
            self.push(
                c,
                Action::CompleteTask {
                    work: work.id,
                    param,
                },
            );
        }
        self.push(c, self.perform(c, face_source, 6, 0.75, tool));
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
        // A destroy work messages the DestroyComponent instead.
        let destroy = matches!(&work.kind, WorkKind::Build(b) if b.destroy);
        self.push(
            c,
            Action::SendMessage(Message {
                component: if destroy {
                    Component::Destroy
                } else {
                    Component::Build
                },
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
    pub(crate) fn patience(
        &mut self,
        work: &mut Work,
        c: usize,
        a: i32,
        b: i32,
        time: i64,
    ) -> bool {
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
    pub(crate) fn is_clear(&self, entity: usize) -> bool {
        let p = &self.placed[entity];
        let n = self.size as i32;
        let (x, y) = p.cell;
        let (w, h) = p.footprint;
        let cells =
            || (y..y + h).flat_map(move |cy| (x..x + w).map(move |cx| (cy * n + cx) as usize));
        let desc = self
            .sites
            .get(&entity)
            .and_then(|s| self.site_templates[s.template].clear.clone());
        // `FUN_1400a2d30` (clearing cells): no road, planned road or road plan.
        let roads_clear = !desc.as_ref().is_some_and(|d| d.clear_cells)
            || cells().all(|i| {
                let t = self.cells.types[i].0;
                t != 0
                    && t != 1
                    && self.cells.road_kind[i] == crate::road::NONE
                    && !self.pending_roads.contains_key(&(i as i32))
            });
        if !roads_clear {
            return false;
        }
        match desc {
            // `FUN_1400a2e00`: no natural resource (of the filter's template).
            Some(d) if d.clear_resources && !d.clear_piles => cells().all(|i| {
                self.cells.objects[i].is_none_or(|o| {
                    let t = &self.templates[self.placed[o].template];
                    t.natural.is_none()
                        || d.reference.as_deref().is_some_and(|f| {
                            !t.reference.eq_ignore_ascii_case(&f.replace('/', "\\"))
                        })
                })
            }),
            // `FUN_1400a2f90`: no object.
            _ => cells().all(|i| self.cells.objects[i].is_none()),
        }
    }

    /// `FUN_14010db00`: the site storage holds every requirement.
    fn has_materials(&self, entity: usize) -> bool {
        let site = &self.sites[&entity];
        let t = &self.site_templates[site.template];
        t.materials
            .iter()
            .all(|&(n, m)| self.storages[site.storage].available(m) >= n * site.stages)
    }

    /// `FUN_1401349f0` (WorkPlaceComponent::BecomeSite): the workers leave and builders work
    /// here.
    pub(crate) fn become_site(&mut self, jobs: &mut Jobs, e: usize) {
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
            // Nobody to move: the workers were let go above.
            self.set_workers(&workers);
            self.with_search(|w, ws, search| w.set_max_workers(ws, e, max, search));
            workers = self.workers();
            // `FUN_1401350d0(1)`: the builders' profession is not its own, so no
            // workers icon.
            let w = self.workforce.workplaces.get_mut(&e).unwrap();
            w.enabled = true;
            w.set_icon(crate::professions::icon::WORK_DISABLED, false);
            w.set_icon(crate::professions::icon::BUILD_PAUSED, false);
        }
        self.set_workers(&workers);
    }

    /// `FUN_140134b40` (WorkPlaceComponent::BecomeBuilding): back to its own profession.
    pub(crate) fn become_building(&mut self, jobs: &mut Jobs, e: usize) {
        let builder = self.professions.builder;
        let mut workers = self.workers();
        let template = self.sites[&e].template;
        let id = self.sites[&e].work_id;
        if let Some(w) = self.workforce.workplaces.get(&e) {
            if w.profession == Some(builder) {
                self.workforce.unregister(e, builder);
                let desc = self.site_templates[template].profession;
                let footprint = self.placed[e].footprint;
                let max = self.site_templates[template].default_max_workers(footprint);
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
    }

    /// `FUN_140134130`: register a work for the site's WorkComponent, replacing any.
    pub(crate) fn set_work(
        &mut self,
        jobs: &mut Jobs,
        entity: usize,
        work: Work,
        mask: u32,
        keep: bool,
    ) {
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

    /// `FUN_140134020`: keep the priority, remove the work; the limit and work icons go.
    pub(crate) fn remove_work(&mut self, jobs: &mut Jobs, entity: usize) {
        let id = self.sites[&entity].work_id;
        if let Some(w) = jobs.works.get(&id) {
            self.sites.get_mut(&entity).unwrap().work_priority = Some(w.priority);
            jobs.remove(id, self);
            self.set_icon(entity, crate::professions::icon::LIMIT, false);
            self.set_icon(entity, crate::professions::icon::NO_WORK, false);
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

    /// `FUN_14010d4c0`: the materials built in count as used (`+0xc`); the building's own
    /// storage replaces the material store.
    fn finish_storage(&mut self, entity: usize) {
        let site = &self.sites[&entity];
        let items: Vec<(usize, i32)> = self.storages[site.storage]
            .items
            .iter()
            .map(|i| (i.material, i.count))
            .collect();
        for (m, n) in items {
            self.count_used(m, n);
        }
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
    pub(crate) fn stamp(&mut self, entity: usize) {
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

    /// `FUN_140129c20` → `FUN_1400a33b0`: the MapComponent's footprint is lifted. Stamped
    /// cells (`#`, `_`, or all with an empty footprint) get their natural type back (ground
    /// where water lies under another occupant), and every cell's object is cleared.
    pub(crate) fn unstamp(&mut self, entity: usize) {
        let p = &self.placed[entity];
        let Some(map) = &self.templates[p.template].map else {
            return;
        };
        let n = self.size as i32;
        let (x0, y0) = p.cell;
        let (w, h) = p.footprint;
        let fp = map.footprint.as_bytes();
        let degrees = p.yaw as i32;
        for y in y0..y0 + h {
            for x in x0..x0 + w {
                let i = (n * y + x) as usize;
                let stamped = fp.is_empty() || {
                    let c = fp
                        .get(crate::placement::footprint_index(
                            x - x0,
                            y - y0,
                            w,
                            h,
                            degrees,
                        ))
                        .copied()
                        .unwrap_or(b'.');
                    c == b'#' || c == b'_'
                };
                if stamped {
                    let mut ty = self.terrain.natural_cell_type(self.size, i);
                    let water = (1u32 << (ty.0 & 0x1f)) & 0x1e0 != 0;
                    if water && self.cells.occupants[i].is_some_and(|o| o != entity) {
                        ty = crate::terrain::CellType(2);
                    }
                    self.cells.types[i] = ty;
                }
                self.cells.objects[i] = None;
            }
        }
    }

    /// Activate the sites placed since the last iteration (`FUN_1400c9d70` → activate).
    pub fn activate(&mut self, jobs: &mut Jobs, entity: usize) {
        self.trade_activate(entity);
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
            if !self.sites.get(&e).is_some_and(|s| s.destroyed) {
                run(self, jobs, e, true, true);
            }
        }
        let destroyed: Vec<usize> = self
            .sites
            .iter()
            .filter(|(_, s)| s.destroyed)
            .map(|(&e, _)| e)
            .collect();
        for e in destroyed {
            self.destroy_entity(jobs, e);
        }
    }

    /// The entity manager's removal of a destroyed site entity (`FUN_1400c8370`, then each
    /// component's remove slot): its work goes (citizens doing it abort), the ZoneComponent
    /// frees its cells and the entity is gone.
    /// Not ported: WorkPlace and Residence removal (only removal markers are destroyed yet).
    pub fn destroy_entity(&mut self, jobs: &mut Jobs, e: usize) {
        if !self.sites.contains_key(&e) {
            return;
        }
        self.remove_work(jobs, e);
        // The components' removal slots (`FUN_14004b380`, `FUN_14012ccb0`).
        self.happiness_leave(e);
        // A torn-down building's WorkPlace and Residence components (they left the town's
        // registries in state 3).
        if self.workforce.workplaces.contains_key(&e) {
            let mut workers = self.workers();
            self.workforce.clear_workers(&mut workers, e);
            self.set_workers(&workers);
            if let Some(p) = self.workforce.workplaces.get(&e).and_then(|w| w.profession) {
                self.workforce.unregister(e, p);
            }
            self.workforce.workplaces.remove(&e);
        }
        if self.homes.contains_key(&e) {
            self.residence_leave(e);
        }
        if self.wildlife.pastures.contains_key(&e) {
            self.pasture_leave(e);
            self.wildlife.pastures.remove(&e);
        }
        // MultiObjectComponent's (`FUN_14012ac90`): its parts go too, lifting their stamps and
        // their floors (`FUN_140124770`).
        for part in self.sites[&e].parts.clone() {
            self.unstamp(part);
            let size = self.size;
            self.cells.floors.remove(size, part);
            self.placed[part].alive = false;
        }
        // TerraformComponent removal (`FUN_140057cb0`): the entity's, then its parts'.
        let parts = self.sites[&e].parts.clone();
        for x in std::iter::once(e).chain(parts) {
            if self.templates[self.placed[x].template].terraform.is_some() {
                self.terraform_requests
                    .push(crate::terraform::Terraform::Undo(x));
            }
        }
        self.sites.remove(&e);
        self.site_list.retain(|&s| s != e);
        let n = self.size as i32;
        let p = &mut self.placed[e];
        p.alive = false;
        let (x, y) = p.cell;
        let (w, h) = p.footprint;
        for cy in y..y + h {
            for cx in x..x + w {
                let i = (cy * n + cx) as usize;
                if self.cells.occupants[i] == Some(e) {
                    self.cells.occupants[i] = None;
                }
            }
        }
        *self.models_dirty = true;
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
                // `FUN_14011ce30`. Not ported: the instant clear (`FUN_14011d230`, description
                // `+0x2d` or the game's instant flag); no shipped site sets it.
                if self.is_clear(e) {
                    self.machine(e).request(1);
                } else {
                    self.register_clear(jobs, e);
                }
                true
            }
            Callback::ClearUpdate0 => {
                if self.is_clear(e) {
                    self.machine(e).request(1);
                }
                false
            }
            Callback::ClearExit0 => {
                // `FUN_14011cd60`: remove the ClearArea work.
                if self.sites[&e].work_id >= 0 && jobs.works.contains_key(&self.sites[&e].work_id) {
                    self.remove_work(jobs, e);
                }
                true
            }
            // `FUN_14011cc40`: the ClearArea work goes.
            Callback::ClearExit15 => {
                let id = self.sites[&e].work_id;
                if id >= 0 && jobs.works.contains_key(&id) {
                    self.remove_work(jobs, e);
                }
                true
            }
            Callback::OrchardEnter2 => {
                self.orchard_enter2(e);
                false
            }
            Callback::OrchardUpdate2 => {
                self.orchard_update2(e);
                true
            }
            Callback::OrchardEnter9 => {
                self.orchard_enter9(jobs, e);
                true
            }
            Callback::OrchardUpdate9 => {
                self.orchard_update9(jobs, e);
                true
            }
            Callback::OrchardEnter10 => {
                self.orchard_enter10(jobs, e);
                true
            }
            Callback::OrchardUpdate10 => {
                self.orchard_update10(jobs, e);
                true
            }
            Callback::OrchardExitWork => {
                self.orchard_exit_work(jobs, e);
                true
            }
            Callback::OrchardEnter12 => {
                self.orchard_enter12(jobs, e);
                true
            }
            Callback::OrchardUpdate12 => {
                self.orchard_update12(e);
                true
            }
            Callback::OrchardEnter11 => {
                self.orchard_enter11(jobs, e);
                true
            }
            Callback::OrchardUpdate11 => {
                self.orchard_update11(jobs, e);
                true
            }
            Callback::OrchardEnter14 => {
                self.orchard_enter14(jobs, e);
                true
            }
            Callback::OrchardExit14 => {
                self.orchard_exit14(jobs, e);
                true
            }
            Callback::OrchardUpdate14 => {
                self.orchard_update14(e);
                true
            }
            Callback::CropEnter2 => {
                self.crop_enter2(e);
                false
            }
            Callback::CropUpdate2 => {
                self.crop_update2(e);
                true
            }
            Callback::CropEnter9 => {
                self.crop_enter9(jobs, e);
                true
            }
            Callback::CropUpdate9 => {
                self.crop_update9(jobs, e);
                true
            }
            Callback::CropEnter10 => {
                self.crop_enter10(jobs, e);
                true
            }
            Callback::CropExit10 => {
                self.crop_exit10(jobs, e);
                true
            }
            Callback::CropUpdate10 => {
                self.crop_update10(e);
                true
            }
            Callback::CropEnter11 => {
                self.crop_enter11(jobs, e);
                true
            }
            Callback::CropUpdate11 => {
                self.crop_update11(jobs, e);
                true
            }
            Callback::ClearEnter1 => {
                self.machine(e).request(2);
                true
            }
            Callback::ClearEnter2 => {
                // Entity event 1: the MapComponent stamps the footprint; the MultiObject passes
                // it on to the parts (`FUN_14012a960`).
                self.stamp(e);
                for part in self.sites[&e].parts.clone() {
                    self.stamp(part);
                }
                true
            }
            Callback::AbsorbUpdate0 => {
                self.absorb_piles(jobs, e);
                true
            }
            // The market's storage joins the town's markets (with its radius).
            Callback::MarketEnter2 => {
                self.storage_changes.push((e, true));
                false
            }
            Callback::MarketEnter3 => {
                self.storage_changes.push((e, false));
                false
            }
            Callback::CemeteryEnter2 => {
                self.cemetery_enter2(e);
                false
            }
            Callback::CemeteryEnter3 => {
                self.cemeteries.retain(|&c| c != e);
                false
            }
            Callback::StorageEnter2 => {
                self.storage_changes.push((e, true));
                false
            }
            Callback::StorageEnter3 => {
                self.storage_changes.push((e, false));
                false
            }
            Callback::PastureEnter2 => {
                self.pasture_enter2(e);
                false
            }
            Callback::PastureUpdate2 => {
                self.pasture_update2(jobs, e);
                true
            }
            Callback::PastureEnter3 => {
                self.pasture_leave(e);
                false
            }
            Callback::WorkPlaceEnter0 => {
                self.become_site(jobs, e);
                false
            }
            Callback::WorkPlaceEnter2 => {
                self.become_building(jobs, e);
                false
            }
            // `FUN_140134230`: every worker leaves and the workplace leaves its profession's.
            Callback::WorkPlaceEnter3 => {
                let mut workers = self.workers();
                self.workforce.clear_workers(&mut workers, e);
                self.set_workers(&workers);
                if let Some(p) = self.workforce.workplaces.get(&e).and_then(|w| w.profession) {
                    self.workforce.unregister(e, p);
                }
                false
            }
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
                // `FUN_14010cad0`: the build is no longer paused.
                self.set_icon(e, crate::professions::icon::BUILD_PAUSED, false);
                // Entity event 4 (`FUN_1400c8040`): the pasture's shelter points are cleared;
                // a TerraformComponent digs, the entity's then its MultiObject parts'
                // (`FUN_14012a960`).
                if self.wildlife.pastures.contains_key(&e) {
                    self.pasture_built(e);
                }
                let parts = self.sites[&e].parts.clone();
                for x in std::iter::once(e).chain(parts) {
                    if self.templates[self.placed[x].template].terraform.is_some() {
                        self.terraform_requests
                            .push(crate::terraform::Terraform::Dig(x));
                    }
                }
                true
            }
            Callback::BuildFinish => {
                self.finish_storage(e);
                self.sites.get_mut(&e).unwrap().complete = true;
                false
            }
            Callback::ResidenceEnter2 => {
                // `FUN_14012f2b0`: register; the component's family slots come from the
                // description (it keeps its state while out of the town's).
                self.workforce.residences.push(e);
                let site = self.sites.get_mut(&e).unwrap();
                let kept = site.home.take();
                let storage = site.storage;
                if let Some(desc) = self.site_templates[site.template].residence {
                    let home = kept.unwrap_or_else(|| crate::life::Home::new(desc, storage));
                    self.homes.entry(e).or_insert(home);
                }
                false
            }
            Callback::ResidenceEnter3 => {
                self.residence_leave(e);
                false
            }
            Callback::DestroyEnter3 => {
                self.destroy_enter3(jobs, e);
                false
            }
            Callback::DestroyEnter4 => {
                self.destroy_enter4(jobs, e);
                false
            }
            Callback::DestroyUpdate4 => {
                self.destroy_update4(jobs, e);
                true
            }
            Callback::DestroyEnter5 => {
                self.destroy_enter5(jobs, e);
                true
            }
            Callback::DestroyUpdate5 => {
                self.destroy_update5(jobs, e);
                true
            }
            Callback::DestroyEnter6 => {
                self.destroy_enter6(e);
                false
            }
            Callback::DestroyUpdate6 => {
                self.destroy_update6(e, self.site_dt);
                true
            }
            Callback::DestroyExit6 => {
                self.destroy_exit6(e);
                true
            }
            Callback::ImmigrationEnter2 => {
                self.immigration_idle(e);
                false
            }
            Callback::ImmigrationUpdate2 => {
                self.immigration_idle(e);
                true
            }
            Callback::ImmigrationEnterF => {
                self.immigration_wait(e);
                false
            }
            Callback::ImmigrationUpdateF => {
                self.immigration_waiting(e, self.site_dt);
                true
            }
            Callback::ImmigrationEnter10 => {
                self.immigration_call(e, true);
                true
            }
            Callback::ImmigrationUpdate10 => {
                self.immigration_call(e, false);
                true
            }
            Callback::ImmigrationEnter11 => {
                self.band_arrived(e);
                true
            }
            Callback::ImmigrationUpdate11 => {
                self.band_waiting(e);
                true
            }
            Callback::ImmigrationLeave => {
                self.send_band_away(e);
                true
            }
            Callback::TradeEnter2 => {
                self.trade_enter2(jobs, e);
                false
            }
            Callback::TradeUpdate2 => {
                self.trade_update2(e, self.site_dt);
                false
            }
            Callback::TradeEnter12 => true,
            Callback::TradeEnter13 => {
                self.trade_enter13(e);
                true
            }
            Callback::TradeExit13 => {
                self.trade_exit13(e);
                true
            }
            Callback::TradeUpdate13 => {
                self.trade_update13(e, self.site_dt);
                true
            }
            Callback::TradeLeave => {
                self.trade_leave(e);
                true
            }
            Callback::WellEnter2 => {
                self.well_join(e, true);
                false
            }
            Callback::WellEnter3 => {
                self.well_join(e, false);
                false
            }
            Callback::AttendanceEnter2 => {
                self.attendance_enter(jobs, e);
                false
            }
            Callback::AttendanceExit2 => {
                // `FUN_1401318c0(icon, 2, full)`.
                let full = self.attendance_full(e);
                self.set_icon(e, crate::professions::icon::LIMIT, full);
                false
            }
            Callback::AttendanceEnter3 => {
                self.remove_work(jobs, e);
                self.attendance_leave(e);
                false
            }
            Callback::HealerEnter2 => {
                if !self.healers.contains(&e) {
                    self.healers.push(e);
                }
                false
            }
            Callback::HappinessEnter => {
                self.happiness_enter(e);
                false
            }
            Callback::HappinessUpdate => {
                self.happiness_update(e);
                false
            }
            Callback::HappinessLeave => {
                self.happiness_leave(e);
                false
            }
            Callback::HealerEnter3 => {
                self.healers.retain(|&x| x != e);
                false
            }
            Callback::ProduceEnter2 => {
                let off = self.sites[&e].produce.as_ref().is_some_and(|p| p.disabled);
                self.machine(e).request(if off { 0xe } else { 0xd });
                false
            }
            Callback::ProduceClearPoints => {
                self.produce_clear_points(e);
                false
            }
            Callback::ProduceEnterD => {
                if self.sites[&e].produce.as_ref().is_some_and(|p| p.disabled) {
                    self.machine(e).request(0xe);
                    return true;
                }
                // `FUN_140134130(work, 1, 0, …)` with slot `+0x90`'s work, then
                // `FUN_1401350d0(1)`.
                let t = &self.site_templates[self.sites[&e].template];
                let work = if t.herbalist.is_some() {
                    self.herbalist_work(e)
                } else if t.forester.is_some() {
                    self.forester_work(e)
                } else if t.fishing.is_some() {
                    self.fishing_work(e)
                } else if t.hunt.is_some() {
                    self.hunt_work(e)
                } else if t.market.is_some() {
                    self.market_work(e)
                } else {
                    self.produce_work(e)
                };
                self.set_work(jobs, e, work, 1, false);
                self.enable_workplace(e, true);
                // Slot `+0x98`.
                self.fishing_register(e);
                false
            }
            Callback::ProduceEnterE => {
                // `FUN_140134020` when registered, then `FUN_1401350d0(0)`.
                let id = self.sites[&e].work_id;
                if id >= 0 && jobs.works.contains_key(&id) {
                    self.remove_work(jobs, e);
                }
                self.enable_workplace(e, false);
                // Slot `+0xa0`.
                self.fishing_unregister(e);
                true
            }
            Callback::ProduceUpdateD => {
                // Slot `+0xa8`.
                self.fishing_water(e);
                false
            }
            Callback::ProduceLeaveD => {
                // Slot `+0xa0`.
                self.fishing_unregister(e);
                true
            }
            Callback::ClearResourceEnter2 => {
                // `FUN_1400c8370` flags the entity (`+0x40` bit 10) and queues it for the entity
                // manager's removal.
                if let Some(site) = self.sites.get_mut(&e) {
                    site.destroyed = true;
                }
                true
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
    fn has_emergency(&self, c: usize) -> bool {
        let s = &self.states[c];
        crate::fire::has_emergency(
            self.cells,
            self.placed,
            s.home,
            s.workplace,
            self.characters[c].position,
        )
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
            WorkKind::Clear(_) => return self.clear_refresh(work),
            WorkKind::Pickup(_) => return self.pickup_refresh(work),
            WorkKind::Produce(_) => return self.produce_refresh(work),
            WorkKind::Fishing(_) => return self.fishing_refresh(work),
            WorkKind::PlantArea(_) | WorkKind::Harvest(_) | WorkKind::PickFruit(_) => {
                return self.area_refresh(work)
            }
            WorkKind::PlantOrchard(_) => return self.plant_orchard_refresh(work),
            // `FUN_1400f3620` / `FUN_1400f0130`.
            WorkKind::Herbalist(h)
            | WorkKind::Forester(h)
            | WorkKind::Hunt(h)
            | WorkKind::TendArea(h)
            | WorkKind::Pasture(h)
            | WorkKind::ClearStorage(h)
            | WorkKind::Fire(h) => h.workers.iter().copied(),
            // `FUN_1400e9590`: the builder no longer on it.
            WorkKind::Road(r) => {
                if let Some(c) = r.builder {
                    if self.characters[c].stack.holds_task(id).is_none() {
                        if let WorkKind::Road(r) = &mut work.kind {
                            r.builder = None;
                        }
                    }
                }
                return;
            }
            // `FUN_1400fafb0`: the haulers no longer on it.
            WorkKind::Trade(t) => {
                let gone: Vec<usize> = t
                    .haulers
                    .keys()
                    .copied()
                    .filter(|&c| self.characters[c].stack.holds_task(id).is_none())
                    .collect();
                if let WorkKind::Trade(t) = &mut work.kind {
                    for c in gone {
                        t.haulers.remove(&c);
                    }
                }
                return;
            }
            // `FUN_1400f4c90`: the worker no longer holds the task.
            WorkKind::Location(l) => {
                if let Some(c) = l.worker {
                    if self.characters[c].stack.holds_task(id).is_none() {
                        if let WorkKind::Location(l) = &mut work.kind {
                            l.worker = None;
                        }
                    }
                }
                return;
            }
            // `FUN_1400f51f0`: the vendors no longer on it leave the plans.
            WorkKind::Market(m) => {
                let gone: Vec<usize> = m
                    .plans
                    .keys()
                    .copied()
                    .filter(|&c| self.characters[c].stack.holds_task(id).is_none())
                    .collect();
                if let WorkKind::Market(m) = &mut work.kind {
                    for c in gone {
                        m.plans.remove(&c);
                    }
                }
                return;
            }
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
            WorkKind::Herbalist(h)
            | WorkKind::Forester(h)
            | WorkKind::Hunt(h)
            | WorkKind::TendArea(h)
            | WorkKind::Pasture(h)
            | WorkKind::ClearStorage(h)
            | WorkKind::Fire(h) => {
                for c in &gone {
                    h.workers.remove(c);
                }
            }
            WorkKind::Clear(_)
            | WorkKind::Pickup(_)
            | WorkKind::Produce(_)
            | WorkKind::Fishing(_)
            | WorkKind::PlantArea(_)
            | WorkKind::Harvest(_)
            | WorkKind::PlantOrchard(_)
            | WorkKind::PickFruit(_)
            | WorkKind::Location(_)
            | WorkKind::Market(_)
            | WorkKind::Trade(_)
            | WorkKind::Road(_)
            | WorkKind::Test(_) => {}
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
            WorkKind::Clear(_) => self.clear_try(work, c, time),
            WorkKind::Pickup(_) => self.pickup_try(work, c, time),
            WorkKind::Produce(_) => self.produce_try(work, c, time),
            WorkKind::Location(_) => self.location_try(work, c),
            WorkKind::ClearStorage(_) => self.clear_storage_try(work, c, time),
            WorkKind::Fire(_) => self.fire_try(work, c, time),
            WorkKind::Trade(_) => self.trade_try(work, c),
            WorkKind::Road(_) => self.road_try(work, c, time),
            WorkKind::Market(_) => self.market_try(work, c),
            WorkKind::Fishing(_) => self.fishing_try(work, c),
            WorkKind::PlantArea(_) => self.plant_try(work, c),
            WorkKind::PlantOrchard(_) => self.plant_orchard_try(work, c),
            WorkKind::TendArea(_) => {
                if self
                    .sites
                    .get(&work.entity)
                    .is_some_and(|s| s.orchard.is_some())
                {
                    self.tend_orchard_try(work, c)
                } else {
                    self.tend_try(work, c)
                }
            }
            WorkKind::PickFruit(_) => {
                let works: BTreeSet<i32> = self.piles.values().map(|p| p.work).collect();
                if self.auto_pickup_offers(work.entity, |id| works.contains(&id)) {
                    return NESTED;
                }
                self.pick_fruit_try(work, c)
            }
            WorkKind::Harvest(_) => {
                let works: BTreeSet<i32> = self.piles.values().map(|p| p.work).collect();
                if self.auto_pickup_offers(work.entity, |id| works.contains(&id)) {
                    return NESTED;
                }
                self.harvest_try(work, c)
            }
            WorkKind::Herbalist(_)
            | WorkKind::Forester(_)
            | WorkKind::Hunt(_)
            | WorkKind::Pasture(_) => {
                self.pasture_stage = false;
                let works: BTreeSet<i32> = self.piles.values().map(|p| p.work).collect();
                if self.auto_pickup_offers(work.entity, |id| works.contains(&id)) {
                    return NESTED;
                }
                self.resume(work, c)
            }
            WorkKind::Test(_) => 0,
        }
    }

    fn take_offers(&mut self) -> Vec<i32> {
        std::mem::take(&mut self.offers)
    }

    fn offers_again(&mut self) -> bool {
        std::mem::take(&mut self.offers_again)
    }

    fn resume(&mut self, work: &mut Work, c: usize) -> u8 {
        match work.kind {
            WorkKind::Produce(_) => self.produce_resume(work, c),
            WorkKind::Herbalist(_) => self.herbalist_try(work, c),
            WorkKind::Forester(_) => self.forester_try(work, c),
            WorkKind::Hunt(_) => self.hunt_try(work, c),
            WorkKind::Harvest(_) => self.harvest_try(work, c),
            WorkKind::PickFruit(_) => self.pick_fruit_try(work, c),
            WorkKind::Pasture(_) => self.pasture_try(work, c),
            _ => 1,
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
            // `FUN_1400eb300`: the resource (or cell) is done.
            WorkKind::Clear(cl) => {
                cl.resources.retain(|_, &mut w| w != c);
                cl.cells.remove(&param);
            }
            // `FUN_1400f8390`.
            WorkKind::Pickup(_) => {
                if let Some(p) = self.piles.get_mut(&work.entity) {
                    p.claimer = None;
                    p.state = 2;
                }
            }
            WorkKind::Produce(_) => Self::produce_complete(work, param, false),
            WorkKind::Fishing(_) => Self::fishing_complete(work, param),
            WorkKind::PlantArea(_) | WorkKind::Harvest(_) => Self::area_complete(work, param),
            WorkKind::PlantOrchard(_) => Self::plant_orchard_done(work, param),
            WorkKind::PickFruit(_) => Self::pick_done(work, param, false),
            // `FUN_1400f4d20`: free for the next one; the work stays.
            WorkKind::Location(l) => l.worker = None,
            // `FUN_1400f3860` / `FUN_1400f0370` / `FUN_1400f7820` / `FUN_1400ec100`.
            WorkKind::Herbalist(h)
            | WorkKind::Forester(h)
            | WorkKind::Hunt(h)
            | WorkKind::TendArea(h)
            | WorkKind::Pasture(h)
            | WorkKind::ClearStorage(h)
            // `FUN_1400eead0`.
            | WorkKind::Fire(h) => {
                h.workers.remove(&c);
            }
            // `FUN_1400f5430`.
            WorkKind::Market(m) => {
                m.plans.remove(&c);
            }
            // `FUN_1400fb1f0`.
            WorkKind::Trade(t) => {
                t.haulers.remove(&c);
            }
            // `FUN_1400e9620`: parameter 1 (the cell built) ends the work.
            WorkKind::Road(r) => {
                r.builder = None;
                return param == 1;
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
            // `FUN_1400eb2d0` calls the complete slot.
            WorkKind::Clear(cl) => {
                cl.resources.retain(|_, &mut w| w != c);
                cl.cells.remove(&param);
            }
            // `FUN_1400f8350`.
            WorkKind::Pickup(_) => {
                if let Some(p) = self.piles.get_mut(&work.entity) {
                    p.claimer = None;
                    p.state = 0;
                }
            }
            WorkKind::Produce(_) => Self::produce_complete(work, param, true),
            // `FUN_1400ef350` calls the complete slot.
            WorkKind::Fishing(_) => Self::fishing_complete(work, param),
            WorkKind::PlantArea(_) | WorkKind::Harvest(_) => Self::area_fail(work, param),
            WorkKind::PlantOrchard(_) => Self::plant_orchard_done(work, param),
            WorkKind::PickFruit(_) => Self::pick_done(work, param, true),
            WorkKind::Location(l) => l.worker = None,
            WorkKind::Pasture(_) => self.pasture_fail(work, c, param),
            // `FUN_1400f3850` / `FUN_1400f0360` / `FUN_1400ec0f0` / `FUN_1400eeac0`.
            WorkKind::ClearStorage(h) | WorkKind::Fire(h) => {
                h.workers.remove(&c);
            }
            WorkKind::Herbalist(h)
            | WorkKind::Forester(h)
            | WorkKind::Hunt(h)
            | WorkKind::TendArea(h) => {
                h.workers.remove(&c);
            }
            // `FUN_1400f5420`.
            WorkKind::Market(m) => {
                m.plans.remove(&c);
            }
            // `FUN_1400fb1e0`.
            WorkKind::Trade(t) => {
                t.haulers.remove(&c);
            }
            // `FUN_1400e95f0`.
            WorkKind::Road(r) => r.builder = None,
            WorkKind::Test(_) => {}
        }
    }

    /// `FUN_1400f1950` / `FUN_1400eac70`: clear every worker's stack (their aborts leave the
    /// set through the fail slot).
    fn cancel(&mut self, work: &mut Work) {
        if let WorkKind::Produce(_) = work.kind {
            return self.produce_cancel(work);
        }
        loop {
            let next = match &work.kind {
                WorkKind::Gather(g) => g.workers.first().copied(),
                WorkKind::Build(b) => b.workers.first().copied(),
                // `FUN_1400ebb20`: every claimer of a resource or cell.
                WorkKind::Clear(cl) => cl
                    .resources
                    .values()
                    .chain(cl.cells.values())
                    .next()
                    .copied(),
                // `FUN_1400f8ec0`: the claimer, while claimed.
                WorkKind::Pickup(_) => self
                    .piles
                    .get(&work.entity)
                    .filter(|p| p.state == 1)
                    .and_then(|p| p.claimer),
                // `FUN_1400f4fc0`: the worker's stack is cleared.
                WorkKind::Location(l) => l.worker,
                // `FUN_1400efd00`: every worker's stack is cleared.
                WorkKind::Fishing(fw) => fw.workers.iter().flatten().next().copied(),
                // `FUN_1400f9930` / `FUN_1400f2740`.
                WorkKind::PlantArea(_)
                | WorkKind::Harvest(_)
                | WorkKind::PlantOrchard(_)
                | WorkKind::PickFruit(_) => Self::area_workers(work).first().copied(),
                // `FUN_1400f3fb0` / `FUN_1400f0d70` / `FUN_1400f8060`.
                WorkKind::Herbalist(h)
                | WorkKind::Forester(h)
                | WorkKind::Hunt(h)
                | WorkKind::TendArea(h)
                | WorkKind::Pasture(h)
                | WorkKind::ClearStorage(h)
                // `FUN_1400eee70`.
                | WorkKind::Fire(h) => h.workers.first().copied(),
                // `FUN_1400f7170`: every vendor's stack is cleared.
                WorkKind::Market(m) => m.plans.keys().next().copied(),
                // `FUN_1400fc0b0`.
                WorkKind::Trade(t) => t.haulers.keys().next().copied(),
                // `FUN_1400e9de0`.
                WorkKind::Road(r) => r.builder,
                WorkKind::Produce(_) | WorkKind::Test(_) => None,
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
                WorkKind::Clear(cl) => {
                    cl.resources.retain(|_, &mut w| w != c);
                    cl.cells.retain(|_, &mut w| w != c);
                }
                WorkKind::Pickup(_) => {
                    if let Some(p) = self.piles.get_mut(&work.entity) {
                        p.claimer = None;
                        p.state = 0;
                    }
                }
                WorkKind::Location(l) => l.worker = None,
                WorkKind::Herbalist(h)
                | WorkKind::Forester(h)
                | WorkKind::Hunt(h)
                | WorkKind::TendArea(h)
                | WorkKind::Pasture(h)
                | WorkKind::ClearStorage(h)
                | WorkKind::Fire(h) => {
                    h.workers.remove(&c);
                }
                WorkKind::Fishing(fw) => {
                    for w in fw.workers.iter_mut().filter(|w| **w == Some(c)) {
                        *w = None;
                    }
                }
                WorkKind::PlantArea(a)
                | WorkKind::Harvest(a)
                | WorkKind::PlantOrchard(a)
                | WorkKind::PickFruit(a) => {
                    let cells: Vec<i32> = a
                        .claims
                        .iter()
                        .filter(|(_, &w)| w == c)
                        .map(|(&i, _)| i)
                        .collect();
                    for i in cells {
                        a.claims.remove(&i);
                        if let Some(n) = a.need.get_mut(i as usize) {
                            *n = true;
                        }
                    }
                }
                WorkKind::Market(m) => {
                    m.plans.remove(&c);
                }
                WorkKind::Trade(t) => {
                    t.haulers.remove(&c);
                }
                WorkKind::Road(r) => r.builder = None,
                WorkKind::Produce(_) | WorkKind::Test(_) => {}
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
                    happiness: self.happiness,
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
                // The drop-off activity (`FUN_14011b330`).
                self.drop_off(c);
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
        if wp.workers.len() as i32 == wp.max() && !wp.workers.contains(&c) {
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

/// Clearing (`ClearArea`), loose piles (`RawMaterialComponent`, `Pickup`) and the drop-off
/// activity.
impl Sim<'_, '_> {
    /// The entity on a cell as the job code reads it: occupant (`+0x00`), else object (`+0x08`).
    fn entity_on(&self, cell: i32) -> Option<usize> {
        let i = cell as usize;
        self.cells.occupants[i].or(self.cells.objects[i])
    }

    /// `FUN_14011ce30`'s registration: count the footprint's objects into a category mask
    /// (bit 0, plus logs/stone/iron from their raw materials), keep `+0x48` = `FUN_14011d800`
    /// and register a ClearArea work.
    fn register_clear(&mut self, jobs: &mut Jobs, e: usize) {
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

    /// `FUN_14011d800`: the footprint cells holding another object.
    pub fn clear_remaining(&self, e: usize) -> i32 {
        clear_remaining(&self.cells.objects, self.size as i32, &self.placed[e], e)
    }

    /// `FUN_1400eafe0`: drop resource claims whose citizen lost the task; a lost cell claim
    /// returns before anything is dropped.
    fn clear_refresh(&mut self, work: &mut Work) {
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
    fn clear_try(&mut self, work: &mut Work, c: usize, time: i64) -> u8 {
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
                        || self.cells.road_kind[i] != crate::road::NONE
                        || self.pending_roads.contains_key(&cell);
                    if road {
                        entries.push((cell, dy * dy + dx * dx, None));
                    }
                }
            }
        }
        crate::ai::crt_qsort_by(&mut entries, |a, b| a.1 - b.1);
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
                let point = cell_ground(self.size, self.terrain, nb);
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
                let target = cell_ground(self.size, self.terrain, cell);
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
        let facing = crate::character::work_facing(self.facing_centre(resource), point);
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

    /// `FUN_1400cff70`: a removal sequence was aborted.
    pub fn unclaim(&mut self, resource: usize) {
        self.claimed.remove(&resource);
    }

    /// `FUN_14012b910`: a resource with the animation flag plays clip `anim`.
    pub fn animate_resource(&mut self, resource: usize, anim: usize) {
        let p = &mut self.placed[resource];
        let t = &self.templates[p.template];
        if t.natural.as_ref().is_some_and(|n| n.flag)
            && t.animations
                .get(p.variant)
                .and_then(Option::as_ref)
                .is_some_and(|g| g.clips.get(anim).is_some())
        {
            p.animation = Some(crate::animation::Playback::new(anim, 0));
        }
    }

    /// RemoveNaturalResource start (`FUN_1400cffe0`) → `FUN_14012b5c0`: queue the resource for
    /// removal; unless another profession should have harvested it, leave a pile of
    /// `yield × min(growth, 1)` units (the educated yield for educated citizens), turned
    /// `RandTol(2) × 90°`. `priority` is the priority of a work at the resource's cell
    /// (`FUN_1400fe220`).
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

    /// A RawMaterialComponent pile entity (`FUN_1400c8bf0` of a pile template, `FUN_14012ec30`
    /// places it, turned `yaw`): stamped on its cell and activated with the next
    /// [`Sim::activate_piles`].
    pub(crate) fn spawn_pile(
        &mut self,
        pile_t: usize,
        cell: (i32, i32),
        amount: i32,
        yaw: f32,
        priority: Option<i32>,
    ) -> usize {
        let Some(material) = self.templates[pile_t].pile_material else {
            return usize::MAX;
        };
        let pt = &self.templates[pile_t];
        let n = self.size as i32;
        let index = cell.1 * n + cell.0;
        let id = self.placed.len();
        let mut yaw = yaw as i32;
        while yaw < 0 {
            yaw += 360;
        }
        let (variant, mesh) = pt.model.as_ref().map_or((0, 0), |m| {
            (m.variant.max(0) as usize, m.mesh.max(0) as usize)
        });
        self.placed.push(Placed {
            template: pile_t,
            bucket: pt.bucket,
            cell,
            footprint: (1, 1),
            position: cell_ground(self.size, self.terrain, index),
            yaw: yaw as f32,
            variant,
            mesh,
            growth: 0.0,
            growth_state: None,
            spawn_state: None,
            alive: true,
            removal_requested: false,
            animation: None,
            scale: [1.0; 4],
        });
        // The pile's MapComponent stamps its cell on activation (`MapDescription +0x30`).
        self.stamp(id);
        self.piles.insert(
            id,
            Pile {
                material,
                count: amount,
                age: 0,
                work: -1,
                mask: pile_mask(self.materials[material].flags),
                priority,
                auto_pickup: None,
                state: 0,
                claimer: None,
            },
        );
        self.new_piles.push(id);
        id
    }

    /// RawMaterialComponent activation (`FUN_14012e6a0`): register each new pile's Pickup work.
    pub fn activate_piles(&mut self, jobs: &mut Jobs) {
        for id in std::mem::take(self.new_piles) {
            let Some(pile) = self.piles.get(&id) else {
                continue;
            };
            let (mask, priority, auto_pickup) = (pile.mask, pile.priority, pile.auto_pickup);
            // RawMaterialComponent's setup (`FUN_14012e790`): its material becomes known.
            self.discover(pile.material);
            let cell = self.rect_cell(id);
            let work = Work::new(
                id,
                cell,
                self.professions.laborer,
                WorkKind::Pickup(PickupWork),
            );
            let w = jobs.add(work, mask, priority);
            self.piles.get_mut(&id).unwrap().work = w;
            if let Some(wp) = auto_pickup {
                self.auto_pickup_add(wp, w);
            }
        }
    }

    /// Destroy a pile (picked up or decayed): its entity, its cell and its work.
    pub fn destroy_pile(&mut self, jobs: &mut Jobs, id: usize) {
        let Some(pile) = self.piles.remove(&id) else {
            return;
        };
        if pile.work >= 0 {
            jobs.remove(pile.work, self);
        }
        let p = &mut self.placed[id];
        p.alive = false;
        let (x, y) = p.cell;
        let i = (y as usize) * self.size + x as usize;
        if self.cells.objects[i] == Some(id) {
            self.cells.objects[i] = None;
            self.cells.types[i] = self.terrain.natural_cell_type(self.size, i);
        }
        *self.models_dirty = true;
    }

    /// `FUN_14010ae20`: every pile on the footprint (by rows) that the site's storage can take
    /// goes into it.
    pub(crate) fn absorb_piles(&mut self, jobs: &mut Jobs, e: usize) {
        let Some(storage) = self.sites.get(&e).map(|s| s.storage) else {
            return;
        };
        let n = self.size as i32;
        let (x, y) = self.placed[e].cell;
        let (w, h) = self.placed[e].footprint;
        for cy in y..y + h {
            for cx in x..x + w {
                let Some(o) = self.cells.objects[(cy * n + cx) as usize] else {
                    continue;
                };
                let Some(pile) = self.piles.get(&o) else {
                    continue;
                };
                let (material, count) = (pile.material, pile.count);
                if self.storages[storage].fits(self.materials, material, count) {
                    self.storages[storage].add(material, count);
                    self.destroy_pile(jobs, o);
                }
            }
        }
    }

    /// PickupRawMaterial start (`FUN_1400ceda0`): the pile goes into the carried storage.
    pub fn take_pile(&mut self, jobs: &mut Jobs, c: usize, id: usize) {
        let Some(pile) = self.piles.get(&id) else {
            return;
        };
        let (material, count) = (pile.material, pile.count);
        self.states[c].carried.add(material, count);
        self.destroy_pile(jobs, id);
    }

    /// `FUN_14012e600`: piles older than an hour disappear.
    pub fn age_piles(&mut self, jobs: &mut Jobs, dt: i64) {
        let old: Vec<usize> = self
            .piles
            .iter_mut()
            .filter_map(|(&id, p)| {
                p.age += dt;
                (p.age > ticks(3600.0)).then_some(id)
            })
            .collect();
        for id in old {
            self.destroy_pile(jobs, id);
        }
    }

    /// `FUN_1400f82f0`: a claimer that lost the task frees the pile.
    fn pickup_refresh(&mut self, work: &mut Work) {
        let id = work.id;
        if let Some(p) = self.piles.get_mut(&work.entity) {
            if let Some(c) = p.claimer {
                if self.characters[c].stack.holds_task(id).is_none() {
                    p.claimer = None;
                    p.state = 0;
                }
            }
        }
    }

    /// `FUN_140092890`: does any registered storage take `count` of `material`?
    fn any_storage_fits(&self, material: usize, count: i32) -> bool {
        self.storage_sites.iter().any(|s| {
            let st = &self.storages[s.storage];
            st.accepts & self.materials[material].flags != 0
                && st.fits(self.materials, material, count)
        })
    }

    /// `FUN_1400f83d0`: Pickup's slot `+0x20`. Takes this pile and every unclaimed pile within
    /// five cells that fits the same storage, nearest first, while they fit the carried
    /// capacity.
    fn pickup_try(&mut self, work: &mut Work, c: usize, time: i64) -> u8 {
        let e = work.entity;
        let Some(pile) = self.piles.get(&e).cloned() else {
            return 1;
        };
        if pile.state != 0 {
            return 1;
        }
        let size = |m: usize, s: &Self| s.materials[m].size;
        let carried = &self.states[c].carried;
        let mut free = carried.capacity - carried.used(self.materials);
        if free < size(pile.material, self) * pile.count {
            self.drop_off(c);
            return 2;
        }
        let from = self.citizen_cell(c);
        let cell = work.cell;
        if !self.any_storage_fits(pile.material, pile.count) {
            return 1;
        }
        let Some(nb) = nearest_neighbour(self.size, &self.cells.types, cell, from, 0xe07) else {
            return 1;
        };
        if !self.reachable(nb, from) {
            return 1;
        }
        if !self.patience(work, c, nb, from, time) {
            return 0;
        }
        let found = self.find_storage(pile.material, pile.count, true, nb);
        let Some(found) = found else {
            return 1;
        };
        let first_rank = self.rank_or_distance(from, cell);
        let mut list: Vec<(i32, i32, i32, i32, usize)> = vec![(cell, nb, first_rank, work.id, e)];
        let n = self.size as i32;
        let (px, py) = (cell % n, cell / n);
        for dy in -5..=5 {
            for dx in -5..=5 {
                if dx == 0 && dy == 0 {
                    continue;
                }
                let (x, y) = (px + dx, py + dy);
                if x < 0 || y < 0 || x >= n || y >= n {
                    continue;
                }
                let k = y * n + x;
                let Some(o) = self.entity_on(k) else { continue };
                let Some(other) = self.piles.get(&o).cloned() else {
                    continue;
                };
                if !self.storages[found.storage].fits(self.materials, other.material, 1) {
                    continue;
                }
                let Some(nb2) = nearest_neighbour(self.size, &self.cells.types, k, from, 0xe07)
                else {
                    continue;
                };
                if !self.reachable(nb2, from) {
                    continue;
                }
                if other.work < 0 || other.state != 0 {
                    continue;
                }
                let r = self.rank_or_distance(from, k);
                let id = other.work;
                list.push((k, nb2, r, id, o));
            }
        }
        crate::ai::crt_qsort_by(&mut list, |a, b| a.2 - b.2);
        let mut first = true;
        for (k, nb2, _, id, o) in list {
            let p = &self.piles[&o];
            let w = size(p.material, self) * p.count;
            if w > free {
                continue;
            }
            free -= w;
            let point = cell_ground(self.size, self.terrain, nb2);
            self.pickup_sequence(
                c,
                if first { Some(&mut *work) } else { None },
                id,
                o,
                point,
                k,
            );
            first = false;
            let p = self.piles.get_mut(&o).unwrap();
            p.state = 1;
            p.claimer = Some(c);
        }
        self.took_job(c, work);
        if first {
            1
        } else {
            2
        }
    }

    /// `FUN_140092a60` from a cell (`put` finds a storage that takes the units).
    pub(crate) fn find_storage(
        &mut self,
        material: usize,
        count: i32,
        put: bool,
        cell: i32,
    ) -> Option<crate::stock::StorageMatch> {
        let town = TownView {
            size: self.size,
            types: &self.cells.types,
            regions: self.regions,
            occupants: &self.cells.occupants,
            objects: &self.cells.objects,
            entities: self.entities,
            happiness: self.happiness,
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
            count,
            put,
            cell,
            None,
            &mut geometry,
            self.rng,
        )
    }

    /// [`Sim::find_storage`] passing over `exclude`'s own storage.
    pub(crate) fn find_storage_excluding(
        &mut self,
        material: usize,
        count: i32,
        put: bool,
        cell: i32,
        exclude: usize,
    ) -> Option<crate::stock::StorageMatch> {
        let town = TownView {
            size: self.size,
            types: &self.cells.types,
            regions: self.regions,
            occupants: &self.cells.occupants,
            objects: &self.cells.objects,
            entities: self.entities,
            happiness: self.happiness,
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
            count,
            put,
            cell,
            Some(exclude),
            &mut geometry,
            self.rng,
        )
    }

    /// `FUN_1400c5fa0`: walk next to a pile, pick it up and report. The first pile's sequence
    /// clears the stack (`current` is the work being tried).
    fn pickup_sequence(
        &mut self,
        c: usize,
        current: Option<&mut Work>,
        work: i32,
        pile: usize,
        point: [f32; 3],
        pile_cell: i32,
    ) {
        if let Some(w) = current {
            self.clear_citizen(c, Some(w));
        }
        let facing = crate::character::work_facing(self.facing_centre(pile), point);
        let carrying = self.states[c].carried.used(self.materials) > 0;
        self.push(c, Action::ShowTool(-1));
        self.on_empty_think(c);
        self.push(c, self.perform(c, facing, 7, 0.75, false));
        if !carrying {
            self.push(c, Action::ShowTool(-2));
        }
        self.push(c, Action::PickupPile { entity: pile });
        self.push(c, Action::CompleteTask { work, param: 0 });
        self.push(c, self.perform(c, facing, 6, 0.75, false));
        let mask = WALKABLE | crate::pathfind::bit(self.cells.types[pile_cell as usize]);
        let m = MoveTo::new(
            point,
            if carrying { 4 } else { 1 },
            0,
            mask,
            0,
            -1.0,
            self.rng,
        );
        self.push(c, Action::move_to(m));
        if carrying {
            self.push(c, Action::ShowTool(-2));
        }
    }

    /// `FUN_14011b330`: carry everything to storages that take it: one drop-off per storage
    /// (nearest found per material, reusing a storage already chosen), then a final drop-off
    /// of whatever is left.
    pub fn drop_off(&mut self, c: usize) {
        let from = self.citizen_cell(c);
        let items: Vec<(usize, i32)> = self.states[c]
            .carried
            .items
            .iter()
            .filter(|i| i.count != 0)
            .map(|i| (i.material, i.count))
            .collect();
        let mut chosen: Vec<(usize, [f32; 3], usize)> = Vec::new();
        for (material, count) in items {
            if chosen
                .iter()
                .any(|&(s, _, _)| self.storages[s].fits(self.materials, material, count))
            {
                continue;
            }
            if let Some(f) = self.find_storage(material, count, true, from) {
                let entity = self.storage_sites[f.site].entity;
                chosen.push((f.storage, f.target, entity));
            }
        }
        if chosen.is_empty() {
            return;
        }
        self.clear_citizen(c, None);
        self.on_empty_think(c);
        // The final drop-off with no target (its redirect search is not ported).
        self.push(
            c,
            Action::DropOff {
                storage: usize::MAX,
                material: usize::MAX,
                count: -1,
            },
        );
        let mask = WALKABLE | crate::pathfind::bit(self.cells.types[from as usize]);
        for (storage, target, entity) in chosen {
            let facing = crate::character::work_facing(self.facing_centre(entity), target);
            self.push(c, self.perform(c, facing, 7, 0.75, false));
            self.push(c, Action::ShowTool(-2));
            self.push(
                c,
                Action::DropOff {
                    storage,
                    material: usize::MAX,
                    count: -1,
                },
            );
            self.push(c, self.perform(c, facing, 6, 0.75, false));
            let m = MoveTo::new(target, 4, 0, mask, 0, -1.0, self.rng);
            self.push(c, Action::move_to(m));
        }
        let s = &mut self.states[c];
        s.job = -1;
        s.health_events |= 4;
        s.supply_activity = 0;
    }
}

/// `FUN_14011d800` over a cell → object grid of side `n`.
pub fn clear_remaining(objects: &[Option<usize>], n: i32, placed: &Placed, e: usize) -> i32 {
    let (x, y) = placed.cell;
    let (w, h) = placed.footprint;
    let mut count = 0;
    for cy in y..y + h {
        for cx in x..x + w {
            if objects[(cy * n + cx) as usize].is_some_and(|o| o != e) {
                count += 1;
            }
        }
    }
    count
}

impl Sim<'_, '_> {
    /// The residence update's work (`FUN_14012ee40`): a house nobody lives in that still
    /// holds goods gets a ClearStorage work (mask 1); an empty storage has it removed.
    pub fn residence_clear_storage(&mut self, jobs: &mut Jobs) {
        let homes: Vec<usize> = self.homes.keys().copied().collect();
        for e in homes {
            let Some(site) = self.sites.get(&e) else {
                continue;
            };
            let storage = site.storage;
            let registered = site.work_id >= 0 && jobs.works.contains_key(&site.work_id);
            let residents: usize = self.homes[&e].families.iter().map(Vec::len).sum();
            let used = self.storages[storage].used(self.materials);
            if residents == 0 && used != 0 && !registered {
                let work = Work::new(
                    e,
                    self.rect_cell(e),
                    self.professions.laborer,
                    WorkKind::ClearStorage(crate::herbalist::HerbalistWork::default()),
                );
                self.set_work(jobs, e, work, 1, false);
            } else if registered && used == 0 {
                self.remove_work(jobs, e);
            }
        }
    }

    /// ClearStorage's slot `+0x20` (`FUN_1400ec120`): the first item still unclaimed that a
    /// town storage takes, as much as the hauler can carry, is carried there from the house's
    /// first use point (a full hauler drops off first).
    pub(crate) fn clear_storage_try(&mut self, work: &mut Work, c: usize, time: i64) -> u8 {
        let e = work.entity;
        let Some(storage) = self.sites.get(&e).map(|s| s.storage) else {
            return 1;
        };
        let here = self.citizen_cell(c);
        let items: Vec<(usize, i32)> = self.storages[storage]
            .items
            .iter()
            .map(|i| (i.material, i.count - i.reserved))
            .collect();
        for (k, (material, left)) in items.into_iter().enumerate() {
            if left <= 0 {
                continue;
            }
            let mut amount = left;
            let size = self.materials[material].size.max(1);
            let carried = &self.states[c].carried;
            let free = carried.capacity - carried.used(self.materials);
            if free < size * amount {
                amount = free / size;
            }
            if amount == 0 {
                // The drop-off activity (`FUN_14011b330`).
                self.drop_off(c);
                return 2;
            }
            let Some(found) = self.find_storage_excluding(material, amount, true, work.cell, e)
            else {
                continue;
            };
            if !self.reachable(here, work.cell) {
                return 1;
            }
            // The house has no StorageLocation: its first use point.
            if self.templates[self.placed[e].template]
                .use_points
                .is_empty()
            {
                return 1;
            }
            let (source, _) = self.use_point(e, 0);
            let source_cell = self.cell_of(source);
            if !self.patience(work, c, source_cell, here, time) {
                return 0;
            }
            let target_entity = self.storage_sites[found.site].entity;
            self.haul(
                c,
                work,
                Haul {
                    target_entity,
                    target_storage: found.storage,
                    target: found.target,
                    source_storage: storage,
                    source_entity: e,
                    source,
                    material,
                    amount,
                    param: (amount << 8) | k as i32,
                    partial: false,
                    complete_on_pickup: true,
                    ..Default::default()
                },
            );
            self.took_job(c, work);
            if let WorkKind::ClearStorage(h) = &mut work.kind {
                h.workers.insert(c);
            }
            return 2;
        }
        1
    }
}
