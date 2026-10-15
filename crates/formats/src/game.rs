//! Gameplay resources from the game executable's registrations.

use crate::crc::type_id;
use crate::crs::{self, Reader};
use crate::error::{format_err, Result};

/// `TerrainGenerator` (serializer `FUN_1400b6b60`): a New Game terrain profile.
#[derive(Debug, Clone)]
pub struct TerrainGenerator {
    pub string_table: Option<String>,
    pub name: String,
    pub flag: bool,
    /// Terrain, river and lake material instances.
    pub materials: [String; 3],
    /// Small / Medium / Large size records, 17 integers each in serialized (wire) order.
    pub sizes: Vec<[i32; 17]>,
}

pub fn terrain_generator(data: &[u8]) -> Result<TerrainGenerator> {
    let (_, value) = crs::parse(data, |r: &mut Reader, h| {
        if h.type_id != type_id("TerrainGenerator") {
            return format_err(format!(
                "expected TerrainGenerator, found {:#010x}",
                h.type_id
            ));
        }
        r.block(|r, _| {
            let string_table = r.external_ref()?;
            let name = r.string()?;
            let flag = r.bool()?;
            let materials = [r.string()?, r.string()?, r.string()?];
            let mut sizes = Vec::new();
            for _ in 0..r.count()? {
                let mut rec = [0i32; 17];
                for v in &mut rec {
                    *v = r.i32()?;
                }
                sizes.push(rec);
            }
            Ok(TerrainGenerator {
                string_table,
                name,
                flag,
                materials,
                sizes,
            })
        })
    })?;
    Ok(value)
}

/// One month of `EnvironmentDescription` lighting (32 bytes at `+0x38 + 0x20·month`).
///
/// `FUN_140098f90` interpolates between the current and next month; `FUN_140098da0` then calls
/// `SetFog(fog_start, fog_end, fog_color with alpha fog_max)` and
/// `SunLight::Set(FromYawPitch(sun_yaw, sun_pitch), sun_color, ambient_color)`.
/// Colours are `0xAARRGGBB`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EnvironmentMonth {
    pub ambient_color: u32,
    pub sun_color: u32,
    pub fog_color: u32,
    pub fog_start: f32,
    pub fog_end: f32,
    pub fog_max: f32,
    /// Degrees.
    pub sun_yaw: f32,
    /// Degrees above the horizon.
    pub sun_pitch: f32,
}

/// `EnvironmentDescription` (serializer `FUN_140099510`): weather and monthly lighting.
#[derive(Debug, Clone)]
pub struct EnvironmentDescription {
    pub template: Option<String>,
    /// Weather effects: (environment resource, ambient audio).
    pub weather: Vec<(Option<String>, Option<String>)>,
    pub months: Vec<EnvironmentMonth>,
}

pub fn environment_description(data: &[u8]) -> Result<EnvironmentDescription> {
    let (_, value) = crs::parse(data, |r: &mut Reader, h| {
        if h.type_id != type_id("EnvironmentDescription") {
            return format_err(format!(
                "expected EnvironmentDescription, found {:#010x}",
                h.type_id
            ));
        }
        r.block(|r, _| {
            let template = r.external_ref()?;
            let mut weather = Vec::new();
            for _ in 0..r.count()? {
                weather.push((r.external_ref()?, r.external_ref()?));
            }
            let mut months = Vec::with_capacity(12);
            for _ in 0..12 {
                months.push(EnvironmentMonth {
                    ambient_color: r.u32()?,
                    sun_color: r.u32()?,
                    fog_color: r.u32()?,
                    fog_start: r.f32()?,
                    fog_end: r.f32()?,
                    fog_max: r.f32()?,
                    sun_yaw: r.f32()?,
                    sun_pitch: r.f32()?,
                });
            }
            Ok(EnvironmentDescription {
                template,
                weather,
                months,
            })
        })
    })?;
    Ok(value)
}

/// `ClimateGenerator` (`FUN_140098470`): monthly weather thresholds used by the environment loop.
#[derive(Debug, Clone)]
pub struct ClimateGenerator {
    pub string_table: Option<String>,
    pub name: String,
    pub enabled: bool,
    /// Four serialized climate settings at in-memory offsets `+0x34..+0x40`.
    pub settings: [f32; 4],
    /// Four floats per month at `+0x44`; the weather selector reads the first three as cutoffs.
    pub monthly_thresholds: [[f32; 4]; 12],
    /// Three serialized climate settings at in-memory offsets `+0x104..+0x10c`.
    pub trailing_settings: [f32; 3],
}

pub fn climate_generator(data: &[u8]) -> Result<ClimateGenerator> {
    let (_, value) = crs::parse(data, |r: &mut Reader, h| {
        if h.type_id != type_id("ClimateGenerator") {
            return format_err(format!(
                "expected ClimateGenerator, found {:#010x}",
                h.type_id
            ));
        }
        r.block(|r, _| {
            let string_table = r.external_ref()?;
            let name = r.string()?;
            let enabled = r.bool()?;
            let mut settings = [0.0; 4];
            for value in &mut settings {
                *value = r.f32()?;
            }
            let mut monthly_thresholds = [[0.0; 4]; 12];
            for month in &mut monthly_thresholds {
                for value in month {
                    *value = r.f32()?;
                }
            }
            let mut trailing_settings = [0.0; 3];
            for value in &mut trailing_settings {
                *value = r.f32()?;
            }
            Ok(ClimateGenerator {
                string_table,
                name,
                enabled,
                settings,
                monthly_thresholds,
                trailing_settings,
            })
        })
    })?;
    Ok(value)
}

/// A structure placed relative to the start point (`FUN_14001c020` record, `FUN_1400a7490`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlacedStructure {
    pub template: String,
    /// Offset from the start point in cells.
    pub x: i32,
    pub y: i32,
    pub a: i32,
    pub b: i32,
    /// Rotation in degrees.
    pub rotation: i32,
}

/// `MapGenerator` (serializer `FUN_1400a7e50`): a New Game starting-condition profile. Field names
/// follow their confirmed consumers; offsets are the in-memory record's.
#[derive(Debug, Clone)]
pub struct MapGenerator {
    pub list: Option<String>,
    pub name: String,
    pub description: String,
    pub flag: bool,
    /// `+0x3c`: radius of the clear-ground start area (`FUN_1400a7bf0`).
    pub start_radius: i32,
    pub value_40: f32,
    pub citizen: String,
    /// `+0x60`: household groups `(flag, extra members up to)`.
    pub households: Vec<(bool, i32)>,
    /// `+0x70`: templates seeded over the map (`FUN_1400a6780`): rock, ore, tree.
    pub natural_resources: Vec<String>,
    /// `+0x80`.
    pub structures: Vec<PlacedStructure>,
    /// `+0x90`: population managers (deer).
    pub populations: Vec<String>,
    /// `+0xa0`: starting raw materials.
    pub raw_materials: Vec<String>,
    /// `+0xb0`: crop templates; `crop_picks` are chosen at random.
    pub crops: Vec<String>,
    /// `+0xc0`: orchard tree templates; `tree_picks` are chosen at random.
    pub orchard_trees: Vec<String>,
    /// `+0xd0`: livestock templates; `livestock_picks` are chosen at random.
    pub livestock: Vec<String>,
    pub crop_picks: i32,
    pub tree_picks: i32,
    pub livestock_picks: i32,
    /// `+0xec..+0xf4`.
    pub livestock_floats: [f32; 3],
    /// `+0xf8..+0x114`: `livestock_count` (`+0xf8`) then seven more integers.
    pub ints: [i32; 8],
    pub string_118: String,
    pub string_138: String,
    pub float_130: f32,
    pub int_150: i32,
    pub string_158: String,
    pub float_170: f32,
}

pub fn map_generator(data: &[u8]) -> Result<MapGenerator> {
    let (_, value) = crs::parse(data, |r: &mut Reader, h| {
        if h.type_id != type_id("MapGenerator") {
            return format_err(format!("expected MapGenerator, found {:#010x}", h.type_id));
        }
        r.block(|r, _| {
            let strings = |r: &mut Reader| -> Result<Vec<String>> {
                (0..r.count()?).map(|_| r.string()).collect()
            };
            let list = r.external_ref()?;
            let name = r.string()?;
            let description = r.string()?;
            let flag = r.bool()?;
            let start_radius = r.i32()?;
            let value_40 = r.f32()?;
            let citizen = r.string()?;
            let households = (0..r.count()?)
                .map(|_| Ok((r.bool()?, r.i32()?)))
                .collect::<Result<_>>()?;
            let natural_resources = strings(r)?;
            let structures = (0..r.count()?)
                .map(|_| {
                    Ok(PlacedStructure {
                        template: r.string()?,
                        x: r.i32()?,
                        y: r.i32()?,
                        a: r.i32()?,
                        b: r.i32()?,
                        rotation: r.i32()?,
                    })
                })
                .collect::<Result<_>>()?;
            let populations = strings(r)?;
            let raw_materials = strings(r)?;
            let crops = strings(r)?;
            let orchard_trees = strings(r)?;
            let livestock = strings(r)?;
            let (crop_picks, tree_picks, livestock_picks) = (r.i32()?, r.i32()?, r.i32()?);
            let livestock_floats = [r.f32()?, r.f32()?, r.f32()?];
            let mut ints = [0; 8];
            for v in &mut ints {
                *v = r.i32()?;
            }
            let string_118 = r.string()?;
            let string_138 = r.string()?;
            let float_130 = r.f32()?;
            let int_150 = r.i32()?;
            let string_158 = r.string()?;
            let float_170 = r.f32()?;
            Ok(MapGenerator {
                list,
                name,
                description,
                flag,
                start_radius,
                value_40,
                citizen,
                households,
                natural_resources,
                structures,
                populations,
                raw_materials,
                crops,
                orchard_trees,
                livestock,
                crop_picks,
                tree_picks,
                livestock_picks,
                livestock_floats,
                ints,
                string_118,
                string_138,
                float_130,
                int_150,
                string_158,
                float_170,
            })
        })
    })?;
    Ok(value)
}

/// `ExternalList`: an ordered list of resource references (e.g. `Game/Terrain/TerrainList.rsc`,
/// the New Game selectors filled by `FUN_14009f280`).
pub fn external_list(data: &[u8]) -> Result<Vec<String>> {
    let (_, value) = crs::parse(data, |r: &mut Reader, h| {
        if h.type_id != type_id("ExternalList") {
            return format_err(format!("expected ExternalList, found {:#010x}", h.type_id));
        }
        r.block(|r, _| {
            (0..r.count()?)
                .map(|_| Ok(r.external_ref()?.unwrap_or_default()))
                .collect()
        })
    })?;
    Ok(value)
}

/// The leading fields of a `ClimateGenerator` (New Game climate profile): string table, name key
/// and the "listed" flag at `+0x38`. The climate parameters that follow are not decoded yet.
#[derive(Debug, Clone)]
pub struct ClimateHeader {
    pub string_table: Option<String>,
    pub name: String,
    pub listed: bool,
}

pub fn climate_header(data: &[u8]) -> Result<ClimateHeader> {
    let (_, value) = crs::parse(data, |r: &mut Reader, h| {
        if h.type_id != type_id("ClimateGenerator") {
            return format_err(format!(
                "expected ClimateGenerator, found {:#010x}",
                h.type_id
            ));
        }
        r.block(|r, _| {
            let header = ClimateHeader {
                string_table: r.external_ref()?,
                name: r.string()?,
                listed: r.bool()?,
            };
            r.skip_rest();
            Ok(header)
        })
    })?;
    Ok(value)
}

/// One toolbar entry (`Toolbar`, serializer `FUN_1400d74e0`): a button or a sub-menu of the
/// in-game toolbar. `FUN_1400bede0` builds each toolbar level from the entries whose `parent`
/// matches, ordered by `order`.
#[derive(Debug, Clone)]
pub struct ToolbarItem {
    pub string_table: Option<String>,
    pub sprite_sheet: Option<String>,
    /// Tooltip string key.
    pub tooltip: String,
    /// Icon sprite name in `sprite_sheet`.
    pub sprite: String,
    /// Action kind (`+0x48`): 0 sub-menu, 1 build template, 6–10 time controls, …
    pub kind: u32,
    /// Parent toolbar (`+0x60`), `None` for the top level.
    pub parent: Option<String>,
    /// Template placed by this entry (`+0x58`), or the utility it opens.
    pub template: Option<String>,
    /// `+0x40`.
    pub value: u32,
    /// `+0x45`: listed in the toolbar.
    pub visible: bool,
    /// `+0x44`.
    pub enabled: bool,
    /// `+0x4c`.
    pub id: i32,
    /// `+0x50`: sort key within its parent.
    pub order: i32,
}

pub fn toolbar_item(data: &[u8]) -> Result<ToolbarItem> {
    let (_, value) = crs::parse(data, |r: &mut Reader, h| {
        if h.type_id != type_id("Toolbar") {
            return format_err(format!("expected Toolbar, found {:#010x}", h.type_id));
        }
        r.block(|r, _| {
            Ok(ToolbarItem {
                string_table: r.external_ref()?,
                sprite_sheet: r.external_ref()?,
                tooltip: r.string()?,
                sprite: r.string()?,
                kind: r.u32()?,
                parent: r.external_ref()?,
                template: r.external_ref()?,
                value: r.u32()?,
                visible: r.bool()?,
                enabled: r.bool()?,
                id: r.i32()?,
                order: r.i32()?,
            })
        })
    })?;
    Ok(value)
}

/// A template's toolbar component (`ToolbarDescription`, serializer `FUN_140132f00`): the icon,
/// name and tooltip shown for build and utility entries, and its placement messages.
#[derive(Debug, Clone)]
pub struct ToolbarDescription {
    pub sprite_sheet: Option<String>,
    pub sprite: String,
    pub string_table: Option<String>,
    pub name: String,
    pub name_lower: String,
    pub tooltip: String,
    /// Message keys (`CreateOk`, `CreateBlocked`, …).
    pub messages: Vec<String>,
    /// `+0x64`.
    pub category: i32,
    /// `+0x60`.
    pub value: i32,
}

pub fn toolbar_description(data: &[u8]) -> Result<ToolbarDescription> {
    let (_, value) = crs::parse(data, |r: &mut Reader, h| {
        if h.type_id != type_id("ToolbarDescription") {
            return format_err(format!(
                "expected ToolbarDescription, found {:#010x}",
                h.type_id
            ));
        }
        r.block(|r, _| {
            Ok(ToolbarDescription {
                sprite_sheet: r.external_ref()?,
                sprite: r.string()?,
                string_table: r.external_ref()?,
                name: r.string()?,
                name_lower: r.string()?,
                tooltip: r.string()?,
                messages: (0..r.count()?).map(|_| r.string()).collect::<Result<_>>()?,
                category: r.i32()?,
                value: r.i32()?,
            })
        })
    })?;
    Ok(value)
}

/// One entry of a `UIDescription` controller table (`FUN_140133530`).
#[derive(Debug, Clone)]
pub struct UiController {
    /// CRC32 type id of the `UI::DialogController` to attach (0 = none, e.g. `StatusBarUI`
    /// `0x509d1aea`).
    pub type_id: u32,
    /// The element to instantiate into the named slot.
    pub element: Option<String>,
    /// Controller configuration (`+0x10`).
    pub config: Option<String>,
    /// Tab label element (`+0x18`), used when the slot is a `TabElement`.
    pub tab: Option<String>,
    /// Name of the slot element in the dialog (`userGroup0`, `userTitle0`).
    pub slot: String,
}

/// `UIDescription` (serializer `FUN_140133350`): a utility window built from a host dialog with
/// content placed into its named slots, each with an optional controller.
#[derive(Debug, Clone)]
pub struct UiDescription {
    /// Host dialog, e.g. `Dialog\StandardDialog.rsc:resource`.
    pub dialog: Option<String>,
    /// `+0x40`.
    pub flags: i32,
    pub controllers: Vec<UiController>,
    /// `+0x20`.
    pub value: i32,
}

pub fn ui_description(data: &[u8]) -> Result<UiDescription> {
    let (_, value) = crs::parse(data, |r: &mut Reader, h| {
        if h.type_id != type_id("UIDescription") {
            return format_err(format!("expected UIDescription, found {:#010x}", h.type_id));
        }
        r.block(|r, _| {
            let dialog = r.external_ref()?;
            let flags = r.i32()?;
            let mut controllers = Vec::new();
            for _ in 0..r.count()? {
                controllers.push(UiController {
                    type_id: r.u32()?,
                    element: r.external_ref()?,
                    config: r.external_ref()?,
                    tab: r.external_ref()?,
                    slot: r.string()?,
                });
            }
            Ok(UiDescription {
                dialog,
                flags,
                controllers,
                value: r.i32()?,
            })
        })
    })?;
    Ok(value)
}

/// An entity template (`ComponentDescription`, serializer `FUN_14011dfc0`): the scheduling bucket
/// and the ordered component descriptions (`model`, `naturalresource`, `growth`, …).
#[derive(Debug, Clone)]
pub struct ComponentDescription {
    /// `+0x20`: manager bucket (default 6 = unscheduled).
    pub bucket: u32,
    /// `+0x28`: description references.
    pub descriptions: Vec<Option<String>>,
}

pub fn component_description(data: &[u8]) -> Result<ComponentDescription> {
    let (_, value) = crs::parse(data, |r: &mut Reader, h| {
        if h.type_id != type_id("ComponentDescription") {
            return format_err(format!(
                "expected ComponentDescription, found {:#010x}",
                h.type_id
            ));
        }
        r.block(|r, _| {
            let bucket = r.u32()?;
            let descriptions = (0..r.count()?)
                .map(|_| r.external_ref())
                .collect::<Result<_>>()?;
            Ok(ComponentDescription {
                bucket,
                descriptions,
            })
        })
    })?;
    Ok(value)
}

/// One model variant: an animation set and its alternative meshes.
#[derive(Debug, Clone)]
pub struct ModelVariant {
    pub animations: Option<String>,
    pub meshes: Vec<Option<String>>,
}

/// `ModelDescription` (serializer `FUN_14012a290`), read by the model component's setup
/// `FUN_140129f90`.
#[derive(Debug, Clone)]
pub struct ModelDescription {
    /// `+0x20` / `+0x28`, 0x18 bytes each.
    pub variants: Vec<ModelVariant>,
    /// `+0x30`: variant used unless `random_variant`.
    pub variant: i32,
    /// `+0x34`: mesh index within the variant (negative = no mesh).
    pub mesh: i32,
    /// `+0x38`: pick the variant with `Rand(variant count)`.
    pub random_variant: bool,
}

pub fn model_description(data: &[u8]) -> Result<ModelDescription> {
    let (_, value) = crs::parse(data, |r: &mut Reader, h| {
        if h.type_id != type_id("ModelDescription") {
            return format_err(format!(
                "expected ModelDescription, found {:#010x}",
                h.type_id
            ));
        }
        r.block(|r, _| {
            let mut variants = Vec::new();
            for _ in 0..r.count()? {
                let animations = r.external_ref()?;
                let meshes = (0..r.count()?)
                    .map(|_| r.external_ref())
                    .collect::<Result<_>>()?;
                variants.push(ModelVariant { animations, meshes });
            }
            Ok(ModelDescription {
                variants,
                variant: r.i32()?,
                mesh: r.i32()?,
                random_variant: r.bool()?,
            })
        })
    })?;
    Ok(value)
}

/// `NaturalResourceDescription` (serializer `FUN_14012b2e0`). Field offsets are in the runtime
/// object; see docs/RESOURCE-SEEDING.md and GAME-LOOP-STEP-14.md.
#[derive(Debug, Clone)]
pub struct NaturalResourceDescription {
    /// `+0x20`: the raw material it yields.
    pub raw_material: Option<String>,
    /// `+0x28`, `+0x2c`: placement noise persistence and octaves.
    pub noise_persistence: f32,
    pub noise_octaves: f32,
    /// `+0x30`: placement noise scale (sampled at `cell / scale`).
    pub noise_scale: f32,
    /// `+0x34`: minimum `noise · 0.5 + 0.5`.
    pub noise_min: f32,
    /// `+0x38`: `RandFloat()` must be at most this.
    pub chance: f32,
    /// `+0x48`: choose the model variant by noise clusters.
    pub cluster: bool,
    /// `+0x4c`, `+0x50`: cluster noise persistence and octaves.
    pub cluster_persistence: f32,
    pub cluster_octaves: f32,
    /// `+0x54`, `+0x58`: cluster noise scale and threshold.
    pub cluster_scale: f32,
    pub cluster_threshold: f32,
    /// `+0x5c`: 1-in-N chance of a random variant instead.
    pub random_variant_one_in: i32,
    /// `+0x3c`, `+0x40`: ground height range.
    pub height_min: f32,
    pub height_max: f32,
    /// `+0x44`: minimum ground normal z.
    pub normal_min: f32,
    /// `+0x64`: transition flag (GAME-LOOP-STEP-14).
    pub flag: bool,
    /// `+0x68`, `+0x70`: `System::Time` values (microseconds).
    pub time_a: i64,
    pub time_b: i64,
    /// `+0x80`.
    pub value: u32,
    /// `+0x78`.
    pub related: Option<String>,
    /// `+0x60`: used by the positioning component.
    pub position_scale: f32,
}

pub fn natural_resource_description(data: &[u8]) -> Result<NaturalResourceDescription> {
    let (_, value) = crs::parse(data, |r: &mut Reader, h| {
        if h.type_id != type_id("NaturalResourceDescription") {
            return format_err(format!(
                "expected NaturalResourceDescription, found {:#010x}",
                h.type_id
            ));
        }
        r.block(|r, _| {
            Ok(NaturalResourceDescription {
                raw_material: r.external_ref()?,
                noise_persistence: r.f32()?,
                noise_octaves: r.f32()?,
                noise_scale: r.f32()?,
                noise_min: r.f32()?,
                chance: r.f32()?,
                cluster: r.bool()?,
                cluster_persistence: r.f32()?,
                cluster_octaves: r.f32()?,
                cluster_scale: r.f32()?,
                cluster_threshold: r.f32()?,
                random_variant_one_in: r.i32()?,
                height_min: r.f32()?,
                height_max: r.f32()?,
                normal_min: r.f32()?,
                flag: r.bool()?,
                time_a: r.i64()?,
                time_b: r.i64()?,
                value: r.u32()?,
                related: r.external_ref()?,
                position_scale: r.f32()?,
            })
        })
    })?;
    Ok(value)
}

/// `GrowthDescription` (serializer `FUN_140125e50`). See GAME-LOOP-STEP-11/12 and
/// RESOURCE-SEEDING.md for the consumers.
#[derive(Debug, Clone)]
pub struct GrowthDescription {
    /// `+0x20` / `+0x24`: maximum growth `= RandFloatTol · range + base` when base > 0
    /// (`FUN_140125bc0`); otherwise −1. Also the spawn step (`FUN_140056620`).
    pub max_base: f32,
    pub max_range: f32,
    pub f28: f32,
    pub b2c: bool,
    pub f30: f32,
    pub f34: f32,
    pub b38: bool,
    pub f3c: f32,
    pub f40: f32,
    pub b44: bool,
    /// `+0x4c`: spawn radius in cells (`RandTol`).
    pub spawn_radius: i32,
    /// `+0x50`: growth timer scale (`RandFloat · value · 300` seconds).
    pub timer_months: i32,
    /// `+0x48`: spawn chance is `1 / value`.
    pub spawn_one_in: i32,
    /// `+0x54`.
    pub threshold: f32,
    /// `+0x58`: maximum occupied neighbours (`FUN_140126100`).
    pub max_neighbours: i32,
    /// `+0x5c`: maximum neighbours of the same growth type.
    pub max_same: i32,
    pub f60: f32,
    /// `+0x64`: scale shape: false = uniform growth, true = `(g/0.15, g/0.15, g, 1)` clamped.
    pub tall: bool,
}

pub fn growth_description(data: &[u8]) -> Result<GrowthDescription> {
    let (_, value) = crs::parse(data, |r: &mut Reader, h| {
        if h.type_id != type_id("GrowthDescription") {
            return format_err(format!(
                "expected GrowthDescription, found {:#010x}",
                h.type_id
            ));
        }
        r.block(|r, _| {
            Ok(GrowthDescription {
                max_base: r.f32()?,
                max_range: r.f32()?,
                f28: r.f32()?,
                b2c: r.bool()?,
                f30: r.f32()?,
                f34: r.f32()?,
                b38: r.bool()?,
                f3c: r.f32()?,
                f40: r.f32()?,
                b44: r.bool()?,
                spawn_radius: r.i32()?,
                timer_months: r.i32()?,
                spawn_one_in: r.i32()?,
                threshold: r.f32()?,
                max_neighbours: r.i32()?,
                max_same: r.i32()?,
                f60: r.f32()?,
                tall: r.bool()?,
            })
        })
    })?;
    Ok(value)
}

/// One `SpawnDescription` entry: a template spawned while the parent's growth is in `[lo, hi]`.
#[derive(Debug, Clone)]
pub struct SpawnEntry {
    pub template: Option<String>,
    pub lo: f32,
    pub hi: f32,
}

/// `SpawnDescription` (serializer `FUN_140056a10`).
pub fn spawn_description(data: &[u8]) -> Result<Vec<SpawnEntry>> {
    let (_, value) = crs::parse(data, |r: &mut Reader, h| {
        if h.type_id != type_id("SpawnDescription") {
            return format_err(format!(
                "expected SpawnDescription, found {:#010x}",
                h.type_id
            ));
        }
        r.block(|r, _| {
            (0..r.count()?)
                .map(|_| {
                    Ok(SpawnEntry {
                        template: r.external_ref()?,
                        lo: r.f32()?,
                        hi: r.f32()?,
                    })
                })
                .collect()
        })
    })?;
    Ok(value)
}

/// `MapDescription` (serializer `FUN_140129a30`): how an entity stamps the gameplay map.
#[derive(Debug, Clone)]
pub struct MapDescription {
    /// `+0x20`: cell type written under the footprint (4 for natural resources).
    pub cell_type: u32,
    /// `+0x28`: footprint rows (`#` own type, `_` ground, `.` untouched); empty = whole area.
    pub footprint: String,
    /// `+0x30`: stamp on activation.
    pub stamp: bool,
    /// `+0x31`: never stamp.
    pub no_stamp: bool,
    /// `+0x32`: draw on the minimap with `color`.
    pub minimap: bool,
    pub color: u32,
}

pub fn map_description(data: &[u8]) -> Result<MapDescription> {
    let (_, value) = crs::parse(data, |r: &mut Reader, h| {
        if h.type_id != type_id("MapDescription") {
            return format_err(format!(
                "expected MapDescription, found {:#010x}",
                h.type_id
            ));
        }
        r.block(|r, _| {
            Ok(MapDescription {
                cell_type: r.u32()?,
                footprint: r.string()?,
                stamp: r.bool()?,
                no_stamp: r.bool()?,
                minimap: r.bool()?,
                color: r.u32()?,
            })
        })
    })?;
    Ok(value)
}

/// `CreatePlacedDescription` (serializer `FUN_14011eda0`): how a building is placed; the start
/// structures (`FUN_1400a7490`) take their size from it.
#[derive(Debug, Clone)]
pub struct CreatePlacedDescription {
    /// `+0x20`.
    pub flags: u32,
    /// `+0x30`.
    pub values: Vec<u32>,
    /// `+0x28`.
    pub name: String,
    /// `+0x44`, `+0x48`: footprint width and height in cells.
    pub width: i32,
    pub height: i32,
    /// `+0x40`.
    pub value: i32,
    /// `+0x50`.
    pub related: Option<String>,
}

pub fn create_placed_description(data: &[u8]) -> Result<CreatePlacedDescription> {
    let (_, value) = crs::parse(data, |r: &mut Reader, h| {
        if h.type_id != type_id("CreatePlacedDescription") {
            return format_err(format!(
                "expected CreatePlacedDescription, found {:#010x}",
                h.type_id
            ));
        }
        r.block(|r, _| {
            let flags = r.u32()?;
            let values = (0..r.count()?).map(|_| r.u32()).collect::<Result<_>>()?;
            Ok(CreatePlacedDescription {
                flags,
                values,
                name: r.string()?,
                width: r.i32()?,
                height: r.i32()?,
                value: r.i32()?,
                related: r.external_ref()?,
            })
        })
    })?;
    Ok(value)
}

/// `RawMaterialDescription` (serializer `FUN_14012e9f0`): a storable resource.
#[derive(Debug, Clone)]
pub struct RawMaterialDescription {
    pub sprite_sheet: Option<String>,
    pub string_table: Option<String>,
    /// `+0x30`, `+0x38`: name keys (`Log`, `LogSmall`).
    pub name: String,
    pub name_small: String,
    /// `+0x40`, `+0x44`.
    pub value_40: i32,
    pub value_44: i32,
    /// `+0x48`: storage units per item (`FUN_140132250`).
    pub size: i32,
    /// `+0x4c`.
    pub value_4c: i32,
    /// `+0x5c`: category flags (`FUN_14008f3a0` maps them to the stock table).
    pub flags: u32,
    /// `+0x50`.
    pub value_50: f32,
    /// `+0x54`.
    pub value_54: i32,
    /// `+0x58`.
    pub flag_58: bool,
}

pub fn raw_material_description(data: &[u8]) -> Result<RawMaterialDescription> {
    let (_, value) = crs::parse(data, |r: &mut Reader, h| {
        if h.type_id != type_id("RawMaterialDescription") {
            return format_err(format!(
                "expected RawMaterialDescription, found {:#010x}",
                h.type_id
            ));
        }
        r.block(|r, _| {
            Ok(RawMaterialDescription {
                sprite_sheet: r.external_ref()?,
                string_table: r.external_ref()?,
                name: r.string()?,
                name_small: r.string()?,
                value_40: r.i32()?,
                value_44: r.i32()?,
                size: r.i32()?,
                value_4c: r.i32()?,
                flags: r.u32()?,
                value_50: r.f32()?,
                value_54: r.i32()?,
                flag_58: r.bool()?,
            })
        })
    })?;
    Ok(value)
}

/// `StorageDescription` (serializer `FUN_140131e70`).
#[derive(Debug, Clone)]
pub struct StorageDescription {
    /// `+0x28`: raw-material flags this storage accepts.
    pub accepts: u32,
    pub flag_20: bool,
    pub flag_21: bool,
    /// `+0x24`: capacity in storage units.
    pub capacity: i32,
    /// `+0x30`: `(count, raw material)` entries.
    pub items: Vec<(i32, Option<String>)>,
}

pub fn storage_description(data: &[u8]) -> Result<StorageDescription> {
    let (_, value) = crs::parse(data, |r: &mut Reader, h| {
        if h.type_id != type_id("StorageDescription") {
            return format_err(format!(
                "expected StorageDescription, found {:#010x}",
                h.type_id
            ));
        }
        r.block(|r, _| {
            let accepts = r.u32()?;
            let flag_20 = r.bool()?;
            let flag_21 = r.bool()?;
            let capacity = r.i32()?;
            let items = (0..r.count()?)
                .map(|_| Ok((r.i32()?, r.external_ref()?)))
                .collect::<Result<_>>()?;
            Ok(StorageDescription {
                accepts,
                flag_20,
                flag_21,
                capacity,
                items,
            })
        })
    })?;
    Ok(value)
}

/// StorageLocationDescription `+0x20` mask; serializer FUN_140132940 (version 2).
pub fn storage_location_description(data: &[u8]) -> Result<u32> {
    let (_, value) = crs::parse(data, |r: &mut Reader, h| {
        if h.type_id != type_id("StorageLocationDescription") {
            return format_err("expected StorageLocationDescription");
        }
        r.block(|r, _| r.u32())
    })?;
    Ok(value)
}

/// RadiusDescription (`FUN_140055e90`): radius `+0x28`, display reference `+0x20`.
pub fn radius_description(data: &[u8]) -> Result<(i32, Option<String>)> {
    let (_, value) = crs::parse(data, |r: &mut Reader, h| {
        if h.type_id != type_id("RadiusDescription") {
            return format_err("expected RadiusDescription");
        }
        r.block(|r, _| Ok((r.i32()?, r.external_ref()?)))
    })?;
    Ok(value)
}

/// `CitizenDescription` (serializer `FUN_140119dd0`). The class has ~60 scalar fields; they are
/// kept keyed by their runtime offset (as the original code reads them) with named getters for
/// the ones whose consumers are traced.
#[derive(Debug, Clone)]
pub struct CitizenDescription {
    /// Scalar fields by runtime offset.
    pub fields: std::collections::BTreeMap<u32, Scalar>,
    /// `+0x138`: external references (`FUN_14001c620`).
    pub list: Vec<Option<String>>,
    /// `+0x20`, `+0x28`, `+0x30`.
    pub refs: [Option<String>; 3],
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Scalar {
    F32(f32),
    I32(i32),
}

impl CitizenDescription {
    pub fn f(&self, offset: u32) -> f32 {
        match self.fields.get(&offset) {
            Some(Scalar::F32(v)) => *v,
            Some(Scalar::I32(v)) => *v as f32,
            None => 0.0,
        }
    }
    pub fn i(&self, offset: u32) -> i32 {
        match self.fields.get(&offset) {
            Some(Scalar::I32(v)) => *v,
            Some(Scalar::F32(v)) => *v as i32,
            None => 0,
        }
    }
    /// `+0x44`: age at which a citizen counts as an adult.
    pub fn adult_age(&self) -> f32 {
        self.f(0x44)
    }
    /// `+0x50`, `+0x54`: starting adults' age range (`FUN_1400a62a0` uses `+0x54 / 2`).
    pub fn start_age(&self) -> (f32, f32) {
        (self.f(0x50), self.f(0x54))
    }
    /// `+0x38`, `+0x3c`: lifespan base and random range (`FUN_14011c550`).
    pub fn lifespan(&self) -> (f32, f32) {
        (self.f(0x38), self.f(0x3c))
    }
}

pub fn citizen_description(data: &[u8]) -> Result<CitizenDescription> {
    let (_, value) = crs::parse(data, |r: &mut Reader, h| {
        if h.type_id != type_id("CitizenDescription") {
            return format_err(format!(
                "expected CitizenDescription, found {:#010x}",
                h.type_id
            ));
        }
        r.block(|r, _| {
            use Scalar::*;
            let mut fields = std::collections::BTreeMap::new();
            let mut f = |r: &mut Reader, o: u32| -> Result<()> {
                fields.insert(o, F32(r.f32()?));
                Ok(())
            };
            for o in [
                0x44, 0x6c, 0x70, 0x74, 0x78, 0x7c, 0x80, 0x84, 0x88, 0x8c, 0x90, 0x38, 0x3c, 0x40,
                0xc0, 0xd8, 0xdc, 0xe0, 0xe4, 0x48, 0x4c, 0x50, 0x54, 0x58, 0x5c, 0x60, 0xc4, 0xd4,
                0xc8, 0xcc, 0xd0, 0xe8, 0xec, 0xf0, 0xf4,
            ] {
                f(r, o)?;
            }
            fields.insert(0x110, I32(r.i32()?));
            let list = (0..r.count()?)
                .map(|_| r.external_ref())
                .collect::<Result<_>>()?;
            fields.insert(0x114, F32(r.f32()?));
            fields.insert(0x118, I32(r.i32()?));
            for o in [0x120, 0x11c, 0x128, 0x124, 0x130, 0xa4] {
                fields.insert(o, F32(r.f32()?));
            }
            fields.insert(0xa8, I32(r.i32()?));
            fields.insert(0xac, I32(r.i32()?));
            fields.insert(0xa0, F32(r.f32()?));
            fields.insert(0xb0, I32(r.i32()?));
            fields.insert(0xb4, F32(r.f32()?));
            fields.insert(0xb8, F32(r.f32()?));
            fields.insert(0xbc, I32(r.i32()?));
            let refs = [r.external_ref()?, r.external_ref()?, r.external_ref()?];
            fields.insert(0x64, F32(r.f32()?));
            fields.insert(0x68, F32(r.f32()?));
            for o in [0xf8, 0xfc, 0x100, 0x104, 0x108] {
                fields.insert(o, I32(r.i32()?));
            }
            fields.insert(0x10c, F32(r.f32()?));
            Ok(CitizenDescription { fields, list, refs })
        })
    })?;
    Ok(value)
}

/// `StatusIconDescription` (serializer `FUN_140131650`, constructor `FUN_140039280`): the icon
/// sprite sheet (`+0x20`), icon size (`+0x28`) and screen-constant flag (`+0x30`). The height
/// offset `+0x2c` is not serialized and stays 1.0.
#[derive(Debug, Clone)]
pub struct StatusIconDescription {
    pub sheet: Option<String>,
    pub size: f32,
    pub screen_constant: bool,
}

pub fn status_icon_description(data: &[u8]) -> Result<StatusIconDescription> {
    let (_, value) = crs::parse(data, |r: &mut Reader, h| {
        if h.type_id != type_id("StatusIconDescription") {
            return format_err(format!(
                "expected StatusIconDescription, found {:#010x}",
                h.type_id
            ));
        }
        r.block(|r, _| {
            Ok(StatusIconDescription {
                sheet: r.external_ref()?,
                size: r.f32()?,
                screen_constant: r.bool()?,
            })
        })
    })?;
    Ok(value)
}

/// `WorkPlaceDescription` (serializer `FUN_1401348f0`): the profession its workers take
/// (`+0x20`; none for storage barns and houses) and a second reference (`+0x28`).
#[derive(Debug, Clone, Default)]
pub struct WorkPlaceDescription {
    pub profession: Option<String>,
    pub reference_28: Option<String>,
}

pub fn workplace_description(data: &[u8]) -> Result<WorkPlaceDescription> {
    let (_, value) = crs::parse(data, |r: &mut Reader, h| {
        if h.type_id != type_id("WorkPlaceDescription") {
            return format_err(format!(
                "expected WorkPlaceDescription, found {:#010x}",
                h.type_id
            ));
        }
        r.block(|r, _| {
            Ok(WorkPlaceDescription {
                profession: r.external_ref()?,
                reference_28: r.external_ref()?,
            })
        })
    })?;
    Ok(value)
}

/// A named point of a `Pathfinding::PointList` (position in model space, yaw in degrees).
#[derive(Debug, Clone, PartialEq)]
pub struct NamedPoint {
    pub name: String,
    pub position: [f32; 3],
    pub yaw: f32,
}

/// `Pathfinding::PointList` (Runtime serializer `0x180111f70`): mesh name, a second string,
/// path points (`+0x40`) and named points (`+0x30`).
#[derive(Debug, Clone)]
pub struct PointList {
    pub mesh: String,
    pub source: String,
    pub path_points: Vec<NamedPoint>,
    pub points: Vec<NamedPoint>,
}

pub fn point_list(data: &[u8]) -> Result<PointList> {
    let (_, value) = crs::parse(data, |r: &mut Reader, h| {
        if h.type_id != type_id("PointList") {
            return format_err(format!("expected PointList, found {:#010x}", h.type_id));
        }
        r.block(|r, _| {
            let mesh = r.string()?;
            let source = r.string()?;
            let list = |r: &mut Reader| -> Result<Vec<NamedPoint>> {
                (0..r.count()?)
                    .map(|_| {
                        Ok(NamedPoint {
                            name: r.string()?,
                            position: r.f32s::<3>()?,
                            yaw: r.f32()?,
                        })
                    })
                    .collect()
            };
            let path_points = list(r)?;
            let points = list(r)?;
            Ok(PointList {
                mesh,
                source,
                path_points,
                points,
            })
        })
    })?;
    Ok(value)
}

/// `InteractDescription` (serializer `FUN_1401289d0`): the point list whose named points
/// "use", "build" and "create" become the three interaction categories (`+0x28/+0x38/+0x48`).
#[derive(Debug, Clone)]
pub struct InteractDescription {
    pub points: Option<String>,
}

pub fn interact_description(data: &[u8]) -> Result<InteractDescription> {
    let (_, value) = crs::parse(data, |r: &mut Reader, h| {
        if h.type_id != type_id("InteractDescription") {
            return format_err(format!(
                "expected InteractDescription, found {:#010x}",
                h.type_id
            ));
        }
        r.block(|r, _| {
            Ok(InteractDescription {
                points: r.external_ref()?,
            })
        })
    })?;
    Ok(value)
}

/// Interaction points of one category (0 use, 1 build, 2 create) in point-list order.
pub fn interaction_points(list: &PointList, category: usize) -> Vec<&NamedPoint> {
    let name = ["use", "build", "create"][category];
    list.points.iter().filter(|p| p.name == name).collect()
}

/// `CharacterDescription` (serializer `FUN_14010f2a0`, defaults from constructor `FUN_14010f3e0`):
/// how a walking entity moves (CharacterComponent update `FUN_14010e990`, MoveTo `FUN_1400cda40`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CharacterDescription {
    /// `+0x20` (default 0.25).
    pub radius: f32,
    /// `+0x24`: added to the heading when the model is turned (default 0).
    pub yaw_offset: f32,
    /// `+0x28`: fraction of the remaining turn kept each update (default 0.85).
    pub turn_keep: f32,
    /// `+0x2c`: animation blend time passed to `PlayAnimation` (default 0.1).
    pub blend: f32,
    /// `+0x30`: speed scale while following another character (default 1).
    pub follow_speed: f32,
    /// `+0x34`: speed scale on cell types 1 and 9 (roads; default 1).
    pub road_speed: f32,
    /// `+0x38`: speed scale on cell type 0 (default 1).
    pub type0_speed: f32,
    /// `+0x3c` / `+0x40`: hold a fixed height instead of following the ground.
    pub fixed_height: bool,
    pub height: f32,
    /// `+0x44`: lowest height the ground clamp allows (default -10).
    pub min_height: f32,
}

impl Default for CharacterDescription {
    fn default() -> Self {
        CharacterDescription {
            radius: 0.25,
            yaw_offset: 0.0,
            turn_keep: 0.85,
            blend: 0.1,
            follow_speed: 1.0,
            road_speed: 1.0,
            type0_speed: 1.0,
            fixed_height: false,
            height: 0.0,
            min_height: -10.0,
        }
    }
}

pub fn character_description(data: &[u8]) -> Result<CharacterDescription> {
    let (_, value) = crs::parse(data, |r: &mut Reader, h| {
        if h.type_id != type_id("CharacterDescription") {
            return format_err(format!(
                "expected CharacterDescription, found {:#010x}",
                h.type_id
            ));
        }
        r.block(|r, _| {
            Ok(CharacterDescription {
                radius: r.f32()?,
                yaw_offset: r.f32()?,
                turn_keep: r.f32()?,
                blend: r.f32()?,
                follow_speed: r.f32()?,
                road_speed: r.f32()?,
                type0_speed: r.f32()?,
                fixed_height: r.bool()?,
                height: r.f32()?,
                min_height: r.f32()?,
            })
        })
    })?;
    Ok(value)
}

/// `BuildDescription` (serializer `FUN_14010e610`): what constructing a building costs.
#[derive(Debug, Clone)]
pub struct BuildDescription {
    /// `+0x20`: `(count, raw material)` requirements.
    pub materials: Vec<(i32, Option<String>)>,
    /// `+0x30`: construction work.
    pub work: i32,
    /// `+0x34`: shows `labelBuildMulti` in the CreateBar (costs per area).
    pub multi: bool,
}

pub fn build_description(data: &[u8]) -> Result<BuildDescription> {
    let (_, value) = crs::parse(data, |r: &mut Reader, h| {
        if h.type_id != type_id("BuildDescription") {
            return format_err(format!(
                "expected BuildDescription, found {:#010x}",
                h.type_id
            ));
        }
        r.block(|r, _| {
            let materials = (0..r.count()?)
                .map(|_| Ok((r.i32()?, r.external_ref()?)))
                .collect::<Result<_>>()?;
            Ok(BuildDescription {
                materials,
                work: r.i32()?,
                multi: r.bool()?,
            })
        })
    })?;
    Ok(value)
}

/// `WorkDescription` (`Template\X.rsc:work`, serializer `FUN_140133d80`).
#[derive(Debug, Clone, Default)]
pub struct WorkDescription {
    /// `+0x24`: most workers (`FUN_140133a90`).
    pub max_workers: i32,
    pub value_28: i32,
    pub value_2c: i32,
    /// `+0x34`: scale the most workers by the footprint area over `+0x30` cells.
    pub per_area: bool,
    pub area: i32,
    pub flag_20: bool,
}

pub fn work_description(data: &[u8]) -> Result<WorkDescription> {
    let (_, value) = crs::parse(data, |r: &mut Reader, h| {
        if h.type_id != type_id("WorkDescription") {
            return format_err(format!(
                "expected WorkDescription, found {:#010x}",
                h.type_id
            ));
        }
        r.block(|r, _| {
            Ok(WorkDescription {
                max_workers: r.i32()?,
                value_28: r.i32()?,
                value_2c: r.i32()?,
                per_area: r.bool()?,
                area: r.i32()?,
                flag_20: r.bool()?,
            })
        })
    })?;
    Ok(value)
}

/// `ClearAreaDescription` (`Template\X.rsc:cleararea`, serializer `FUN_14011d5c0`).
#[derive(Debug, Clone, Default)]
pub struct ClearAreaDescription {
    /// `+0x20`: an external reference handed to the ClearArea work (`FUN_1400ebe30`).
    pub reference: Option<String>,
    /// `+0x28..+0x2a`: the ClearArea work's flags (natural resources, empty cells, piles).
    pub clear_resources: bool,
    pub clear_cells: bool,
    pub clear_piles: bool,
    /// `+0x2b`: register the work keeping its priority.
    pub keep_priority: bool,
    /// `+0x2c`: stamp the footprint once cleared (`FUN_14011d740`).
    pub stamp: bool,
    /// `+0x2d`: remove everything in the footprint at once (`FUN_14011d230`).
    pub instant: bool,
    pub value_30: i32,
}

pub fn clear_area_description(data: &[u8]) -> Result<ClearAreaDescription> {
    let (_, value) = crs::parse(data, |r: &mut Reader, h| {
        if h.type_id != type_id("ClearAreaDescription") {
            return format_err(format!(
                "expected ClearAreaDescription, found {:#010x}",
                h.type_id
            ));
        }
        r.block(|r, _| {
            Ok(ClearAreaDescription {
                reference: r.external_ref()?,
                clear_resources: r.bool()?,
                clear_cells: r.bool()?,
                clear_piles: r.bool()?,
                keep_priority: r.bool()?,
                stamp: r.bool()?,
                instant: r.bool()?,
                value_30: r.i32()?,
            })
        })
    })?;
    Ok(value)
}

/// `CreateRemovalDescription` (`Template\ClearX.rsc:createremoval`, serializer
/// `FUN_14011ef90`): what a RemovalTool drag does to each cell (`FUN_140108ab0`).
#[derive(Debug, Clone, Default)]
pub struct CreateRemovalDescription {
    /// `+0x20`: mark natural resources for removal.
    pub resources: bool,
    /// `+0x28/+0x30`: the resource templates it marks (empty: every one).
    pub filters: Vec<String>,
    /// `+0x38`: the marker entity's template (`Template\Clear.rsc:resource`).
    pub template: Option<String>,
    /// `+0x40` roads, `+0x41` demolish buildings, `+0x42` cancel, `+0x43` prioritise.
    pub roads: bool,
    pub buildings: bool,
    pub cancel: bool,
    pub priority: bool,
}

pub fn create_removal_description(data: &[u8]) -> Result<CreateRemovalDescription> {
    let (_, value) = crs::parse(data, |r: &mut Reader, h| {
        if h.type_id != type_id("CreateRemovalDescription") {
            return format_err(format!(
                "expected CreateRemovalDescription, found {:#010x}",
                h.type_id
            ));
        }
        r.block(|r, _| {
            let resources = r.bool()?;
            let n = r.count()?;
            let filters = (0..n)
                .map(|_| Ok(r.external_ref()?.unwrap_or_default()))
                .collect::<Result<_>>()?;
            Ok(CreateRemovalDescription {
                resources,
                filters,
                template: r.external_ref()?,
                roads: r.bool()?,
                buildings: r.bool()?,
                cancel: r.bool()?,
                priority: r.bool()?,
            })
        })
    })?;
    Ok(value)
}

/// `CreateDragDescription` (`Template\X.rsc:createdrag`, serializer `FUN_14011ec20`).
#[derive(Debug, Clone, Default)]
pub struct CreateDragDescription {
    /// `+0x20`: admitted cell types (`FUN_1400a4aa0`'s mask).
    pub mask: u32,
    pub value_28: i32,
    pub value_2c: i32,
    pub value_30: i32,
    pub value_34: i32,
    /// `+0x24`: ignore cell occupants.
    pub override_occupant: bool,
    /// `+0x38`: an external reference.
    pub reference: Option<String>,
}

pub fn create_drag_description(data: &[u8]) -> Result<CreateDragDescription> {
    let (_, value) = crs::parse(data, |r: &mut Reader, h| {
        if h.type_id != type_id("CreateDragDescription") {
            return format_err(format!(
                "expected CreateDragDescription, found {:#010x}",
                h.type_id
            ));
        }
        r.block(|r, _| {
            Ok(CreateDragDescription {
                mask: r.u32()?,
                value_28: r.i32()?,
                value_2c: r.i32()?,
                value_30: r.i32()?,
                value_34: r.i32()?,
                override_occupant: r.bool()?,
                reference: r.external_ref()?,
            })
        })
    })?;
    Ok(value)
}

/// `Profession` (serializer `FUN_1400ae3e0`): a job citizens can be assigned to.
#[derive(Debug, Clone)]
pub struct Profession {
    /// `+0x20`, `+0x28`.
    pub string_table: Option<String>,
    pub sprite_sheet: Option<String>,
    /// `+0x30`: name key, also the icon's sprite name (`ProfessionBuilder`).
    pub name: String,
    /// `+0x38`, `+0x40`: tooltip and death-message keys.
    pub tooltip: String,
    pub death: String,
    pub value_48: i32,
    pub flag_4c: bool,
    pub flag_4d: bool,
    pub value_54: f32,
    pub value_58: f32,
    pub value_50: i32,
    pub flag_4e: bool,
}

pub fn profession(data: &[u8]) -> Result<Profession> {
    let (_, value) = crs::parse(data, |r: &mut Reader, h| {
        if h.type_id != type_id("Profession") {
            return format_err(format!("expected Profession, found {:#010x}", h.type_id));
        }
        r.block(|r, _| {
            Ok(Profession {
                string_table: r.external_ref()?,
                sprite_sheet: r.external_ref()?,
                name: r.string()?,
                tooltip: r.string()?,
                death: r.string()?,
                value_48: r.i32()?,
                flag_4c: r.bool()?,
                flag_4d: r.bool()?,
                value_54: r.f32()?,
                value_58: r.f32()?,
                value_50: r.i32()?,
                flag_4e: r.bool()?,
            })
        })
    })?;
    Ok(value)
}
