//! The citizens' CharacterComponent and CitizenAIComponent run together each simulation
//! iteration. See docs/CITIZEN-AI.md.
//!
//! Order per iteration: animations advance, every character runs its actions and moves, "sequence
//! finished" messages run the AI again with zero time, then the AI updates.
//!
//! The citizens' scheduled components (CitizenComponent, CitizenAIComponent and the
//! CharacterComponent's notify) run for the citizens bucket 1 visited this iteration, with the
//! time since each one's last visit; so do the sites' state machines, fires, blight, hunts,
//! attendance, residences and piles in bucket 3 (`town::Env::visits`).
//!
//! Deviation: these run after the bucket pass (system by system, each in visiting order)
//! instead of interleaved with it entity by entity, so random draws can come in another order.
//!
//! This file holds the agents' state and set-up; the rest is split by topic: `update` (the
//! iteration), `thinking` (the AI), `lives`, `commands`, `paths` and `views`.
//!
//! Original: `AnimationManager::Update`; the entity manager's bit-`0x04` components;
//! `FUN_140114810` (sequence finished); `FUN_1400c9560` (the buckets).

mod commands;
mod lives;
mod paths;
mod thinking;
mod update;
mod views;
pub use views::entity_views;
use views::{refresh_views, storage_of};

use banished_formats::game::CharacterDescription;
use banished_formats::types::AnimationGroup;

use crate::behaviour::activities::{loiter, EntityView, StorageGeometry, TownView};
use crate::behaviour::ai::{activity, CitizenAi, CitizenView, Thresholds};
use crate::behaviour::character::{Action, Character, Component, Message, MoveTo, World};
use crate::nature::seeding::{Cells, Placed, Template};
use crate::pathfind::{regions, Pathfinder, WALKABLE};
use crate::people::citizens::Citizen;
use crate::random::Random;
use crate::scheduler::ticks;
use crate::sectors::SectorGraph;
use crate::terrain::{CellType, HeightGrid};
use crate::town::{Env, Sim};
use crate::work::stock::{find_storage, RawMaterial, Storage, StorageSite};

/// `DAT_140178818`: children's model and walk scale (`FUN_14011a700`).
const CHILD_SCALE: f32 = 0.66;

/// `DAT_1401788fc`: cooldown set by the fetch-tool and fetch-clothing activities.
const FETCH_COOLDOWN: f32 = 75.0;

/// The CitizenDescription values the AI reads.
#[derive(Debug, Clone, Copy, Default)]
pub struct CitizenRulesAi {
    /// The thresholds of the citizen's needs.
    pub thresholds: Thresholds,
    // +0x6c; +0xa4
    /// The starting health.
    pub health: f32,
    /// The starting happiness.
    pub happiness: f32,
    // +0xbc
    /// Loiter search range.
    pub loiter_range: i32,
    // +0x130
    /// How long clothing lasts, in seconds (the citizen description's value multiplied by 300).
    ///
    /// Original: `FUN_140117730`.
    pub clothing_duration: f32,
    /// The highest health.
    pub health_max: f32,
    /// Midpoint for food, quarter-point for cold status warnings.
    ///
    /// Original: `FUN_140118340`.
    pub hunger_limit: f32,
    /// The cold level of the cold warning.
    pub cold_limit: f32,
    /// Whether a citizen without a workplace shows no job warning, when no professions are loaded.
    pub profession_allows_idle: bool,
    // +0x110
    /// CitizenDescription: the tool type CitizenUI calls "Iron" (others are "Steel").
    pub tool_grade: i32,
    // CitizenDescription +0xfc, +0x100, +0x104 (ints) and +0x108, +0x10c (°F); used by FUN_1400fc3c0.
    /// The rank of work a citizen takes at once, before it has waited.
    pub work_near: i32,
    /// The rank of work a citizen takes after it has waited the longest.
    pub work_far: i32,
    /// The longest wait, in months (of 300 seconds).
    pub work_minutes: i32,
    /// The rank of work a citizen always takes.
    pub work_ok: i32,
    /// The temperature at or above which a citizen takes any work in range.
    pub work_cold: f32,
    // +0x114
    /// CitizenDescription: work duration factor without a tool.
    ///
    /// Original: `FUN_1400cebf0`.
    pub no_tool_factor: f32,
    // +0x118
    /// The education a citizen counts as educated with.
    pub education: i32,
    // +0xf8
    /// How far a firefighter looks for water.
    ///
    /// Original: `FUN_14008d6a0`.
    pub water_distance: i32,
}

/// Citizen fields read/written by equipment activities and Citizen messages 1/6/7.
#[derive(Debug, Clone, Default)]
pub struct CitizenState {
    /// The health.
    pub health: f32,
    /// The happiness.
    pub happiness: f32,
    /// The hunger.
    pub hunger: f32,
    /// The cold.
    pub cold: f32,
    /// Whether the citizen is sick.
    pub sick: bool,
    /// The work it is assigned, if any (*inferred*).
    pub work: Option<usize>,
    // +0xf0; +0x280
    /// StatusIconComponent (Citizen points to that component).
    pub status: u32,
    // +0xa8
    /// The tool level shown in the citizen window.
    pub tool_status: i32,
    // +0xac
    /// The uses left on its tool.
    pub tool_uses: i32,
    // +0x1e0
    /// The clothing level shown in the citizen window.
    pub clothing_status: i32,
    // +0x1e4
    /// The clothing level: 0 none, 1 or 2.
    pub clothing: i32,
    // +0x1e8
    /// The time its clothing lasts.
    pub clothing_remaining: i64,
    // +0x54
    /// Flags of health events.
    pub health_events: u32,
    // +0x88
    /// Status flags.
    pub flags: u32,
    // +0xb0
    /// The supply errand it runs for its home: 1 food, 2 firewood.
    pub supply_activity: i32,
    // +0x30
    /// The id of the work it does (−1 none).
    pub job: i32,
    // +0x238
    /// The home, if any.
    pub home: Option<usize>,
    /// What it carries.
    pub carried: Storage,
    // +0x1f0
    /// Profession (index into the profession list).
    pub profession: usize,
    // +0x248
    /// Workplace entity.
    pub workplace: Option<usize>,
    /// The tool ShowTool attached (−1 none, −2 carried goods).
    pub shown_tool: i32,
    // +0x240
    /// Family slot in the home's ResidenceComponent (−1 none).
    pub home_slot: i32,
    // +0x4c
    /// Pregnancy (positive, counting down) or the wait after a birth (negative).
    pub reproduction: f32,
    // +0x58, +0x90
    /// Time towards the next health step.
    pub health_timer: i64,
    /// Time towards the next happiness step.
    pub happiness_timer: i64,
    // +0x98
    /// Every kind of food eaten.
    pub foods: Vec<usize>,
    // +0x260
    /// The children and other dependents, in ring order.
    pub dependents: Vec<usize>,
    // +0x218
    /// Time attending school.
    pub school_time: i64,
    // +0x208
    /// The school's AttendanceComponent entity (`Citizen::school` mirrors it).
    pub school: Option<usize>,
    // +0x228, +0x230
    /// The doctor's AttendanceComponent entity.
    pub doctor: Option<usize>,
    /// The chapel's AttendanceComponent entity.
    pub priest: Option<usize>,
    // +0x68
    /// The disease (index into the disease list).
    pub disease: Option<usize>,
    // +0x70
    /// The illness left while sick.
    pub disease_left: i64,
    /// The immunity left after an illness.
    pub immunity: i64,
    // +0x78
    /// Time to the next spread attempt.
    pub spread: i64,
    // +0x60
    /// Time since the last disease check.
    pub disease_clock: i64,
    // +0x84
    /// The work count the happiness compares with `_workForHappiness`; never written in 1.0.7, so
    /// 0.
    pub field_84: i32,
    // +0x220
    /// Has had a school place.
    pub schooled: bool,
    // +0x1f8
    /// The profession whose work it had an accident at.
    ///
    /// Original: `FUN_14011bb20`.
    pub accident: Option<usize>,
    // +0x200
    /// Time since the last injury check.
    pub accident_timer: i64,
}

impl CitizenState {
    /// The state of a new citizen.
    ///
    /// Original: `FUN_140119c40`, set-up `FUN_140119520`.
    pub fn newborn(rules: &crate::people::life::LifeRules) -> CitizenState {
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
    /// Handle a message about equipment (a tool or clothing given, worn out or used).
    ///
    /// Original: the equipment branches of `FUN_140117730` and `FUN_1401152b0`.
    pub fn message(&mut self, m: Message, rules: CitizenRulesAi) {
        match m.message {
            1 => {
                crate::people::status::set_icon(&mut self.status, 4, false, false);
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

/// The citizens' agents: each citizen's AI and character, and the state the iteration needs.
pub struct Agents {
    /// The citizens' characters.
    pub characters: Vec<Character>,
    /// The citizens' AI.
    pub ais: Vec<CitizenAi>,
    /// The citizens' states.
    pub states: Vec<CitizenState>,
    /// The CitizenDescription for the CitizenComponent's update.
    pub life: crate::people::life::LifeRules,
    // +0xa0
    /// The living citizens, in activation order.
    pub registry: Vec<usize>,
    /// Log entries the citizens asked for since the caller last took them.
    pub events: Vec<crate::people::life::LifeEvent>,
    // GameGlobal +0xa8
    /// `Game/Disease/Diseases.rsc` (`GameGlobal`).
    pub diseases: Vec<banished_formats::game::Disease>,
    // +0x1a8
    /// The town's disease level.
    pub town_disease: f32,
    /// The character description new citizens get.
    desc: CharacterDescription,
    /// A new citizen's carried storage.
    carried: Storage,
    /// Citizens queued for deletion this iteration (`FUN_14011c2a0`), with the reason.
    dying: Vec<(usize, i32)>,
    // +0x50
    /// The laborer profession's carry amount (what home stocking carries at least).
    pub carry: i32,
    sectors: SectorGraph,
    pathfinder: Pathfinder,
    regions: Vec<u16>,
    region_types: Vec<CellType>,
    /// The [`Cells::types_revision`] `region_types` was last brought up to.
    types_seen: u64,
    /// How far `Cells::sector_marks` has been taken in.
    marks_seen: usize,
    rules: CitizenRulesAi,
}

impl Agents {
    /// New agents with no citizens.
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
            life: crate::people::life::LifeRules::default(),
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
            types_seen: cells.types_revision,
            marks_seen: cells.sector_marks.len(),
            rules,
        }
    }

    /// The AI's rules, from the citizen description.
    pub fn rules(&self) -> CitizenRulesAi {
        self.rules
    }

    /// The work and construction context over these citizens and `env`.
    fn sim<'s, 't>(
        &'s mut self,
        citizens: &[Citizen],
        env: &'s mut Env<'_, 't>,
        groups: &'s [Option<&'s AnimationGroup>],
        adult: &'s [bool],
        entities: &'s [EntityView],
    ) -> (Sim<'s, 't>, &'s mut crate::work::jobs::Jobs) {
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
                rebuilds: &mut c.rebuilds,
                site_dt: 0,
                water: &mut c.water,
                game_flags: c.game_flags,
                precipitation: c.precipitation,
                population: c.population,
                visitors: &mut c.visitors,
                trading_posts: &mut c.trading_posts,
                trade_value: &mut c.trade_value,
                known_materials: &mut c.known_materials,
                known: &mut c.known,
                pending_roads: &mut c.pending_roads,
                terraform_requests: &mut c.terraform_requests,
                flatten_requests: &mut c.flatten_requests,
                tornadoes: &mut c.tornadoes,
                tornado_kills: &mut c.tornado_kills,
                trackers: &mut c.trackers,
                decals: &mut c.decals,
                counters: &mut c.counters,
                trade_goods: &c.trade_goods,
                river: &c.river,
                pathfinder: &mut self.pathfinder,
                names: c.names.as_deref(),
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
}
