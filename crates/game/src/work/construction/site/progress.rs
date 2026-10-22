//! A site's construction progress (ConstructionComponent).

use super::*;

/// The job manager, professions, workforce and every construction site.
#[derive(Debug, Clone, Default)]
pub struct Construction {
    // +0x1b8; +0x190
    /// The town's commute optimisation (StockManager).
    pub commute: crate::behaviour::commute::Commute,
    /// The job manager.
    pub jobs: Jobs,
    /// The professions.
    pub professions: Professions,
    /// The citizens' workplaces and professions.
    pub workforce: Workforce,
    /// The building templates, as construction reads them.
    pub templates: Vec<SiteTemplate>,
    /// Entity → site.
    pub sites: BTreeMap<usize, Site>,
    // Game +0xf0 → +0xd0
    /// `Game →`: sites in their gathering and building states.
    pub site_list: Vec<usize>,
    /// Sites placed since the last iteration (activated before the next one).
    pub placed: Vec<usize>,
    /// Player commands for the next iteration.
    pub commands: Vec<Command>,
    /// Loose raw-material piles (RawMaterialComponent), by entity.
    pub piles: BTreeMap<usize, Pile>,
    /// NaturalResourceComponent flag `0x100000`: a removal sequence holds the resource.
    pub claimed_resources: BTreeSet<usize>,
    /// Piles created since the last iteration (registered with the entity manager and
    /// activated after the citizens' update).
    pub new_piles: Vec<usize>,
    /// Entities were created or removed (the models need refreshing).
    pub models_dirty: bool,
    /// Entities the player destroyed (RemovalTool cancel), before the next iteration.
    pub destroy: Vec<usize>,
    /// Finished residences' ResidenceComponents, by entity.
    pub homes: BTreeMap<usize, crate::people::life::Home>,
    // +0x60
    /// AttendanceComponents, by entity, and the town manager's registry.
    pub attendance: BTreeMap<usize, Attendance>,
    /// The town manager's list of schools, chapels and doctors.
    pub attendance_list: Vec<usize>,
    // +0x70
    /// The town manager's HealerComponents.
    pub healers: Vec<usize>,
    // +0x50
    /// The town manager's registered HappinessComponents, by entity.
    pub happiness: Vec<usize>,
    // Game +0x128 → +8; +0
    /// The calendar's seconds into the year, set before each iteration.
    pub calendar: f32,
    /// The calendar's year, set before each iteration.
    pub year: u32,
    /// FruitComponents of orchard trees, by entity.
    pub fruits: BTreeMap<usize, crate::production::orchard::Fruit>,
    // +0x2d
    /// NaturalResourceComponent set by a crop field: whether harvesting leaves a pile.
    pub keep_piles: BTreeMap<usize, bool>,
    /// Event-log entries the sites posted: type, severity, entity; the game formats them with the
    /// entity's toolbar name.
    ///
    /// Original: `FUN_14009a2d0`.
    pub site_events: Vec<(i32, i32, usize)>,
    // +0x00
    /// StorageLocationComponents joining (`true`) or leaving the town's storages, in order; the
    /// town applies them after the iteration's update.
    pub storage_changes: Vec<(usize, bool)>,
    // +0x40
    /// The town's cemeteries, in order.
    pub cemeteries: Vec<usize>,
    // +0x28
    /// TombstoneComponents and the time each has left.
    pub tombstones: BTreeMap<usize, i64>,
    /// Burials decided at a death (cemetery, grave cell), placed after the citizens' update.
    pub burials: Vec<(usize, i32)>,
    /// Entities a DestroyComponent puts in a torn-down building's place, created by the town after
    /// the update.
    ///
    /// Original: `FUN_1400c9d70`.
    pub rebuilds: Vec<crate::work::destroy::Rebuild>,
    /// The town's water sources (built with the map).
    ///
    /// Original: `FUN_1400a43f0`.
    pub water: crate::disasters::fire::Water,
    // Game +0xd0; FUN_14009d530 sets 0xf0 with disasters off, else 0.
    /// The game's flags (for example, disasters off).
    pub game_flags: u32,
    /// The sky's state is precipitation, set before each iteration.
    pub precipitation: bool,
    // +0xa8
    /// Town manager: the population, set before each update.
    pub population: i32,
    // +0x1a8; +0x1a0
    /// Town manager /: the visitors' disease risk and how long it lasts.
    pub visitors: (f32, i64),
    // +0x1b0
    /// Town manager: nomads taken in.
    pub nomads_admitted: i32,
    // +0x30
    /// Town manager: the trading posts.
    pub trading_posts: Vec<usize>,
    // +0x1ac
    /// Town manager: the value traded away.
    pub trade_value: i32,
    // +0xe8
    /// Map: the cells with a planned road, and their BuildRoad works.
    pub pending_roads: BTreeMap<i32, i32>,
    /// Tunnels to dig or fill back (TerraformComponent event 4 and removal), for the terrain's
    /// owner, and each terraformed entity's kept heights and cells.
    pub terraform_requests: Vec<crate::world::terraform::Terraform>,
    /// Requests to flatten the ground: cell rectangles (`x0, y0, x1, y1`, inclusive) whose
    /// cells' low corners go to height 0. They are applied to the terrain after the iteration.
    ///
    /// Original: `FUN_1400a35b0`.
    pub flatten_requests: Vec<[i32; 4]>,
    /// The tornadoes by entity, and the citizens they killed this iteration (death code 5; the
    /// citizens' pass applies it).
    pub tornadoes: BTreeMap<usize, crate::disasters::tornado::Tornado>,
    /// The citizens killed by tornadoes this iteration.
    pub tornado_kills: Vec<usize>,
    /// TrackerComponents by entity, and the stock manager's made/used counters.
    pub trackers: BTreeMap<usize, crate::work::tracker::Tracker>,
    /// DecalComponents that left their setup state, by entity (the others show at their
    /// description's alpha with material 0).
    pub decals: BTreeMap<usize, Decal>,
    /// What the town made and used since the last sample.
    pub counters: crate::work::tracker::StockCounters,
    /// The terraforming state of tunnels, by entity.
    pub terraforms: BTreeMap<usize, crate::world::terraform::TerraformState>,
    /// Built road cells Remove Roads marked, for the town to place their markers (cell,
    /// marker template).
    pub road_markers: Vec<(i32, usize)>,
    /// The goods with a ValueDescription, in the factory's order (`GameResources`).
    pub trade_goods: Vec<crate::commerce::trade::TradeGood>,
    // +0x1e0
    /// MapComponent 's first river centre line, the trader boats' way.
    pub river: Vec<[f32; 2]>,
    /// `Game/Names/Male` and `Female`, for the merchants' names.
    pub names: Option<
        std::sync::Arc<(
            unbanished_formats::names::NameDatabase,
            unbanished_formats::names::NameDatabase,
        )>,
    >,
    /// The town manager's known crops, orchard trees and livestock, as placed-object templates.
    ///
    /// Original: `FUN_140096ef0`.
    pub known: Vec<usize>,
    // +0x110
    /// The stock manager's known raw materials: the start profile's, then each as its first pile is
    /// set up or it is traded for.
    ///
    /// Original: `FUN_140097330`.
    pub known_materials: Vec<usize>,
}
