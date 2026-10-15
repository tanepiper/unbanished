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

/// `DAT_140178818`: children's model and walk scale (`FUN_14011a700`).
const CHILD_SCALE: f32 = 0.66;

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
    /// `+0x118`: the education a citizen counts as educated with.
    pub education: i32,
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
    /// `+0x240`: family slot in the home's ResidenceComponent (−1 none).
    pub home_slot: i32,
    /// `+0x4c`: pregnancy (positive, counting down) or the wait after a birth (negative).
    pub reproduction: f32,
    /// `+0x58`, `+0x90`: time towards the next health and happiness step.
    pub health_timer: i64,
    pub happiness_timer: i64,
    /// `+0x98`: every kind of food eaten.
    pub foods: Vec<usize>,
    /// `+0x260` ring: children and other dependents, in ring order.
    pub dependents: Vec<usize>,
    /// `+0x218`: time attending school.
    pub school_time: i64,
    /// `+0x208`: the school's AttendanceComponent entity (`Citizen::school` mirrors it).
    pub school: Option<usize>,
    /// `+0x228`, `+0x230`: the doctor's and the chapel's AttendanceComponent entity.
    pub doctor: Option<usize>,
    pub priest: Option<usize>,
    /// `+0x68`: the disease (index into the disease list).
    pub disease: Option<usize>,
    /// `+0x70`: illness left while sick, immunity left after.
    pub disease_left: i64,
    pub immunity: i64,
    /// `+0x78`: time to the next spread attempt.
    pub spread: i64,
    /// `+0x60`: time since the last disease check.
    pub disease_clock: i64,
    /// `+0x84`.
    pub field_84: i32,
    /// `+0x220`: has had a school place.
    pub schooled: bool,
    /// `+0x1f8`: the profession whose work it had an accident at (`FUN_14011bb20`).
    pub accident: Option<usize>,
    /// `+0x200`: time since the last injury check.
    pub accident_timer: i64,
}

impl CitizenState {
    /// A new citizen (`FUN_140119c40` and setup `FUN_140119520`).
    pub fn newborn(rules: &crate::life::LifeRules) -> CitizenState {
        CitizenState {
            job: -1,
            shown_tool: -1,
            home_slot: -1,
            health: rules.f(0x6c),
            happiness: rules.f(0xa4),
            ..Default::default()
        }
    }
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
    /// The CitizenDescription for the CitizenComponent's update.
    pub life: crate::life::LifeRules,
    /// Town manager `+0xa0`: living citizens in activation order.
    pub registry: Vec<usize>,
    /// Log entries the citizens asked for since the caller last took them.
    pub events: Vec<crate::life::LifeEvent>,
    /// `Game/Disease/Diseases.rsc` (`GameGlobal +0xa8`).
    pub diseases: Vec<banished_formats::game::Disease>,
    /// Town manager `+0x1a8`.
    pub town_disease: f32,
    /// The character description new citizens get.
    desc: CharacterDescription,
    /// A new citizen's carried storage.
    carried: Storage,
    /// Citizens queued for deletion this iteration (`FUN_14011c2a0`), with the reason.
    dying: Vec<(usize, i32)>,
    /// The laborer profession's `+0x50` (what home stocking carries at least).
    pub carry: i32,
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
        let mut states: Vec<CitizenState> = citizens
            .iter()
            .map(|_| CitizenState {
                job: -1,
                shown_tool: -1,
                home_slot: -1,
                health: rules.health,
                happiness: rules.happiness,
                carried: carried.clone(),
                ..Default::default()
            })
            .collect();
        for (s, c) in states.iter_mut().zip(citizens) {
            s.school = c.school;
        }
        // `FUN_14011bd70` put each child in its guardian's ring.
        for (i, c) in citizens.iter().enumerate() {
            if let Some(g) = c.parent {
                states[g].dependents.push(i);
            }
        }
        Agents {
            characters: citizens
                .iter()
                .map(|c| Character::new(desc, c.position, scale(c)))
                .collect(),
            ais: citizens.iter().map(|_| CitizenAi::default()).collect(),
            states,
            life: crate::life::LifeRules::default(),
            registry: (0..citizens.len()).collect(),
            events: Vec::new(),
            diseases: Vec::new(),
            town_disease: 0.0,
            desc,
            carried,
            carry: 0,
            dying: Vec::new(),
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
                homes: &mut c.homes,
                attendance: &mut c.attendance,
                attendance_list: &mut c.attendance_list,
                healers: &mut c.healers,
                happiness: &mut c.happiness,
                calendar: c.calendar,
                year: c.year,
                fruits: &mut c.fruits,
                wildlife: &mut *env.wildlife,
                keep_piles: &mut c.keep_piles,
                site_events: &mut c.site_events,
                storage_changes: &mut c.storage_changes,
                cemeteries: &mut c.cemeteries,
                tombstones: &mut c.tombstones,
                burials: &mut c.burials,
                site_list: &mut c.site_list,
                events: Vec::new(),
                piles: &mut c.piles,
                claimed: &mut c.claimed_resources,
                new_piles: &mut c.new_piles,
                models_dirty: &mut c.models_dirty,
                offers: Vec::new(),
                offers_again: false,
                removed: Vec::new(),
                animation_events: Vec::new(),
                produce_resume: None,
                pasture_stage: false,
            },
            &mut c.jobs,
        )
    }

    /// Adults without a school: the citizens professions count.
    fn adults(&self, citizens: &[Citizen]) -> Vec<bool> {
        citizens
            .iter()
            .zip(&self.states)
            .map(|(c, s)| c.alive && c.age >= self.rules.thresholds.adult_age && s.school.is_none())
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
        let destroy = std::mem::take(&mut env.construction.destroy);
        if commands.is_empty() && placed.is_empty() && destroy.is_empty() {
            return;
        }
        let entities = entity_views(env.placed, env.templates, env.construction);
        let adult = self.adults(citizens);
        let (mut sim, jobs) = self.sim(citizens, env, groups, &adult, &entities);
        // Placing activates at once in the original (`FUN_1400c9d70`); a cancel comes after.
        for e in placed {
            sim.activate(jobs, e);
            jobs.drain(&mut sim);
        }
        for e in destroy {
            sim.destroy_entity(jobs, e);
            jobs.drain(&mut sim);
        }
        for command in commands {
            match command {
                crate::professions::Command::SetCrop { entity, crop } => {
                    if sim.wildlife.pastures.contains_key(&entity) {
                        // The kind's list is only enabled while the pasture is empty
                        // (`FUN_1400ba320`).
                        if sim.wildlife.pastures[&entity].livestock.is_empty() {
                            sim.set_pasture_kind(jobs, entity, crop);
                        }
                    } else if sim.sites.get(&entity).is_some_and(|s| s.orchard.is_some()) {
                        sim.set_orchard_tree(entity, crop);
                    } else {
                        sim.set_crop(entity, crop);
                    }
                    jobs.drain(&mut sim);
                    continue;
                }
                crate::professions::Command::FieldAction { entity, action } => {
                    if sim.wildlife.pastures.contains_key(&entity) {
                        sim.pasture_action(jobs, entity, action);
                    } else if sim.sites.get(&entity).is_some_and(|s| s.orchard.is_some()) {
                        sim.orchard_action(entity, action);
                    } else {
                        sim.field_action(entity, action);
                    }
                    jobs.drain(&mut sim);
                    continue;
                }
                crate::professions::Command::SetLimit { entity, limit } => {
                    sim.set_pasture_limit(entity, limit);
                    continue;
                }
                crate::professions::Command::SetEnabled { entity, enabled } => {
                    sim.enable_workplace(entity, enabled);
                    jobs.drain(&mut sim);
                    continue;
                }
                _ => {}
            }
            let mut workers = sim.workers();
            let town = TownView {
                size: sim.size,
                types: &sim.cells.types,
                regions: sim.regions,
                occupants: &sim.cells.occupants,
                objects: &sim.cells.objects,
                entities: sim.entities,
                happiness: sim.happiness,
            };
            let (size, terrain) = (sim.size, sim.terrain);
            let ground = move |c: i32| cell_ground(size, terrain, c);
            let mut geometry = StorageGeometry {
                town: &town,
                sectors: sim.sectors,
                ground: &ground,
            };
            let characters = &*sim.characters;
            let cell = |c: usize| town.cell_of(characters[c].position);
            let professions = sim.professions;
            let anywhere = |p: usize| professions.works_anywhere(p);
            let sites = &*sim.sites;
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
                rng: &mut *sim.rng,
            };
            sim.workforce.apply(
                command,
                &mut workers,
                sim.professions,
                sim.homes,
                &mut geometry,
                &mut search,
            );
            sim.set_workers(&workers);
        }
    }

    /// One simulation iteration of every citizen. `groups[i]` is citizen `i`'s animation group
    /// (`None` for the dead); `newborn` makes a child (`FUN_14011c550`'s draws).
    pub fn update(
        &mut self,
        citizens: &mut Vec<Citizen>,
        env: &mut Env,
        groups: &[Option<&AnimationGroup>],
        dt: f32,
        newborn: &mut dyn FnMut(&mut Random, &Citizen, bool) -> Citizen,
    ) {
        self.sync_map(env.cells);
        // `FUN_140118340`'s job icon (bit 3): no workplace for a profession that needs one.
        let professions = env.construction.professions.clone();
        for state in &mut self.states {
            let anywhere = if professions.list.is_empty() {
                self.rules.profession_allows_idle
            } else {
                professions.works_anywhere(state.profession)
            };
            crate::status::set_icon(
                &mut state.status,
                3,
                !anywhere && state.workplace.is_none(),
                false,
            );
        }
        self.sectors.age_cache(ticks(dt));
        let mut entities = entity_views(env.placed, env.templates, env.construction);
        let mut animating = crate::town::animating(env.placed, env.templates);
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
                // The current job's work (`+0x30`) and its profession's accident odds.
                let accident_odds = (0 <= state.job)
                    .then(|| env.construction.jobs.works.get(&state.job))
                    .flatten()
                    .and_then(|w| {
                        let p = env.construction.professions.list.get(w.profession)?;
                        Some((w.profession, p.accident.0, p.accident.1))
                    });
                let mut world = World {
                    size: env.cells.size,
                    types: &env.cells.types,
                    objects: &env.cells.objects,
                    occupants: &env.cells.occupants,
                    flags: &env.cells.flags,
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
                    animating: &mut animating,
                    accident: &mut state.accident,
                    accident_timer: &mut state.accident_timer,
                    accident_odds,
                };
                self.characters[i].update(&mut world, group, dt);
            }
            citizens[i].position = self.characters[i].position;
            self.deliver(i, messages, citizens, env, groups, &adult, &mut entities);
        }
        // The sites' StateMachine updates (bit 4, then their bucket); the piles' ages
        // (RawMaterialComponent's scheduled update, run every iteration here).
        {
            let (mut sim, jobs) = self.sim(citizens, env, groups, &adult, &entities);
            sim.update_sites(jobs);
            sim.update_attendance();
            sim.age_piles(jobs, ticks(dt));
            sim.update_hunts(ticks(dt));
            sim.update_blight(ticks(dt));
            sim.place_burials();
            jobs.drain(&mut sim);
        }
        // The residences' state-2 updates (`FUN_14012ee40`): the ClearStorage work, then the
        // fire.
        {
            let (mut sim, jobs) = self.sim(citizens, env, groups, &adult, &entities);
            sim.residence_clear_storage(jobs);
            jobs.drain(&mut sim);
        }
        {
            let c = &mut *env.construction;
            for home in c.homes.values_mut() {
                crate::life::update_home(
                    home,
                    env.storages,
                    env.materials,
                    env.temperature,
                    ticks(dt),
                );
            }
        }
        for i in 0..self.ais.len() {
            if groups.get(i).copied().flatten().is_none() || !citizens[i].alive {
                continue;
            }
            // CharacterComponent's scheduled update (`FUN_14010e950`): the running chain's
            // notify, a hunter's `HuntAnimal`.
            refresh_views(&mut entities, env);
            self.hunt_notify(i, env, groups);
            // CitizenComponent before CitizenAIComponent (the template's component order); a
            // citizen queued for deletion stops there (entity flag `0x400`).
            let born = self.live(i, citizens, env, &entities, ticks(dt), newborn);
            for c in born {
                let mut ch = Character::new(self.desc, citizens[c].position, CHILD_SCALE);
                ch.yaw = self.characters[i].yaw;
                self.characters.push(ch);
                self.ais.push(CitizenAi::default());
                self.states[c].carried = self.carried.clone();
            }
            if self.dying.iter().any(|&(d, _)| d == i) {
                continue;
            }
            refresh_views(&mut entities, env);
            self.think(i, citizens, env, groups, &adult, &entities, ticks(dt));
        }
        // The entity manager deletes the queued entities (each component's remove slot);
        // removals can queue more (orphaned children).
        let mut k = 0;
        while k < self.dying.len() {
            let (c, _) = self.dying[k];
            self.remove(c, citizens, env, groups, &entities);
            k += 1;
        }
        self.dying.clear();
        for (c, s) in citizens.iter_mut().zip(&self.states) {
            c.school = s.school;
        }
    }

    /// The wild animals' CharacterComponent update (`FUN_14010e990`): animate, run the
    /// actions, turn and move.
    pub fn move_animals(
        &mut self,
        wildlife: &mut crate::wildlife::Wildlife,
        cells: &Cells,
        terrain: &HeightGrid,
        rng: &mut Random,
        materials: &[RawMaterial],
        dt: f32,
    ) {
        self.sync_map(cells);
        let mut animating = Vec::new();
        let mut storages: Vec<Storage> = Vec::new();
        let ids: Vec<usize> = wildlife.animals.keys().copied().collect();
        for a in ids {
            let (template, variant) = {
                let animal = &wildlife.animals[&a];
                if animal.dying {
                    continue;
                }
                (animal.template, animal.variant)
            };
            let Some(group) = wildlife.groups.get(&(template, variant)).cloned() else {
                continue;
            };
            let animal = wildlife.animals.get_mut(&a).unwrap();
            let mut messages = Vec::new();
            let (mut carried, mut tool_uses, mut shown_tool) = (Storage::default(), 0, -1);
            let (mut accident, mut accident_timer) = (None, 0);
            let mut world = World {
                size: cells.size,
                types: &cells.types,
                objects: &cells.objects,
                occupants: &cells.occupants,
                flags: &cells.flags,
                regions: &self.regions,
                terrain,
                pathfinder: &mut self.pathfinder,
                rng,
                messages: &mut messages,
                materials,
                storages: &mut storages,
                carried: &mut carried,
                tool_uses: &mut tool_uses,
                shown_tool: &mut shown_tool,
                animating: &mut animating,
                accident: &mut accident,
                accident_timer: &mut accident_timer,
                accident_odds: None,
            };
            animal.character.animate(&group, dt);
            animal.character.update(&mut world, &group, dt);
        }
    }

    /// `HuntAnimal`'s notify (`FUN_1400cd2b0`): once the lodge has gathered its hunting time
    /// (`FUN_140127f50`), the nearest wild animal within 8 (`FUN_140097730`), not already
    /// taken, next to a cell the hunter can reach, is taken (`FUN_1400c1370` without clearing:
    /// walk there, shoot 4 s, remove it, leave its raw materials where it stood), and its herd
    /// moves on (`FUN_140127810`).
    fn hunt_notify(&mut self, i: usize, env: &mut Env, groups: &[Option<&AnimationGroup>]) {
        let Some(group) = groups.get(i).copied().flatten() else {
            return;
        };
        let Some((lodge, done)) = self.characters[i].stack.hunt_watch() else {
            return;
        };
        if *done {
            return;
        }
        let Some(site) = env.construction.sites.get(&lodge) else {
            return;
        };
        let Some(d) = env.construction.templates[site.template].hunt else {
            return;
        };
        if site.hunt_time.unwrap_or(0) < ticks(d.time * 300.0) {
            return;
        }
        let pos = self.characters[i].position;
        let mut best = None;
        let mut nearest = 8.0f32 * 8.0;
        for (&a, animal) in &env.wildlife.animals {
            if animal.dying {
                continue;
            }
            let q = animal.character.position;
            let (dx, dy, dz) = (q[0] - pos[0], q[1] - pos[1], q[2] - pos[2]);
            let d2 = dx * dx + dy * dy + dz * dz;
            if d2 < nearest {
                nearest = d2;
                best = Some(a);
            }
        }
        let Some(a) = best else { return };
        let animal = &env.wildlife.animals[&a];
        if animal.flags & crate::wildlife::HUNTED != 0 {
            return;
        }
        let n = env.cells.size as i32;
        let cell_of = |p: [f32; 3]| {
            let half = (n >> 1) as f32;
            let x = ((p[0] + half) as i32).clamp(0, n - 1);
            let y = ((p[1] + half) as i32).clamp(0, n - 1);
            n * y + x
        };
        let (here, at) = (cell_of(pos), cell_of(animal.character.position));
        let Some(nb) = crate::pathfind::nearest_neighbour(
            env.cells.size,
            &env.cells.types,
            at,
            here,
            WALKABLE,
        ) else {
            return;
        };
        let town = TownView {
            size: env.cells.size,
            types: &env.cells.types,
            regions: &self.regions,
            occupants: &env.cells.occupants,
            objects: &env.cells.objects,
            entities: &[],
            happiness: &[],
        };
        if !town.reachable(WALKABLE, nb, here) {
            return;
        }
        if let Some((_, done)) = self.characters[i].stack.hunt_watch() {
            *done = true;
        }
        let materials = env
            .wildlife
            .animal_kinds
            .get(&animal.template)
            .map(|k| k.materials.clone())
            .unwrap_or_default();
        let herd = animal.herd;
        let batch = self.states[i].carried.capacity;
        let workplace = self.states[i].workplace;
        let mut duration = ticks(4.0);
        if self.states[i].tool_uses < 1 {
            duration = ticks(crate::scheduler::seconds(duration) * self.rules.no_tool_factor);
        }
        let target = cell_ground(env.cells.size, env.terrain, nb);
        let walk = MoveTo::new(target, 1, 0, WALKABLE, 0, -1.0, env.rng);
        let ch = &mut self.characters[i];
        let mut stack = std::mem::take(&mut ch.stack);
        for &m in &materials {
            stack.push(
                ch,
                group,
                Action::CreateRawMaterial {
                    template: m,
                    cell: at,
                    batch,
                    workplace,
                    factor: 1.0,
                },
            );
        }
        stack.push(ch, group, Action::RemoveAnimal { entity: a });
        stack.push(
            ch,
            group,
            Action::PerformWork {
                facing: -1000.0,
                anim: 0xb,
                duration,
                elapsed: 0,
                consume: true,
            },
        );
        stack.push(ch, group, Action::move_to(walk));
        ch.stack = stack;
        // `FUN_14010c730`, then the herd's new target.
        let mut scheduler = crate::scheduler::EntityScheduler::default();
        let mut wild = crate::wildlife::Wild {
            wildlife: env.wildlife,
            placed: env.placed,
            templates: env.templates,
            cells: env.cells,
            terrain: env.terrain,
            rng: env.rng,
            // Taking an animal creates no entity.
            scheduler: &mut scheduler,
        };
        wild.hunted(a);
        if let Some(h) = herd {
            wild.retarget(h);
        }
        if let Some(t) = env
            .construction
            .sites
            .get_mut(&lodge)
            .and_then(|s| s.hunt_time.as_mut())
        {
            *t = 0;
        }
    }

    /// `FUN_140118340` for citizen `i`; returns the children born.
    #[allow(clippy::too_many_arguments)]
    fn live(
        &mut self,
        i: usize,
        citizens: &mut Vec<Citizen>,
        env: &mut Env,
        entities: &[EntityView],
        dt: i64,
        newborn: &mut dyn FnMut(&mut Random, &Citizen, bool) -> Citizen,
    ) -> Vec<usize> {
        if !citizens[i].alive || self.dying.iter().any(|&(d, _)| d == i) {
            return Vec::new();
        }
        let was_child = citizens[i].age < self.life.adult_age();
        let mut born = Vec::new();
        self.with_people(citizens, env, entities, newborn, &mut born, |people| {
            people.update(i, dt)
        });
        // `FUN_1401195c0`: an adult's model and walk at full scale.
        if was_child && citizens[i].age >= self.life.adult_age() {
            self.characters[i].scale = 1.0;
        }
        born
    }

    /// Run `f` over the citizens' life context.
    pub(crate) fn with_people<R>(
        &mut self,
        citizens: &mut Vec<Citizen>,
        env: &mut Env,
        entities: &[EntityView],
        newborn: &mut dyn FnMut(&mut Random, &Citizen, bool) -> Citizen,
        born: &mut Vec<usize>,
        f: impl FnOnce(&mut crate::life::People) -> R,
    ) -> R {
        let idle: Vec<bool> = self
            .characters
            .iter()
            .map(|c| !c.stack.is_active())
            .collect();
        let happiness = env.construction.happiness.clone();
        let town = TownView {
            size: env.cells.size,
            types: &env.cells.types,
            regions: &self.regions,
            occupants: &env.cells.occupants,
            objects: &env.cells.objects,
            entities,
            happiness: &happiness,
        };
        let c = &mut *env.construction;
        let mut people = crate::life::People {
            citizens,
            states: &mut self.states,
            registry: &mut self.registry,
            homes: &mut c.homes,
            rules: &self.life,
            town: &town,
            layers: &env.cells.layers,
            rng: env.rng,
            temperature: env.temperature,
            diseases: &self.diseases,
            town_disease: self.town_disease,
            storages: env.storages,
            materials: env.materials,
            idle: &idle,
            events: &mut self.events,
            dying: &mut self.dying,
            newborn,
            born,
            attendance: &mut c.attendance,
            attendance_list: &c.attendance_list,
            workforce: &mut c.workforce,
            professions: &c.professions,
            sites: &c.sites,
            cemeteries: &c.cemeteries,
            tombstones: &c.tombstones,
            burials: &mut c.burials,
            sectors: &mut self.sectors,
            terrain: env.terrain,
        };
        f(&mut people)
    }

    /// `FUN_140119170`: the dead citizen's components let go of the town (a removed entity's
    /// CharacterComponent abandons its actions, CitizenComponent leaves its home, family,
    /// workplace and registry; a laborer takes over its profession).
    /// Not ported: the carried goods (StorageComponent removal) are lost.
    #[allow(clippy::too_many_arguments)]
    fn remove(
        &mut self,
        c: usize,
        citizens: &mut Vec<Citizen>,
        env: &mut Env,
        groups: &[Option<&AnimationGroup>],
        entities: &[EntityView],
    ) {
        // Abandon the action stack: aborted CompleteTasks fail their works.
        {
            let adult = self.adults(citizens);
            let (mut sim, jobs) = self.sim(citizens, env, groups, &adult, entities);
            sim.clear_citizen(c, None);
            sim.characters[c].stack.set_on_empty(Vec::new());
            jobs.drain(&mut sim);
        }
        let happiness = env.construction.happiness.clone();
        let town = TownView {
            size: env.cells.size,
            types: &env.cells.types,
            regions: &self.regions,
            occupants: &env.cells.occupants,
            objects: &env.cells.objects,
            entities,
            happiness: &happiness,
        };
        let k = &mut *env.construction;
        let mut born = Vec::new();
        let mut never = |_: &mut Random, m: &Citizen, _| m.clone();
        let idle = vec![true; citizens.len()];
        let mut people = crate::life::People {
            citizens,
            states: &mut self.states,
            registry: &mut self.registry,
            homes: &mut k.homes,
            rules: &self.life,
            town: &town,
            layers: &env.cells.layers,
            rng: env.rng,
            temperature: env.temperature,
            diseases: &self.diseases,
            town_disease: self.town_disease,
            storages: env.storages,
            materials: env.materials,
            idle: &idle,
            events: &mut self.events,
            dying: &mut self.dying,
            newborn: &mut never,
            born: &mut born,
            attendance: &mut k.attendance,
            attendance_list: &k.attendance_list,
            workforce: &mut k.workforce,
            professions: &k.professions,
            sites: &k.sites,
            cemeteries: &k.cemeteries,
            tombstones: &k.tombstones,
            burials: &mut k.burials,
            sectors: &mut self.sectors,
            terrain: env.terrain,
        };
        let workplace = people.states[c].workplace;
        let profession = people.states[c].profession;
        people.bury(c);
        let laborer = k.professions.laborer;
        people.citizens[c].alive = false;
        // `FUN_1401173b0(None)` then `Town::Hire` for anything but a laborer.
        let mut workers: Vec<crate::professions::Worker> = people
            .states
            .iter()
            .zip(people.citizens.iter())
            .map(|(s, ci)| crate::professions::Worker {
                adult: ci.alive && ci.age >= self.life.adult_age() && s.school.is_none(),
                profession: s.profession,
                workplace: s.workplace,
                home: s.home,
            })
            .collect();
        k.workforce.set_workplace(&mut workers, c, None);
        if profession != laborer && !k.professions.list.is_empty() {
            let ground_size = env.cells.size;
            let terrain = env.terrain;
            let ground = move |cell: i32| cell_ground(ground_size, terrain, cell);
            let mut geometry = StorageGeometry {
                town: &town,
                sectors: &mut self.sectors,
                ground: &ground,
            };
            let hired = k.workforce.hire(
                &mut workers,
                laborer,
                profession,
                workplace,
                &k.homes,
                &mut geometry,
                env.rng,
            );
            match hired {
                Some(h) => self.events.push(crate::life::LifeEvent {
                    kind: crate::life::reason::REPLACED,
                    severity: 1,
                    citizen: h,
                    extra: Some((None, citizens[c].name.clone())),
                }),
                None => self.events.push(crate::life::LifeEvent {
                    kind: crate::life::reason::NOT_REPLACED,
                    severity: 1,
                    citizen: c,
                    extra: None,
                }),
            }
        }
        for (s, w) in self.states.iter_mut().zip(&workers) {
            s.profession = w.profession;
            s.workplace = w.workplace;
        }
    }

    /// Deliver the messages citizen `i`'s actions sent, in order.
    #[allow(clippy::too_many_arguments)]
    fn deliver(
        &mut self,
        i: usize,
        messages: Vec<Message>,
        citizens: &mut Vec<Citizen>,
        env: &mut Env,
        groups: &[Option<&AnimationGroup>],
        adult: &[bool],
        entities: &mut Vec<EntityView>,
    ) {
        for m in messages {
            // A message can create entities (piles) that a later one's AI reads.
            refresh_views(entities, env);
            let entities: &[EntityView] = entities;
            match m.component {
                Component::Citizen => match m.message {
                    0 => {
                        let home = m.entity.and_then(|e| env.construction.homes.get(&e));
                        crate::life::warm_at(&mut self.states[i], home, env.temperature);
                    }
                    2 => {
                        if let Some(st) = m.entity.and_then(|e| storage_of(env, e)) {
                            crate::life::eat(
                                &mut self.states[i],
                                &mut env.storages[st],
                                env.materials,
                                env.rng,
                            );
                        }
                    }
                    3 => {
                        // Treated by the herbalist or doctor.
                        self.states[i].health_events |= 0x200;
                        self.states[i].health += 0.5;
                    }
                    4 | 5 => {
                        let mut born = Vec::new();
                        self.with_people(
                            citizens,
                            env,
                            entities,
                            &mut |_, m, _| m.clone(),
                            &mut born,
                            |p| {
                                if m.message == 4 {
                                    p.lesson(i)
                                } else {
                                    p.treated(i)
                                }
                            },
                        );
                    }
                    _ => self.states[i].message(m, self.rules),
                },
                Component::CitizenAi if m.message == 0 => {
                    // `FUN_140114810`: the CitizenComponent's update with no time (while the
                    // character is idle: hunger, cold and old age can end it here), then the
                    // AI's unless the entity is going.
                    self.live(i, citizens, env, entities, 0, &mut |_, m, _| m.clone());
                    if !self.dying.iter().any(|&(d, _)| d == i) {
                        self.think(i, citizens, env, groups, adult, entities, 0);
                    }
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
                Component::CreateRawMaterial => {
                    let educated = citizens[i].education >= self.rules.education;
                    let (mut sim, jobs) = self.sim(citizens, env, groups, adult, entities);
                    let factor = f32::from_bits(m.message as u32);
                    sim.create_raw_material(jobs, i, m.a.max(0) as usize, m.b, educated, factor);
                    jobs.drain(&mut sim);
                }
                Component::CreateNaturalResource => {
                    let (mut sim, jobs) = self.sim(citizens, env, groups, adult, entities);
                    sim.plant(m.a.max(0) as usize, m.b);
                    jobs.drain(&mut sim);
                }
                Component::NaturalResource => {
                    let Some(e) = m.entity else { continue };
                    let educated = citizens[i].education >= self.rules.education;
                    let (mut sim, jobs) = self.sim(citizens, env, groups, adult, entities);
                    match m.message {
                        0 => {
                            // `FUN_1400fe220`: a work at the resource's cell lends its priority.
                            let n = sim.size as i32;
                            let (x, y) = sim.placed[e].cell;
                            let cell = y * n + x;
                            let priority = jobs
                                .works
                                .values()
                                .find(|w| w.cell == cell)
                                .map(|w| w.priority);
                            sim.remove_resource(i, e, educated, priority, m.a != 0);
                            sim.activate_piles(jobs);
                        }
                        1 => sim.unclaim(e),
                        _ => sim.animate_resource(e, m.a.max(0) as usize),
                    }
                    jobs.drain(&mut sim);
                }
                Component::Orchard => {
                    let Some(e) = m.entity else { continue };
                    let (mut sim, jobs) = self.sim(citizens, env, groups, adult, entities);
                    sim.orchard_message(e, m.message, m.a);
                    jobs.drain(&mut sim);
                }
                Component::CropField => {
                    let Some(e) = m.entity else { continue };
                    let (mut sim, jobs) = self.sim(citizens, env, groups, adult, entities);
                    sim.crop_message(e, m.message, m.a);
                    jobs.drain(&mut sim);
                }
                Component::Animal => {
                    if let Some(a) = m.entity {
                        env.wildlife.kill(a);
                    }
                }
                Component::Pasture => {
                    let Some(e) = m.entity else { continue };
                    let (mut sim, jobs) = self.sim(citizens, env, groups, adult, entities);
                    sim.pasture_message(e, m.message);
                    jobs.drain(&mut sim);
                }
                Component::Happiness => {
                    let Some(e) = m.entity else { continue };
                    if m.message != 0 {
                        continue;
                    }
                    let (mut sim, jobs) = self.sim(citizens, env, groups, adult, entities);
                    sim.happiness_visit(e);
                    jobs.drain(&mut sim);
                }
                Component::Pile => {
                    let Some(e) = m.entity else { continue };
                    let (mut sim, jobs) = self.sim(citizens, env, groups, adult, entities);
                    sim.take_pile(jobs, i, e);
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
        citizens: &mut Vec<Citizen>,
        env: &mut Env,
        groups: &[Option<&AnimationGroup>],
        adult: &[bool],
        entities: &[EntityView],
        dt: i64,
    ) {
        let Some(group) = groups.get(i).copied().flatten() else {
            return;
        };
        let c = &citizens[i];
        let (home_food, home_fuel) = self.states[i]
            .home
            .and_then(|h| env.construction.homes.get(&h))
            .map_or((false, false), |h| {
                (
                    crate::needs::home_has_food(
                        h,
                        self.states[i].home_slot,
                        env.storages,
                        env.materials,
                        &self.life,
                    ),
                    crate::needs::home_has_fuel(h, env.storages, env.materials, &self.life),
                )
            });
        let spouse_activity = c
            .spouse
            .map(|p| self.states[p].supply_activity)
            .filter(|&a| a == 1 || a == 2)
            .map(|a| a as u32);
        let view = CitizenView {
            home_food,
            home_fuel,
            spouse_activity,
            doctor: self.states[i].doctor.is_some(),
            age: c.age,
            school: self.states[i].school.is_some(),
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
                // `FUN_14011ada0` once for the home-stocking activities.
                let home_reachable = (a == activity::HOME_FOOD || a == activity::HOME_FUEL) && {
                    let town = TownView {
                        size: env.cells.size,
                        types: &env.cells.types,
                        regions: &self.regions,
                        occupants: &env.cells.occupants,
                        objects: &env.cells.objects,
                        entities,
                        happiness: &env.construction.happiness,
                    };
                    let cell = town.cell_of(self.characters[i].position);
                    self.states[i]
                        .home
                        .is_some_and(|h| town.can_reach(cell, h, env.rng))
                };
                let carrying = self.states[i].carried.used(env.materials) > 0;
                let done = if (home_reachable || a == activity::HERBS) && carrying {
                    // `FUN_140111160` / `FUN_1401108b0` / `FUN_140111d60`: first drop off what
                    // is carried, then this activity again.
                    let (mut sim, jobs) = self.sim(citizens, env, groups, adult, entities);
                    sim.drop_off(i);
                    jobs.drain(&mut sim);
                    ai.queued = Some(a);
                    true
                } else if matches!(
                    a,
                    activity::EAT
                        | activity::EAT_URGENT
                        | activity::WARM
                        | activity::WARM_URGENT
                        | activity::HOME_FOOD
                        | activity::HOME_FUEL
                        | activity::HERBS
                        | activity::DOCTOR
                        | activity::SCHOOL
                        | activity::PLAY
                ) {
                    let town = TownView {
                        size: env.cells.size,
                        types: &env.cells.types,
                        regions: &self.regions,
                        occupants: &env.cells.occupants,
                        objects: &env.cells.objects,
                        entities,
                        happiness: &env.construction.happiness,
                    };
                    let mut needs = crate::needs::Needs {
                        town: &town,
                        sectors: &mut self.sectors,
                        sites: env.sites,
                        storages: env.storages,
                        materials: env.materials,
                        homes: &mut env.construction.homes,
                        temperature: env.temperature,
                        life: &self.life,
                        carry: self.carry,
                        terrain: env.terrain,
                        size: env.cells.size,
                        rng: env.rng,
                        healers: &env.construction.healers,
                        workplaces: &env.construction.workforce.workplaces,
                        attendance: &mut env.construction.attendance,
                        citizens,
                        reschool: false,
                    };
                    let mut who = crate::needs::Who {
                        ai,
                        ch: &mut self.characters[i],
                        state: &mut self.states[i],
                        group,
                    };
                    let done = match a {
                        activity::EAT | activity::EAT_URGENT => needs.eat(&mut who),
                        activity::WARM | activity::WARM_URGENT => needs.warm(&mut who),
                        activity::HOME_FOOD => needs.stock_home(&mut who, false, home_reachable),
                        activity::HOME_FUEL => needs.stock_home(&mut who, true, home_reachable),
                        activity::HERBS => needs.herbs(&mut who),
                        activity::DOCTOR => needs.doctor(&mut who, i),
                        activity::SCHOOL => needs.school(&mut who),
                        _ => needs.play(&mut who, i),
                    };
                    if needs.reschool {
                        // `FUN_14011af20(c, 1)`.
                        let mut born = Vec::new();
                        self.with_people(
                            citizens,
                            env,
                            entities,
                            &mut |_, m, _| m.clone(),
                            &mut born,
                            |p| p.leave_service(i, crate::life::service::SCHOOL, true),
                        );
                    }
                    let outbox = std::mem::take(&mut self.characters[i].stack.outbox);
                    if !outbox.is_empty() {
                        let (mut sim, jobs) = self.sim(citizens, env, groups, adult, entities);
                        for m in outbox {
                            jobs.fail(m.a, i, m.b, &mut sim);
                        }
                        jobs.drain(&mut sim);
                    }
                    done
                } else if a == activity::DROP_OFF {
                    // `FUN_14011b330`.
                    let (mut sim, jobs) = self.sim(citizens, env, groups, adult, entities);
                    sim.drop_off(i);
                    jobs.drain(&mut sim);
                    self.characters[i].stack.is_active()
                } else if a == activity::WORK || a == activity::EMERGENCY {
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
                        happiness: &env.construction.happiness,
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
            let ground_cell = move |c: i32| cell_ground(size, terrain, c);
            let mut geometry = StorageGeometry {
                town,
                sectors,
                ground: &ground_cell,
            };
            let mut rank =
                |a: i32, b: i32| crate::stock::StorageSearchMap::rank(&mut geometry, a, b);
            let Some(plan) = loiter(
                town,
                cell,
                state.home,
                rules.loiter_range,
                rng,
                ground,
                &mut rank,
            ) else {
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

/// Entities placed during the update (piles, plantings) join the views; the original reads
/// its entities live.
fn refresh_views(entities: &mut Vec<EntityView>, env: &Env) {
    if entities.len() != env.placed.len() {
        *entities = entity_views(env.placed, env.templates, env.construction);
    }
}

/// The parts of each placed entity the activities read.
pub fn entity_views(
    placed: &[Placed],
    templates: &[Template],
    construction: &crate::construction::Construction,
) -> Vec<EntityView> {
    let mut views: Vec<EntityView> = placed
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
        .collect();
    // `FUN_14004b080`: registered, and not the unhappy kind.
    for (&e, site) in &construction.sites {
        let d = construction.templates[site.template].happiness;
        if let (Some(d), Some(v)) = (d, views.get_mut(e)) {
            if site.happiness_registered && d.kind != 5 {
                v.happiness = Some(d.pad);
            }
        }
    }
    views
}

/// An entity's StorageComponent: a construction site's (houses, workplaces), else a town
/// storage's.
fn storage_of(env: &Env, entity: usize) -> Option<usize> {
    env.construction
        .sites
        .get(&entity)
        .map(|s| s.storage)
        .or_else(|| {
            env.sites
                .iter()
                .find(|s| s.entity == entity)
                .map(|s| s.storage)
        })
}

/// FUN_1400a48b0 without bridge picking (same ground query as MoveTo).
fn cell_ground(size: usize, terrain: &HeightGrid, cell: i32) -> [f32; 3] {
    let n = size as i32;
    let h = (n >> 1) as f32 - 0.5;
    let (x, y) = ((cell % n) as f32 - h, (cell / n) as f32 - h);
    let mut normal = [0.0; 3];
    [x, y, terrain.ground(x, y, &mut normal)]
}
