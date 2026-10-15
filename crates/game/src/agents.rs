//! The citizens' CharacterComponent and CitizenAIComponent run together each simulation
//! iteration. See docs/CITIZEN-AI.md.
//!
//! Order per iteration: animations advance (`AnimationManager::Update`), every character runs
//! its actions and moves (entity manager bit-`0x04` set), "sequence finished" messages re-run
//! the AI with zero time (`FUN_140114810`), then the AI updates.
//!
//! Deviations: the AI runs every iteration instead of on its bucket's quota, and after the
//! placed entities' bucket pass instead of interleaved with it.

use banished_formats::game::CharacterDescription;
use banished_formats::types::AnimationGroup;

use crate::activities::{loiter, EntityView, StorageGeometry, TownView};
use crate::ai::{activity, CitizenAi, CitizenView, Thresholds};
use crate::character::{Action, Character, Component, Message, MoveTo, World};
use crate::citizens::Citizen;
use crate::pathfind::{regions, Pathfinder, WALKABLE};
use crate::random::Random;
use crate::scheduler::ticks;
use crate::sectors::SectorGraph;
use crate::seeding::{Cells, Placed, Template};
use crate::stock::{find_storage, RawMaterial, Storage, StorageSite};
use crate::terrain::{CellType, HeightGrid};
use crate::town::{Env, Sim};

/// `DAT_1401788fc`: cooldown set by the fetch-tool and fetch-clothing activities.
const FETCH_COOLDOWN: f32 = 75.0;

/// The CitizenDescription values the AI reads.
#[derive(Debug, Clone, Copy, Default)]
pub struct CitizenRulesAi {
    pub thresholds: Thresholds,
    /// `+0x6c` starting health, `+0xa4` starting happiness.
    pub health: f32,
    pub happiness: f32,
    /// `+0xbc`: loiter search range.
    pub loiter_range: i32,
    /// CitizenDescription +0x130 multiplied by 300 (FUN_140117730).
    pub clothing_duration: f32,
    pub health_max: f32,
    /// FUN_140118340: midpoint for food, quarter-point for cold status warnings.
    pub hunger_limit: f32,
    pub cold_limit: f32,
    pub profession_allows_idle: bool,
    /// CitizenDescription `+0x110`: the tool type CitizenUI calls "Iron" (others are "Steel").
    pub tool_grade: i32,
    /// CitizenDescription `+0xfc`, `+0x100`, `+0x104`, `+0x108` (ints) and `+0x10c` (°F): the
    /// work-distance patience of `FUN_1400fc3c0`.
    pub work_near: i32,
    pub work_far: i32,
    pub work_minutes: i32,
    pub work_ok: i32,
    pub work_cold: f32,
    /// CitizenDescription `+0x114`: work duration factor without a tool (`FUN_1400cebf0`).
    pub no_tool_factor: f32,
}

/// Citizen fields read/written by equipment activities and Citizen messages 1/6/7.
#[derive(Debug, Clone, Default)]
pub struct CitizenState {
    pub health: f32,
    pub happiness: f32,
    pub hunger: f32,
    pub cold: f32,
    pub sick: bool,
    pub work: Option<usize>,
    /// StatusIconComponent +0xf0 (Citizen +0x280 points to that component).
    pub status: u32,
    // Not ported: tool use during jobs and the citizen clothing-wear countdown.
    pub tool_status: i32,        // +0xa8
    pub tool_uses: i32,          // +0xac
    pub clothing_status: i32,    // +0x1e0
    pub clothing: i32,           // +0x1e4
    pub clothing_remaining: i64, // +0x1e8
    pub health_events: u32,      // +0x54
    pub flags: u32,              // +0x88
    pub supply_activity: i32,    // +0xb0
    pub job: i32,                // +0x30
    pub home: Option<usize>,     // +0x238
    pub carried: Storage,
    /// `+0x1f0`: profession (index into the profession list).
    pub profession: usize,
    /// `+0x248`: workplace entity.
    pub workplace: Option<usize>,
    /// The tool ShowTool attached (−1 none, −2 carried goods).
    pub shown_tool: i32,
}

impl CitizenState {
    /// Port of FUN_140117730 / FUN_1401152b0 (equipment message branches).
    pub fn message(&mut self, m: Message, rules: CitizenRulesAi) {
        match m.message {
            1 => {
                crate::status::set_icon(&mut self.status, 4, false, false);
                self.tool_uses = self.tool_uses.wrapping_add(m.a);
                self.tool_status = m.a;
            }
            6 | 7 => {
                self.clothing = m.message - 5;
                self.clothing_status = self.clothing;
                self.clothing_remaining = ticks(rules.clothing_duration);
                self.flags |= 0x800;
            }
            8 => self.flags |= 0x40000,
            _ => {}
        }
    }
}

struct TownStock<'a> {
    materials: &'a [RawMaterial],
    storages: &'a mut [Storage],
    sites: &'a [StorageSite],
}

pub struct Agents {
    pub characters: Vec<Character>,
    pub ais: Vec<CitizenAi>,
    pub states: Vec<CitizenState>,
    sectors: SectorGraph,
    pathfinder: Pathfinder,
    regions: Vec<u16>,
    region_types: Vec<CellType>,
    rules: CitizenRulesAi,
}

impl Agents {
    pub fn new(
        citizens: &[Citizen],
        cells: &Cells,
        desc: CharacterDescription,
        rules: CitizenRulesAi,
        carried: Storage,
        scale: impl Fn(&Citizen) -> f32,
    ) -> Agents {
        Agents {
            characters: citizens
                .iter()
                .map(|c| Character::new(desc, c.position, scale(c)))
                .collect(),
            ais: citizens.iter().map(|_| CitizenAi::default()).collect(),
            states: citizens
                .iter()
                .map(|_| CitizenState {
                    job: -1,
                    shown_tool: -1,
                    health: rules.health,
                    happiness: rules.happiness,
                    carried: carried.clone(),
                    ..Default::default()
                })
                .collect(),
            sectors: SectorGraph::new(cells.size, &cells.types),
            pathfinder: Pathfinder::new(cells.size, &cells.types),
            regions: regions(cells.size, &cells.types),
            region_types: cells.types.clone(),
            rules,
        }
    }

    pub fn rules(&self) -> CitizenRulesAi {
        self.rules
    }

    /// `FUN_1400a1950` when the map changed, and the path grid's type commands.
    fn sync_map(&mut self, cells: &Cells) {
        if self.region_types == cells.types {
            return;
        }
        for (i, (old, new)) in self.region_types.iter().zip(&cells.types).enumerate() {
            if old != new {
                self.pathfinder.set_type(i, *new);
                self.sectors
                    .mark((i % cells.size) as i32, (i / cells.size) as i32);
            }
        }
        self.region_types = cells.types.clone();
        self.regions = regions(cells.size, &cells.types);
    }

    /// The work and construction context over these citizens and `env`.
    fn sim<'s, 't>(
        &'s mut self,
        citizens: &[Citizen],
        env: &'s mut Env<'_, 't>,
        groups: &'s [Option<&'s AnimationGroup>],
        adult: &'s [bool],
        entities: &'s [EntityView],
    ) -> (Sim<'s, 't>, &'s mut crate::jobs::Jobs) {
        let _ = citizens;
        let c = &mut *env.construction;
        (
            Sim {
                characters: &mut self.characters,
                states: &mut self.states,
                groups,
                adult,
                rules: self.rules,
                size: env.cells.size,
                cells: &mut *env.cells,
                regions: &self.regions,
                sectors: &mut self.sectors,
                terrain: env.terrain,
                placed: &mut *env.placed,
                templates: env.templates,
                entities,
                storages: &mut *env.storages,
                storage_sites: env.sites,
                materials: env.materials,
                rng: &mut *env.rng,
                stock: env.stock,
                temperature: env.temperature,
                professions: &c.professions,
                workforce: &mut c.workforce,
                site_templates: &c.templates,
                sites: &mut c.sites,
                site_list: &mut c.site_list,
                events: Vec::new(),
            },
            &mut c.jobs,
        )
    }

    /// Adults without a school: the citizens professions count.
    fn adults(&self, citizens: &[Citizen]) -> Vec<bool> {
        citizens
            .iter()
            .map(|c| c.age >= self.rules.thresholds.adult_age && c.school.is_none())
            .collect()
    }

    /// Player commands and newly placed sites, applied before an iteration (where the
    /// original's UI events and placement land between iterations).
    pub fn apply_commands(
        &mut self,
        citizens: &[Citizen],
        env: &mut Env,
        groups: &[Option<&AnimationGroup>],
    ) {
        self.sync_map(env.cells);
        let commands = std::mem::take(&mut env.construction.commands);
        let placed = std::mem::take(&mut env.construction.placed);
        if commands.is_empty() && placed.is_empty() {
            return;
        }
        let entities = entity_views(env.placed, env.templates);
        let adult = self.adults(citizens);
        let (mut sim, jobs) = self.sim(citizens, env, groups, &adult, &entities);
        for e in placed {
            sim.activate(jobs, e);
            jobs.drain(&mut sim);
        }
        for command in commands {
            let mut workers = sim.workers();
            let town = TownView {
                size: sim.size,
                types: &sim.cells.types,
                regions: sim.regions,
                occupants: &sim.cells.occupants,
                objects: &sim.cells.objects,
                entities: sim.entities,
                happiness: &[],
            };
            let (size, terrain) = (sim.size, sim.terrain);
            let ground = move |c: i32| cell_ground(size, terrain, c);
            let mut geometry = StorageGeometry {
                town: &town,
                sectors: sim.sectors,
                ground: &ground,
            };
            sim.workforce.apply(
                command,
                &mut workers,
                sim.professions,
                &mut geometry,
                sim.rng,
            );
            sim.set_workers(&workers);
        }
    }

    /// One simulation iteration of every citizen. `groups[i]` is citizen `i`'s animation group.
    pub fn update(
        &mut self,
        citizens: &mut [Citizen],
        env: &mut Env,
        groups: &[Option<&AnimationGroup>],
        dt: f32,
    ) {
        self.sync_map(env.cells);
        // FUN_140118340's icon writes belong to simulation, never layout/hover/draw.
        // The surrounding needs/assignment updates remain unported; reflect their live fields.
        let professions = env.construction.professions.clone();
        for state in &mut self.states {
            let anywhere = if professions.list.is_empty() {
                self.rules.profession_allows_idle
            } else {
                professions.works_anywhere(state.profession)
            };
            for (bit, on) in [
                (0, state.home.is_none()),
                (
                    1,
                    state.hunger
                        > self.rules.thresholds.hungry_urgent
                            + (self.rules.hunger_limit - self.rules.thresholds.hungry_urgent) * 0.5,
                ),
                (
                    2,
                    state.cold
                        > self.rules.thresholds.cold_urgent
                            + (self.rules.cold_limit - self.rules.thresholds.cold_urgent) * 0.25,
                ),
                (3, !anywhere && state.workplace.is_none()),
                (5, state.sick),
            ] {
                crate::status::set_icon(&mut state.status, bit, on, false);
            }
        }
        self.sectors.age_cache(ticks(dt));
        let entities = entity_views(env.placed, env.templates);
        let adult = self.adults(citizens);
        for (ch, group) in self.characters.iter_mut().zip(groups) {
            if let Some(g) = group {
                ch.animate(g, dt);
            }
        }
        for i in 0..self.characters.len() {
            let Some(group) = groups[i] else { continue };
            let mut messages = Vec::new();
            {
                let state = &mut self.states[i];
                let mut world = World {
                    size: env.cells.size,
                    types: &env.cells.types,
                    regions: &self.regions,
                    terrain: env.terrain,
                    pathfinder: &mut self.pathfinder,
                    rng: env.rng,
                    messages: &mut messages,
                    materials: env.materials,
                    storages: env.storages,
                    carried: &mut state.carried,
                    tool_uses: &mut state.tool_uses,
                    shown_tool: &mut state.shown_tool,
                };
                self.characters[i].update(&mut world, group, dt);
            }
            citizens[i].position = self.characters[i].position;
            self.deliver(i, messages, citizens, env, groups, &adult, &entities);
        }
        // The sites' StateMachine updates (bit 4, then their bucket).
        {
            let (mut sim, jobs) = self.sim(citizens, env, groups, &adult, &entities);
            sim.update_sites(jobs);
            jobs.drain(&mut sim);
        }
        for i in 0..self.ais.len() {
            if groups[i].is_none() {
                continue;
            }
            self.think(i, citizens, env, groups, &adult, &entities, ticks(dt));
        }
    }

    /// Deliver the messages citizen `i`'s actions sent, in order.
    #[allow(clippy::too_many_arguments)]
    fn deliver(
        &mut self,
        i: usize,
        messages: Vec<Message>,
        citizens: &[Citizen],
        env: &mut Env,
        groups: &[Option<&AnimationGroup>],
        adult: &[bool],
        entities: &[EntityView],
    ) {
        for m in messages {
            match m.component {
                Component::Citizen => self.states[i].message(m, self.rules),
                Component::CitizenAi if m.message == 0 => {
                    self.think(i, citizens, env, groups, adult, entities, 0);
                }
                Component::Build if m.message == 0 => {
                    // `FUN_14010dbe0`: one unit of work.
                    if let Some(site) = m.entity.and_then(|e| env.construction.sites.get_mut(&e)) {
                        site.progress += 1;
                    }
                }
                Component::Jobs => {
                    let (mut sim, jobs) = self.sim(citizens, env, groups, adult, entities);
                    if m.message == 0 {
                        jobs.complete(m.a, i, m.b, &mut sim);
                    } else {
                        jobs.fail(m.a, i, m.b, &mut sim);
                    }
                    jobs.drain(&mut sim);
                }
                _ => {}
            }
        }
    }

    /// `FUN_1401148b0` for citizen `i`.
    #[allow(clippy::too_many_arguments)]
    fn think(
        &mut self,
        i: usize,
        citizens: &[Citizen],
        env: &mut Env,
        groups: &[Option<&AnimationGroup>],
        adult: &[bool],
        entities: &[EntityView],
        dt: i64,
    ) {
        let Some(group) = groups[i] else { return };
        let c = &citizens[i];
        let view = CitizenView {
            age: c.age,
            school: c.school.is_some(),
            tool: self.states[i].tool_uses,
            clothing: self.states[i].clothing,
            home: self.states[i].home.is_some(),
            carrying: self.states[i].carried.used(env.materials),
            health: self.states[i].health,
            happiness: self.states[i].happiness,
            hunger: self.states[i].hunger,
            cold: self.states[i].cold,
            sick: self.states[i].sick,
            ..Default::default()
        };
        let rules = self.rules;
        let mut ai = std::mem::take(&mut self.ais[i]);
        let busy = self.characters[i].stack.is_active();
        // The AI draws from the same stream the activities do.
        let mut rng = std::mem::replace(env.rng, Random::new(0));
        ai.update(
            dt,
            &view,
            &rules.thresholds,
            &mut rng,
            || busy,
            |ai, a, rng| {
                std::mem::swap(env.rng, rng);
                let done = if a == activity::WORK || a == activity::EMERGENCY {
                    let (mut sim, jobs) = self.sim(citizens, env, groups, adult, entities);
                    if a == activity::WORK {
                        jobs.assign_work(i, &mut sim);
                    } else {
                        jobs.assign_emergency(i, &mut sim);
                    }
                    jobs.drain(&mut sim);
                    self.characters[i].stack.is_active()
                } else {
                    let town = TownView {
                        size: env.cells.size,
                        types: &env.cells.types,
                        regions: &self.regions,
                        occupants: &env.cells.occupants,
                        objects: &env.cells.objects,
                        entities,
                        happiness: &[],
                    };
                    let mut stock = TownStock {
                        materials: env.materials,
                        storages: env.storages,
                        sites: env.sites,
                    };
                    let ch = &mut self.characters[i];
                    let done = run(
                        ai,
                        a,
                        ch,
                        &mut self.states[i],
                        &town,
                        &mut self.sectors,
                        &mut stock,
                        env.terrain,
                        group,
                        rules,
                        env.cells.size,
                        env.rng,
                    );
                    // Aborted CompleteTasks of the replaced sequence.
                    let outbox = std::mem::take(&mut ch.stack.outbox);
                    if !outbox.is_empty() {
                        let (mut sim, jobs) = self.sim(citizens, env, groups, adult, entities);
                        for m in outbox {
                            jobs.fail(m.a, i, m.b, &mut sim);
                        }
                        jobs.drain(&mut sim);
                    }
                    done
                };
                std::mem::swap(env.rng, rng);
                done
            },
        );
        *env.rng = rng;
        self.ais[i] = ai;
    }
}

/// `FUN_1401146c0`'s targets that are ported; the rest report "nothing to do".
#[allow(clippy::too_many_arguments)]
fn run(
    ai: &mut CitizenAi,
    a: u32,
    ch: &mut Character,
    state: &mut CitizenState,
    town: &TownView,
    sectors: &mut SectorGraph,
    stock: &mut TownStock,
    terrain: &HeightGrid,
    group: &AnimationGroup,
    rules: CitizenRulesAi,
    size: usize,
    rng: &mut Random,
) -> bool {
    match a {
        activity::TOOL | activity::CLOTHING => {
            // FUN_140111a50 / FUN_140111830: cooldown is set even when already equipped.
            ai.cooldowns[a as usize] = ticks(FETCH_COOLDOWN);
            let tool = a == activity::TOOL;
            if if tool {
                state.tool_uses > 0
            } else {
                state.clothing != 0
            } {
                return false;
            }
            let cell = town.cell_of(ch.position);
            let home = state.home.map_or(-1, |h| town.entrance(h, rng));
            let ground = |c| cell_ground(size, terrain, c);
            let mut geometry = StorageGeometry {
                town,
                sectors,
                ground: &ground,
            };
            let Some(found) = find_storage(
                stock.sites,
                stock.storages,
                stock.materials,
                if tool { 2 } else { 4 },
                cell,
                home,
                &mut geometry,
                rng,
            ) else {
                if tool {
                    state.tool_status = 0;
                    crate::status::set_icon(&mut state.status, 4, true, false);
                } else {
                    state.clothing_status = 0;
                }
                state.flags |= 4;
                // Not ported: drawing the StatusIconComponent's billboard (its mask is live).
                return false;
            };
            // FUN_1400c2540 / FUN_1400c2180: push in reverse execution order.
            let mut stack = std::mem::take(&mut ch.stack);
            stack.clear(ch, group, stock.storages);
            stack.set_on_empty(vec![Action::SendMessage(Message {
                component: Component::CitizenAi,
                message: 0,
                entity: None,
                a: 0,
                b: 0,
            })]);
            let material = &stock.materials[found.material];
            stack.push(
                ch,
                group,
                Action::SendMessage(Message {
                    component: Component::Citizen,
                    message: if tool {
                        1
                    } else {
                        6 + ((material.flags >> 15) & 1) as i32
                    },
                    entity: None,
                    a: if tool { material.tool_uses } else { 0 },
                    b: 0,
                }),
            );
            stock.storages[found.storage].reserve(found.material, 1);
            stack.push(
                ch,
                group,
                Action::ConsumeStoredRawMaterial {
                    storage: found.storage,
                    items: vec![(found.material, 1)],
                    reserved: true,
                },
            );
            // FUN_1400c7e70: face the source model (or map rectangle centre).
            let e = &town.entities[stock.sites[found.site].entity];
            let centre = if e.has_model {
                e.position
            } else {
                e.rect.map_or(e.position, |(x, y, w, h)| {
                    cell_ground(size, terrain, (y + (h >> 1)) * size as i32 + x + (w >> 1))
                })
            };
            let facing = crate::character::work_facing(centre, found.target);
            stack.push(
                ch,
                group,
                Action::PerformWork {
                    facing,
                    anim: 8,
                    duration: ticks(1.0),
                    elapsed: 0,
                    consume: false,
                },
            );
            stack.push(
                ch,
                group,
                Action::move_to(MoveTo::new(found.target, 1, 0, WALKABLE, 0, -1.0, rng)),
            );
            ch.stack = stack;
            state.health_events |= 4;
            state.supply_activity = 0;
            state.job = -1;
            ch.stack.is_active()
        }

        activity::IDLE => {
            let cell = town.cell_of(ch.position);
            let ground = |c: i32| {
                let n = size as i32;
                let h = (n >> 1) as f32 - 0.5;
                let (x, y) = ((c % n) as f32 - h, (c / n) as f32 - h);
                let mut normal = [0.0; 3];
                [x, y, terrain.ground(x, y, &mut normal)]
            };
            let Some(plan) = loiter(town, cell, state.home, rules.loiter_range, rng, ground) else {
                return false;
            };
            // `FUN_1400bfb80`.
            let mut stack = std::mem::take(&mut ch.stack);
            stack.clear(ch, group, stock.storages);
            stack.set_on_empty(vec![Action::SendMessage(Message {
                component: Component::CitizenAi,
                message: 0,
                entity: None,
                a: 0,
                b: 0,
            })]);
            if let Some(e) = plan.happiness {
                // Citizen message 8 (state bit 0x40000), then HappinessComponent message 0.
                for (component, message, entity) in [
                    (Component::Citizen, 8, None),
                    (Component::Happiness, 0, Some(e)),
                ] {
                    stack.push(
                        ch,
                        group,
                        Action::SendMessage(Message {
                            component,
                            message,
                            entity,
                            a: 0,
                            b: 0,
                        }),
                    );
                }
            }
            stack.push(
                ch,
                group,
                Action::PlayAnimation {
                    anim: 0,
                    duration: ticks(plan.duration),
                    elapsed: 0,
                },
            );
            let walk = MoveTo::new(plan.target, 1, 0, WALKABLE, 0, -1.0, rng);
            stack.push(ch, group, Action::move_to(walk));
            ch.stack = stack;
            ai.since_loiter = 0;
            ch.stack.is_active()
        }
        _ => false,
    }
}

/// The parts of each placed entity the activities read.
pub fn entity_views(placed: &[Placed], templates: &[Template]) -> Vec<EntityView> {
    placed
        .iter()
        .map(|p| {
            let t = &templates[p.template];
            let rect = t.map.as_ref().map(|_| {
                let (w, h) = p.footprint;
                (p.cell.0, p.cell.1, w, h)
            });
            let (s, c) = p.yaw.to_radians().sin_cos();
            let use_points = t
                .use_points
                .iter()
                .map(|l| {
                    [
                        p.position[0] + c * l[0] - s * l[1],
                        p.position[1] + s * l[0] + c * l[1],
                        p.position[2] + l[2],
                    ]
                })
                .collect();
            EntityView {
                alive: p.alive,
                rect: if p.alive { rect } else { None },
                use_points,
                has_interact: t.has_interact,
                position: p.position,
                has_model: t.model.is_some(),
                happiness: None,
            }
        })
        .collect()
}

/// FUN_1400a48b0 without bridge picking (same ground query as MoveTo).
fn cell_ground(size: usize, terrain: &HeightGrid, cell: i32) -> [f32; 3] {
    let n = size as i32;
    let h = (n >> 1) as f32 - 0.5;
    let (x, y) = ((cell % n) as f32 - h, (cell / n) as f32 - h);
    let mut normal = [0.0; 3];
    [x, y, terrain.ground(x, y, &mut normal)]
}
