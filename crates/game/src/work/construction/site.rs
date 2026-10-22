//! What a building site holds: its template, production, workers, decals, piles and the work it
//! offers, and the town's list of sites (`Construction`).

mod progress;
mod work;
pub use progress::Construction;
pub use work::{Attendance, BuildWork, ClearWork, Gather, PickupWork, Pile};

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
    pub clear: Option<unbanished_formats::game::ClearAreaDescription>,
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
    pub residence: Option<unbanished_formats::game::ResidenceDescription>,
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
    pub happiness: Option<unbanished_formats::game::HappinessDescription>,
    /// FishingDescription, resolved.
    pub fishing: Option<crate::production::fishing::FishingTemplate>,
    /// HuntDescription.
    pub hunt: Option<unbanished_formats::game::HuntDescription>,
    /// CropFieldDescription.
    pub crop_field: Option<crate::production::farm::CropFieldTemplate>,
    /// OrchardDescription.
    pub orchard: Option<crate::production::orchard::OrchardTemplate>,
    // +0x20
    /// PastureDescription: the stock slot of its products' limit.
    pub pasture: Option<i32>,
    /// DiseaseDescription (fields, orchards, pastures).
    pub disease: Option<unbanished_formats::game::DiseaseDescription>,
    /// CemeteryDescription's tombstone template and its TombstoneDescription.
    pub cemetery: Option<crate::people::cemetery::CemeteryTemplate>,
    /// MarketDescription.
    pub market: Option<unbanished_formats::game::MarketDescription>,
    // ModelComponent +0x5c
    /// StatusIconDescription (presentation), and the finished model's height its icons stand on
    /// (`ModelComponent`).
    pub status_icon: Option<unbanished_formats::game::StatusIconDescription>,
    /// The height of the status icon above the building, if it has one.
    pub icon_height: Option<f32>,
    /// DestroyDescription, resolved.
    pub destroy: Option<crate::work::destroy::DestroyTemplate>,
    /// FireDescription (`Template\Common.rsc:fire*`).
    pub fire: Option<unbanished_formats::game::FireDescription>,
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

/// A resolved [`unbanished_formats::game::ConsumeProduceDescription`].
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
