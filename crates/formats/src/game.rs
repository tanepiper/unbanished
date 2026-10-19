//! Gameplay resources from the game executable's registrations.
//!
//! Each resource has a struct and a reader function (`toolbar_description`, …). They live in
//! topic files and are re-exported here, so every path is `banished_formats::game::NAME`:
//! [`world`], [`interface`], [`entities`], [`nature`], [`particles`], [`construction`],
//! [`goods`], [`work`], [`people`] and [`disasters`].

mod construction;
mod disasters;
mod entities;
mod goods;
mod interface;
mod nature;
mod particles;
mod people;
mod work;
mod world;
pub use construction::{
    build_description, clear_area_description, create_bridge_description, create_drag_description,
    create_fenced_description, create_placed_description, create_removal_description,
    create_road_description, destroy_description, floor_description, terraform_description,
    BuildDescription, ClearAreaDescription, CreateBridgeDescription, CreateDragDescription,
    CreateFencedDescription, CreatePlacedDescription, CreateRemovalDescription,
    CreateRoadDescription, DestroyDescription, FloorDescription, TerraformDescription,
};
pub use disasters::{fire_description, tornado_description, FireDescription};
pub use entities::{
    character_description, component_description, decal_description, interact_description,
    interaction_points, model_description, point_list, CharacterDescription, ComponentDescription,
    DecalDescription, InteractDescription, ModelDescription, ModelVariant, NamedPoint, PointList,
};
pub use goods::{
    auto_pickup_description, market_description, raw_material_description, residence_description,
    storage_description, storage_location_description, trade_description, value_description,
    MarketDescription, Merchant, RawMaterialDescription, ResidenceDescription, StorageDescription,
    TradeDescription, TradeRow, ValueDescription,
};
pub use interface::{
    status_icon_description, toolbar_description, toolbar_item, ui_description,
    StatusIconDescription, ToolbarDescription, ToolbarItem, UiController, UiDescription,
};
pub use nature::{
    animal_description, fruit_description, growth_description, herd_description,
    natural_resource_description, spawn_description, AnimalDescription, GrowthDescription,
    HerdDescription, NaturalResourceDescription, SpawnEntry,
};
pub use particles::{
    particle_description, particle_system, tornado_particle, ParticleDescription, ParticleSlot,
    ParticleSystem, TornadoParticle,
};
pub use people::{
    attendance_description, cemetery_description, citizen_description, disease,
    disease_description, happiness_description, immigration_description, nomad_description,
    population_description, tombstone_description, CitizenDescription, Disease, DiseaseDescription,
    HappinessDescription, ImmigrationDescription, PopulationDescription, Scalar,
};
pub use work::{
    consume_produce_description, crop_field_description, fishing_description, forester_description,
    herbalist_description, hunt_description, livestock_description, orchard_description,
    pasture_description, profession, quarry_description, radius_description, work_description,
    workplace_description, ConsumeProduceDescription, FishingDescription, ForesterDescription,
    HerbalistDescription, HuntDescription, LivestockDescription, ProduceRecipe, Profession,
    QuarryDescription, WorkDescription, WorkPlaceDescription,
};
pub use world::{
    climate_generator, climate_header, environment_description, external_list, map_description,
    map_generator, terrain_generator, ClimateGenerator, ClimateHeader, EnvironmentDescription,
    EnvironmentMonth, MapDescription, MapGenerator, PlacedStructure, TerrainGenerator,
};

use crate::crc::type_id;
use crate::crs::{self, Reader};
use crate::error::{format_err, Result};
