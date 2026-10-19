//! The simulation context jobs and construction run in: [`Sim`] implements the job manager's
//! work slots ([`WorkHost`]) and the building state callbacks ([`Callbacks`]), over the
//! citizens, the map, the storages and the construction state. See docs/JOBS.md.
//!
//! This file holds the context's state and a few free helpers; its methods are split by topic:
//! - `geometry`: cells, ground heights, reachability and work points;
//! - `orders`: a building's workers and the actions given to citizens;
//! - `sites`: building sites and buildings, marking them on the map, destroying them;
//! - `building`: carrying materials to a site and building it;
//! - `clearing`: clearing an area before building;
//! - `piles`: loose piles of goods on the ground;
//! - `storage`: picking up goods, finding a storage, dropping off, emptying a storage;
//! - `callbacks` and `work_host`: the two trait implementations.

mod building;
mod callbacks;
mod clearing;
mod geometry;
mod orders;
mod piles;
mod sites;
mod storage;
mod work_host;

use std::collections::{BTreeMap, BTreeSet};

use banished_formats::types::AnimationGroup;

use crate::behaviour::activities::{EntityView, StorageGeometry, TownView};
use crate::behaviour::agents::{CitizenRulesAi, CitizenState};
use crate::behaviour::character::{Action, Character, Component, Message, MoveTo};
use crate::nature::seeding::{Cells, Placed, Template};
use crate::pathfind::nearest_neighbour;
use crate::pathfind::WALKABLE;
use crate::random::Random;
use crate::scheduler::{seconds, ticks};
use crate::sectors::SectorGraph;
use crate::terrain::HeightGrid;
use crate::work::construction::{
    run, BuildWork, Callback, Callbacks, ClearWork, Gather, Machine, PickupWork, Pile, Site,
    SiteTemplate,
};
use crate::work::jobs::{JobEvent, Jobs, Work, WorkHost, WorkKind, NESTED};
use crate::work::professions::{Professions, Worker, Workforce};
use crate::work::stock::{find_material_storage, RawMaterial, Storage, StorageSite};
use crate::world::placement::{PlacementRules, Rect};

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
    /// Several materials at once (when `material` is null): the delivery drops everything
    /// carried.
    ///
    /// Original: `FUN_1400cf700` adds each one to the pickup.
    pub items: Vec<(usize, i32)>,
}

/// The world the citizens' update borrows (everything but the citizens themselves).
pub struct Env<'a, 't> {
    /// Wild animals (hunting).
    pub wildlife: &'a mut crate::nature::wildlife::Wildlife,
    /// The gameplay map.
    pub cells: &'a mut Cells,
    /// The placed objects.
    pub placed: &'a mut Vec<Placed>,
    /// The object templates.
    pub templates: &'a [Template],
    /// The ground's heights.
    pub terrain: &'a HeightGrid<'t>,
    /// The raw materials.
    pub materials: &'a [RawMaterial],
    /// The town's storages.
    pub storages: &'a mut [Storage],
    /// The storage sites.
    pub sites: &'a [StorageSite],
    /// The game's random numbers.
    pub rng: &'a mut Random,
    /// The construction manager.
    pub construction: &'a mut crate::work::construction::Construction,
    /// The stock manager's `(count, limit)` per slot from the last recount.
    pub stock: [(i32, i32); 21],
    // Game +0x130 → +0x18
    /// The environment's temperature.
    pub temperature: f32,
    /// This iteration's bucket visits (`EntityScheduler::take_visits`): placed entities by
    /// index, citizens from `scheduler::EXTRA`, each with the time since its last visit.
    pub visits: &'a [(usize, i64)],
}

/// Everything a work slot or a state callback touches.
pub struct Sim<'s, 't> {
    /// The citizens' characters.
    pub characters: &'s mut [Character],
    /// The citizens' states.
    pub states: &'s mut [CitizenState],
    /// Each citizen's animation group.
    pub groups: &'s [Option<&'s AnimationGroup>],
    /// Adult and not at school (the citizens assignment counts).
    pub adult: &'s [bool],
    /// The citizens' AI rules.
    pub rules: CitizenRulesAi,
    /// The map's size in cells.
    pub size: usize,
    /// The gameplay map.
    pub cells: &'s mut Cells,
    /// The walkable region of each cell.
    pub regions: &'s [u16],
    /// The sector graph.
    pub sectors: &'s mut SectorGraph,
    /// The ground's heights.
    pub terrain: &'s HeightGrid<'t>,
    /// The placed objects.
    pub placed: &'s mut Vec<Placed>,
    /// The object templates.
    pub templates: &'s [Template],
    /// A view of each entity.
    pub entities: &'s [EntityView],
    /// The town's storages.
    pub storages: &'s mut [Storage],
    /// The storage sites.
    pub storage_sites: &'s [StorageSite],
    /// The raw materials.
    pub materials: &'s [RawMaterial],
    /// The game's random numbers.
    pub rng: &'s mut Random,
    /// The stock manager's `(count, limit)` per slot from the last recount.
    pub stock: [(i32, i32); 21],
    /// The environment's temperature.
    pub temperature: f32,
    /// The professions.
    pub professions: &'s Professions,
    /// The workers assigned to each workplace.
    pub workforce: &'s mut Workforce,
    /// The building templates.
    pub site_templates: &'s [SiteTemplate],
    /// The buildings, by entity.
    pub sites: &'s mut BTreeMap<usize, Site>,
    /// The homes, by entity.
    pub homes: &'s mut BTreeMap<usize, crate::people::life::Home>,
    /// Who attends each service building, by entity.
    pub attendance: &'s mut BTreeMap<usize, crate::work::construction::Attendance>,
    /// The service buildings that people attend.
    pub attendance_list: &'s mut Vec<usize>,
    /// The herbalists and hospitals.
    pub healers: &'s mut Vec<usize>,
    /// The buildings that raise happiness nearby.
    pub happiness: &'s mut Vec<usize>,
    /// The calendar's seconds into the year.
    pub calendar: f32,
    /// The current year.
    pub year: u32,
    /// The orchards' fruit, by entity.
    pub fruits: &'s mut BTreeMap<usize, crate::production::orchard::Fruit>,
    /// The animals (livestock and their pastures).
    pub wildlife: &'s mut crate::nature::wildlife::Wildlife,
    /// Whether removing a resource leaves its piles behind, by resource.
    pub keep_piles: &'s mut BTreeMap<usize, bool>,
    /// Building events for the event log: `(kind, argument, entity)`.
    pub site_events: &'s mut Vec<(i32, i32, usize)>,
    /// Storages that opened or closed: `(entity, opened)`.
    pub storage_changes: &'s mut Vec<(usize, bool)>,
    /// The buildings to build again after they were destroyed.
    pub rebuilds: &'s mut Vec<crate::work::destroy::Rebuild>,
    /// The town's water sources (wells and shore points).
    pub water: &'s mut crate::disasters::fire::Water,
    // Game +0xd0
    /// The game's debug flags (all four disaster bits are set when disasters are off).
    pub game_flags: u32,
    // +0x10
    /// The sky's state is precipitation (environment = 0).
    pub precipitation: bool,
    // +0xa8
    /// The town's population.
    pub population: i32,
    // +0x1a8; +0x1a0
    /// The visitors' timer and the time of the last visitor.
    pub visitors: &'s mut (f32, i64),
    // +0xe8
    /// The planned road cells, each with its road-building work.
    pub pending_roads: &'s mut BTreeMap<i32, i32>,
    /// The pending changes to the ground.
    pub terraform_requests: &'s mut Vec<crate::world::terraform::Terraform>,
    /// The rectangles to flatten: `[x0, y0, x1, y1]`.
    pub flatten_requests: &'s mut Vec<[i32; 4]>,
    /// The tornadoes, by entity.
    pub tornadoes: &'s mut BTreeMap<usize, crate::disasters::tornado::Tornado>,
    /// The citizens a tornado killed.
    pub tornado_kills: &'s mut Vec<usize>,
    /// The production trackers, by entity.
    pub trackers: &'s mut BTreeMap<usize, crate::work::tracker::Tracker>,
    /// The ground decals of building sites, by entity.
    pub decals: &'s mut BTreeMap<usize, crate::work::construction::Decal>,
    /// The counts of goods made and used.
    pub counters: &'s mut crate::work::tracker::StockCounters,
    // +0x30; +0x1ac
    /// The town's trading posts.
    pub trading_posts: &'s mut Vec<usize>,
    /// The total value traded.
    pub trade_value: &'s mut i32,
    // +0x110; +0x120
    /// The raw materials the town knows; `known` holds the crops, orchard trees and livestock it
    /// knows.
    ///
    /// Original: `FUN_140097330`, `FUN_140096ef0`.
    pub known_materials: &'s mut Vec<usize>,
    /// The crops, orchard trees and livestock the town knows.
    pub known: &'s mut Vec<usize>,
    /// The goods traders can bring.
    pub trade_goods: &'s [crate::commerce::trade::TradeGood],
    /// The river's path.
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
    /// The cemeteries.
    pub cemeteries: &'s mut Vec<usize>,
    /// The tombstones, with the time each one stays.
    pub tombstones: &'s mut BTreeMap<usize, i64>,
    /// The burials waiting: `(cemetery, cell)`.
    pub burials: &'s mut Vec<(usize, i32)>,
    /// The buildings that have a site record.
    pub site_list: &'s mut Vec<usize>,
    /// The job events from this update.
    pub events: Vec<JobEvent>,
    /// The piles, by entity.
    pub piles: &'s mut BTreeMap<usize, Pile>,
    /// The resources a work has claimed.
    pub claimed: &'s mut std::collections::BTreeSet<usize>,
    /// The piles made in this update.
    pub new_piles: &'s mut Vec<usize>,
    /// Set when a model changed and must be drawn again.
    pub models_dirty: &'s mut bool,
    /// Works a ClearArea try asked the manager to offer ([`WorkHost::take_offers`]).
    pub offers: Vec<i32>,
    /// Scratch for `Sim::seeder` (planting removes nothing and plays no sounds).
    pub removed: Vec<usize>,
    /// The animation events to play: `(entity, effect)`.
    pub animation_events: Vec<(usize, String)>,
    /// Whether the offers get a second try (as an auto-pickup's do).
    ///
    /// Original: `FUN_1400fdaf0(.., 1)`.
    pub offers_again: bool,
    /// A ConsumeProduce try waiting on its pile offers ([`WorkHost::resume`]).
    pub(crate) produce_resume: Option<crate::work::produce::ProduceResume>,
    /// A pasture try waiting on its product pickups' offers ([`Sim::pasture_try`]).
    pub(crate) pasture_stage: bool,
}

/// A pile's work category mask from its material's flags.
///
/// Original: `FUN_14012e950`.
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

/// The ground at a cell's centre, on the terrain only (bridge floors are skipped).
///
/// Original: `FUN_1400a48b0` with `param_4` 1.
pub fn cell_ground(size: usize, terrain: &HeightGrid, cell: i32) -> [f32; 3] {
    let n = size as i32;
    let h = (n >> 1) as f32 - 0.5;
    let (x, y) = ((cell % n) as f32 - h, (cell / n) as f32 - h);
    let mut normal = [0.0; 3];
    [x, y, terrain.ground(x, y, &mut normal)]
}

/// The ground at a cell's centre: on a bridge's floor where one covers the cell above the
/// terrain, else on the terrain.
///
/// Original: `FUN_1400a48b0` with `param_4` 0, as nearly every caller passes.
pub fn floor_ground(
    floors: &crate::world::floor::Floors,
    size: usize,
    terrain: &HeightGrid,
    cell: i32,
) -> [f32; 3] {
    let n = size as i32;
    let h = (n >> 1) as f32 - 0.5;
    let (x, y) = ((cell % n) as f32 - h, (cell / n) as f32 - h);
    [x, y, floors.ground(size, terrain, x, y)]
}

/// The amount still to clear in an entity's area, over a grid of `n × n` cells that maps each
/// cell to its object.
///
/// Original: `FUN_14011d800`.
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
