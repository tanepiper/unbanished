//! The simulation context jobs and construction run in: [`Sim`] implements the job manager's
//! work slots ([`WorkHost`]) and the building state callbacks ([`Callbacks`]), over the
//! citizens, the map, the storages and the construction state. See docs/JOBS.md.
//!
//! This file holds the context's state and a few free helpers; its methods are split by topic:
//! - [`geometry`]: cells, ground heights, reachability and work points;
//! - [`orders`]: a building's workers and the actions given to citizens;
//! - [`sites`]: building sites and buildings, marking them on the map, destroying them;
//! - [`building`]: carrying materials to a site and building it;
//! - [`clearing`]: clearing an area before building;
//! - [`piles`]: loose piles of goods on the ground;
//! - [`storage`]: picking up goods, finding a storage, dropping off, emptying a storage;
//! - [`callbacks`] and [`work_host`]: the two trait implementations.

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
    /// Several materials at once (`material` null, `FUN_1400cf700` adds each to the pickup):
    /// the delivery drops everything carried.
    pub items: Vec<(usize, i32)>,
}

/// The world the citizens' update borrows (everything but the citizens themselves).
pub struct Env<'a, 't> {
    /// Wild animals (hunting).
    pub wildlife: &'a mut crate::nature::wildlife::Wildlife,
    pub cells: &'a mut Cells,
    pub placed: &'a mut Vec<Placed>,
    pub templates: &'a [Template],
    pub terrain: &'a HeightGrid<'t>,
    pub materials: &'a [RawMaterial],
    pub storages: &'a mut [Storage],
    pub sites: &'a [StorageSite],
    pub rng: &'a mut Random,
    pub construction: &'a mut crate::work::construction::Construction,
    /// The stock manager's `(count, limit)` per slot from the last recount.
    pub stock: [(i32, i32); 21],
    /// The environment's temperature (`Game +0x130 → +0x18`).
    pub temperature: f32,
    /// This iteration's bucket visits (`EntityScheduler::take_visits`): placed entities by
    /// index, citizens from `scheduler::EXTRA`, each with the time since its last visit.
    pub visits: &'a [(usize, i64)],
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
    pub homes: &'s mut BTreeMap<usize, crate::people::life::Home>,
    pub attendance: &'s mut BTreeMap<usize, crate::work::construction::Attendance>,
    pub attendance_list: &'s mut Vec<usize>,
    pub healers: &'s mut Vec<usize>,
    pub happiness: &'s mut Vec<usize>,
    /// The calendar's seconds into the year.
    pub calendar: f32,
    pub year: u32,
    pub fruits: &'s mut BTreeMap<usize, crate::production::orchard::Fruit>,
    /// The animals (livestock and their pastures).
    pub wildlife: &'s mut crate::nature::wildlife::Wildlife,
    pub keep_piles: &'s mut BTreeMap<usize, bool>,
    pub site_events: &'s mut Vec<(i32, i32, usize)>,
    pub storage_changes: &'s mut Vec<(usize, bool)>,
    pub rebuilds: &'s mut Vec<crate::work::destroy::Rebuild>,
    /// The town's water sources (wells and shore points).
    pub water: &'s mut crate::disasters::fire::Water,
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
    pub terraform_requests: &'s mut Vec<crate::world::terraform::Terraform>,
    pub flatten_requests: &'s mut Vec<[i32; 4]>,
    pub tornadoes: &'s mut BTreeMap<usize, crate::disasters::tornado::Tornado>,
    pub tornado_kills: &'s mut Vec<usize>,
    pub trackers: &'s mut BTreeMap<usize, crate::work::tracker::Tracker>,
    pub decals: &'s mut BTreeMap<usize, crate::work::construction::Decal>,
    pub counters: &'s mut crate::work::tracker::StockCounters,
    /// Town manager `+0x30` and `+0x1ac`.
    pub trading_posts: &'s mut Vec<usize>,
    pub trade_value: &'s mut i32,
    /// Town manager `+0x110` (known raw materials, `FUN_140097330`) and `+0x120` (known crops,
    /// orchard trees and livestock, `FUN_140096ef0`).
    pub known_materials: &'s mut Vec<usize>,
    pub known: &'s mut Vec<usize>,
    pub trade_goods: &'s [crate::commerce::trade::TradeGood],
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
    pub(crate) produce_resume: Option<crate::work::produce::ProduceResume>,
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

/// `FUN_1400a48b0` at a cell's centre, on the terrain only (the floors skipped: `param_4` 1).
pub fn cell_ground(size: usize, terrain: &HeightGrid, cell: i32) -> [f32; 3] {
    let n = size as i32;
    let h = (n >> 1) as f32 - 0.5;
    let (x, y) = ((cell % n) as f32 - h, (cell / n) as f32 - h);
    let mut normal = [0.0; 3];
    [x, y, terrain.ground(x, y, &mut normal)]
}

/// `FUN_1400a48b0` at a cell's centre (`param_4` 0, as nearly every caller passes): on a
/// bridge's floor where one covers the cell above the terrain.
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
