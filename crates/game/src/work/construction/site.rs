//! What a building site holds: its template, production, workers, decals, piles and the work it
//! offers, and the town's list of sites (`Construction`).

use super::*;

/// The parts of a building's descriptions construction reads.
#[derive(Debug, Clone, Default)]
pub struct SiteTemplate {
    /// `Template\X.rsc:resource`.
    pub reference: String,
    /// The controllers in component order.
    pub controllers: Vec<Controller>,
    // +0x20; +0x30
    /// The materials to build it: `(count, raw material)`.
    pub materials: Vec<(i32, usize)>,
    /// The building work needed, in work units.
    pub work: i32,
    // +0x2c
    /// ClearAreaDescription: present, flatten/stamp flag.
    pub clear: Option<banished_formats::game::ClearAreaDescription>,
    // +0x24
    /// WorkDescription (most workers once built).
    pub max_workers: i32,
    // +0x34; +0x30; +0x24
    /// Scale the most workers by the footprint's area over this many cells (from the work
    /// description).
    pub per_area: Option<i32>,
    // +0x20; +0x28; +0x2c
    /// Whether the player may change the most workers in the building's window.
    pub adjustable: bool,
    /// The fewest workers the player may set.
    pub min_jobs: i32,
    /// The most workers the player may set.
    pub max_jobs: i32,
    // +0x20
    /// WorkPlaceDescription.
    pub profession: Option<usize>,
    /// ConsumeProduceDescription, resolved.
    pub produce: Option<ProduceTemplate>,
    /// StorageDescription: accepted flags, capacity, per footprint cell.
    pub storage: Option<(u32, i32, bool)>,
    /// ModelDescription meshes per variant (construction stages, the last finished).
    pub meshes: Vec<usize>,
    /// ResidenceDescription.
    pub residence: Option<banished_formats::game::ResidenceDescription>,
    /// AttendanceDescription capacity.
    pub attendance: Option<i32>,
    /// HerbalistDescription (gatherer, herbalist), resolved.
    pub herbalist: Option<crate::production::herbalist::HerbalistTemplate>,
    // +0x28
    /// RadiusDescription.
    pub radius: Option<i32>,
    // +0x20
    /// AutoPickupDescription.
    pub auto_pickup: Option<bool>,
    /// ForesterDescription, resolved.
    pub forester: Option<crate::production::forester::ForesterTemplate>,
    /// HappinessDescription.
    pub happiness: Option<banished_formats::game::HappinessDescription>,
    /// FishingDescription, resolved.
    pub fishing: Option<crate::production::fishing::FishingTemplate>,
    /// HuntDescription.
    pub hunt: Option<banished_formats::game::HuntDescription>,
    /// CropFieldDescription.
    pub crop_field: Option<crate::production::farm::CropFieldTemplate>,
    /// OrchardDescription.
    pub orchard: Option<crate::production::orchard::OrchardTemplate>,
    // +0x20
    /// PastureDescription: the stock slot of its products' limit.
    pub pasture: Option<i32>,
    /// DiseaseDescription (fields, orchards, pastures).
    pub disease: Option<banished_formats::game::DiseaseDescription>,
    /// CemeteryDescription's tombstone template and its TombstoneDescription.
    pub cemetery: Option<crate::people::cemetery::CemeteryTemplate>,
    /// MarketDescription.
    pub market: Option<banished_formats::game::MarketDescription>,
    // ModelComponent +0x5c
    /// StatusIconDescription (presentation), and the finished model's height its icons stand on
    /// (`ModelComponent`).
    pub status_icon: Option<banished_formats::game::StatusIconDescription>,
    /// The height of the status icon above the building, if it has one.
    pub icon_height: Option<f32>,
    /// DestroyDescription, resolved.
    pub destroy: Option<crate::work::destroy::DestroyTemplate>,
    /// FireDescription (`Template\Common.rsc:fire*`).
    pub fire: Option<banished_formats::game::FireDescription>,
    /// ImmigrationDescription, resolved.
    pub immigration: Option<crate::people::immigration::ImmigrationTemplate>,
    /// TradeDescription, resolved.
    pub trade: Option<crate::commerce::trade::TradeTemplate>,
    // +0x68; +0x6c
    /// TradeDescription /: the visitors' disease risk trading brings, and its months.
    pub trade_risk: Option<(f32, i32)>,
    /// The pile template of each BuildDescription material (the refunds' piles).
    pub material_templates: Vec<Option<usize>>,
}

/// A resolved [`banished_formats::game::ConsumeProduceDescription`].
#[derive(Debug, Clone, Default)]
pub struct ProduceTemplate {
    /// The recipes.
    pub recipes: Vec<Recipe>,
    // +0x40
    /// Work cycles per production.
    pub cycles: i32,
    // +0x50
    /// Work type (tool mesh, animation, walk).
    pub work_type: u32,
    // +0x38/+0x3c
    /// A message to the building's component of that type after a production.
    pub message: (u32, i32),
    // +0x44
    /// Seconds per cycle.
    pub duration: f32,
    // +0x48
    /// Productions before exhaustion (−1 never).
    pub lifetime: i32,
    // +0x4c
    /// Initial selector.
    pub selector: i32,
}

/// A production alternative: inputs (raw material, count), the output pile template and its
/// raw material, and the stock group the limit check reads.
#[derive(Debug, Clone, Default)]
pub struct Recipe {
    /// What it uses: `(raw material, count)`.
    pub inputs: Vec<(usize, i32)>,
    /// The template of the output pile.
    pub output: usize,
    /// The raw material of the output.
    pub output_material: usize,
    /// The stock group whose limit is checked.
    pub group: i32,
}

// +0x48; +0x50; +0x58; +0x68
/// ConsumeProduceComponent state (switched off, selector, weights, productions so far).
#[derive(Debug, Clone, Default)]
pub struct ProduceState {
    /// Whether production is switched off.
    pub disabled: bool,
    /// The recipe chosen in the window.
    pub selector: i32,
    /// The weight of each recipe, for choosing what to make.
    pub weights: Vec<f32>,
    /// The productions so far.
    pub produced: i32,
}

// +0x58; +0x68; +0x48; +0x4c
/// `ConsumeProduce` work state: the worker at each use point, each output point's state (0 free, 1
/// a production is under way, 2 holds output), every output point was taken, work cycles towards
/// the next production.
#[derive(Debug, Clone, Default)]
pub struct ProduceWork {
    /// The worker at each use point.
    pub workers: Vec<Option<usize>>,
    /// The state of each output point: 0 free, 1 a production is under way, 2 holds output.
    pub points: Vec<u8>,
    /// Whether every output point was taken.
    pub full: bool,
    /// The work cycles towards the next production.
    pub cycles: i32,
}

// vtable 0x140169400; +0x30; +0x28; +0x2c
/// DecalComponent (0x38 bytes, serialized alpha, material): setup takes the description's alpha;
/// entity event 1 (the ClearArea's state 2) and the build's state 8 make it opaque; a collapse
/// switches to the DestroyDescription's material. Each change re-adds the map decal. See
/// docs/DECALS.md.
///
/// Original: `FUN_140120ca0`, `FUN_14011ccc0`, `FUN_14010cfb0`, `FUN_140122aa0`, `FUN_140120c30`.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Decal {
    // +0x30
    /// is 1.
    pub opaque: bool,
    // +0x28
    /// Index into the description's materials.
    pub material: i32,
}

/// One building under construction (and, once built, its WorkComponent).
#[derive(Debug, Clone)]
pub struct Site {
    /// The building's entity.
    pub entity: usize,
    /// Index into [`Construction::templates`].
    pub template: usize,
    /// The building's state machine.
    pub machine: Machine,
    // +0x48; +0x4c
    /// BuildComponent progress, stages.
    pub progress: i32,
    /// The construction stages.
    pub stages: i32,
    // +0x28; +0x2c
    /// WorkComponent registered work id (−1) and kept priority.
    pub work_id: i32,
    /// The priority to keep for its work, if any.
    pub work_priority: Option<i32>,
    /// The building's StorageComponent (index into the town's storages).
    pub storage: usize,
    /// State 2 reached.
    pub complete: bool,
    // +0x48
    /// ClearAreaComponent: objects on the footprint when clearing began.
    pub clear_total: i32,
    /// The entity was destroyed during a state callback; the site goes once the machine has run.
    ///
    /// Original: `FUN_1400c8370`.
    pub destroyed: bool,
    // +0x48
    /// ConsumeProduceComponent (ResourceProduceComponent's for the others).
    pub produce: Option<ProduceState>,
    /// AutoPickupComponent.
    pub auto_pickup: Option<crate::production::herbalist::AutoPickup>,
    /// ForesterComponent's toggles.
    pub forester: Option<crate::production::forester::Forester>,
    // +0x28
    /// HappinessComponent: registered.
    pub happiness_registered: bool,
    /// FishingComponent.
    pub fishing: Option<crate::production::fishing::Fishing>,
    // +0x50
    /// HuntComponent: the hunting time gathered.
    pub hunt_time: Option<i64>,
    /// CropFieldComponent.
    pub crop_field: Option<crate::production::farm::CropField>,
    /// OrchardComponent.
    pub orchard: Option<crate::production::orchard::Orchard>,
    // +0x48
    /// MultiObjectComponent: the entities placed with this one (a pasture's shelter and fences), in
    /// order.
    pub parts: Vec<usize>,
    // +0x58; +0x4c
    /// MultiObjectComponent: the stages its construction counts (a bridge's cells, a fenced area's
    /// border cells); `BuildComponent::EnterGather` takes it for.
    pub multi_stages: Option<i32>,
    /// DiseaseComponent.
    pub disease: Option<crate::disasters::blight::Blight>,
    // +0x48
    /// CemeteryComponent: the grave cells.
    ///
    /// Original: `FUN_140047b60`.
    pub graves: Vec<i32>,
    /// DestroyComponent.
    pub destroy: Option<crate::work::destroy::Destroy>,
    /// FireComponent.
    pub fire: Option<crate::disasters::fire::Fire>,
    /// ImmigrationComponent.
    pub immigration: Option<crate::people::immigration::Immigration>,
    /// TradeComponent.
    pub trade: Option<crate::commerce::trade::Trade>,
    /// The ResidenceComponent while the residence is out of the town's (its states 3 to 7).
    pub home: Option<crate::people::life::Home>,
    // +0x28; +0x30; +0x20
    /// UIComponent (bytes, the UIDescription's): the window's remembered settings
    /// (`DialogInstance::SaveSettings`), empty until it is first closed.
    pub ui_settings: Vec<u8>,
}

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
            banished_formats::names::NameDatabase,
            banished_formats::names::NameDatabase,
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

// vtable 0x14015ee00
/// AttendanceComponent: who attends a school, chapel or doctor.
#[derive(Debug, Clone, PartialEq)]
pub struct Attendance {
    // +0x20
    /// AttendanceDescription.
    pub capacity: i32,
    // +0x28
    /// Attendees in arrival order.
    pub attendees: Vec<usize>,
    /// The workplace's profession (teacher, priest, doctor).
    pub profession: Option<usize>,
}

// vtable 0x14016a5a0
/// `RawMaterialComponent`: a loose pile.
///
/// Original: `FUN_14012e8d0`.
#[derive(Debug, Clone, PartialEq)]
pub struct Pile {
    /// The raw material (RawMaterialDescription of the pile's template).
    pub material: usize,
    // +0x28
    /// Units.
    pub count: i32,
    // +0x38
    /// Age; after 3600 s the pile is destroyed.
    ///
    /// Original: `FUN_14012e600`.
    pub age: i64,
    /// The Pickup work its WorkComponent registered.
    ///
    /// Original: `FUN_14012e6a0`.
    pub work: i32,
    /// The work's category mask.
    ///
    /// Original: `FUN_14012e950`.
    pub mask: u32,
    /// A priority to keep from the work at its cell, applied on activation.
    ///
    /// Original: `FUN_1400fe220`.
    pub priority: Option<i32>,
    /// The workplace whose AutoPickup gets the Pickup work once registered.
    pub auto_pickup: Option<usize>,
    // +0x48; +0x50
    /// The Pickup work's (0 free, 1 claimed, 2 picked up) and (claimer). Kept on the pile: one
    /// Pickup try claims several piles' works.
    pub state: i32,
    /// The citizen who claimed the pile, if any.
    pub claimer: Option<usize>,
}

// +0x48; +0x58; +0x68; +0x70..+0x72
/// `ClearArea` state: claimed resources (entity → citizen), claimed cells, template filter,
/// resources / cells / piles.
#[derive(Debug, Clone, Default)]
pub struct ClearWork {
    /// The claimed resources: entity → citizen.
    pub resources: BTreeMap<usize, usize>,
    /// The claimed cells: cell → citizen.
    pub cells: BTreeMap<i32, usize>,
    /// Only resources of this template are cleared, when set.
    pub filter: Option<String>,
    /// Whether natural resources are cleared.
    pub clear_resources: bool,
    /// Whether cells are cleared.
    pub clear_cells: bool,
    /// Whether piles are cleared.
    pub clear_piles: bool,
}

// +0x48; +0x50
/// `Pickup` work: its state lives on the [`Pile`] it belongs to.
#[derive(Debug, Clone, Default)]
pub struct PickupWork;

// +0x48; +0x58; +0x68
/// `GatherRawMaterials` state (delivered per requirement, in transit, workers).
#[derive(Debug, Clone, Default)]
pub struct Gather {
    /// The materials delivered, for each requirement.
    pub stored: Vec<i32>,
    /// The materials on their way, for each requirement.
    pub transit: Vec<i32>,
    /// The workers.
    pub workers: BTreeSet<usize>,
}

// +0x48; +0x4c; +0x50; +0x60
/// `BuildStructure` state (destroy flag, point cursor, occupied build points, workers).
#[derive(Debug, Clone, Default)]
pub struct BuildWork {
    /// Whether this is demolition instead of construction.
    pub destroy: bool,
    /// The next build point to try.
    pub cursor: i32,
    /// Which build points are taken.
    pub occupied: Vec<bool>,
    /// The workers.
    pub workers: BTreeSet<usize>,
}

impl SiteTemplate {
    /// The default number of workers of a building of `footprint` cells.
    ///
    /// Original: `WorkComponent::MaxWorkers`, `FUN_140133a90`.
    pub fn default_max_workers(&self, footprint: (i32, i32)) -> i32 {
        let Some(area) = self.per_area else {
            return self.max_workers;
        };
        let n = self.max_workers;
        // `DAT_14017884c` = 1.0.
        let scaled = ((footprint.0 * footprint.1) as f32 / area as f32 * n as f32 + 1.0) as i32;
        if scaled <= n {
            scaled.max(1)
        } else {
            n
        }
    }
}
