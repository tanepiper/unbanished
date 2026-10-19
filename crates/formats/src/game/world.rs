//! Map generation, terrain and climate: the New Game profiles and the map description.

use super::*;

/// `TerrainGenerator`: a New Game terrain profile.
///
/// Original: `FUN_1400b6b60`.
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

/// Read a [`TerrainGenerator`] from the bytes of its resource.
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

// +0x38 + 0x20·month. Interpolated by FUN_140098f90, applied by FUN_140098da0.
/// One month of `EnvironmentDescription` lighting (32 bytes at `+ 0x20·month`).
///
/// The game interpolates between the current and the next month, then calls
/// `SetFog(fog_start, fog_end, fog_color with alpha fog_max)` and
/// `SunLight::Set(FromYawPitch(sun_yaw, sun_pitch), sun_color, ambient_color)`. Colours are
/// `0xAARRGGBB`.
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

/// `EnvironmentDescription`: weather and monthly lighting.
///
/// Original: `FUN_140099510`.
#[derive(Debug, Clone)]
pub struct EnvironmentDescription {
    pub template: Option<String>,
    /// Weather effects: (environment resource, ambient audio).
    pub weather: Vec<(Option<String>, Option<String>)>,
    pub months: Vec<EnvironmentMonth>,
}

/// Read a [`EnvironmentDescription`] from the bytes of its resource.
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

/// `ClimateGenerator`: monthly weather thresholds used by the environment loop.
///
/// Original: `FUN_140098470`.
#[derive(Debug, Clone)]
pub struct ClimateGenerator {
    pub string_table: Option<String>,
    pub name: String,
    pub enabled: bool,
    // +0x34..+0x40
    /// Four climate settings (not identified yet).
    pub settings: [f32; 4],
    // +0x44
    /// Four values for each month; the weather selector reads the first three as limits.
    pub monthly_thresholds: [[f32; 4]; 12],
    // +0x104..+0x10c
    /// Three more climate settings (not identified yet).
    pub trailing_settings: [f32; 3],
}

/// Read a [`ClimateGenerator`] from the bytes of its resource.
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

// A FUN_14001c020 record.
/// A building placed relative to the start point of a new town.
///
/// Original: `FUN_1400a7490`.
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

/// `MapGenerator`: a New Game starting-condition profile. Field names follow their confirmed
/// consumers; offsets are the in-memory record's.
///
/// Original: `FUN_1400a7e50`.
#[derive(Debug, Clone)]
pub struct MapGenerator {
    pub list: Option<String>,
    pub name: String,
    pub description: String,
    pub flag: bool,
    // +0x3c
    /// Radius of the clear-ground start area.
    ///
    /// Original: `FUN_1400a7bf0`.
    pub start_radius: i32,
    pub value_40: f32,
    pub citizen: String,
    // +0x60
    /// The household groups: a flag, and the most extra members (`(flag, extra members up to)`).
    pub households: Vec<(bool, i32)>,
    // +0x70
    /// Templates seeded over the map: rock, ore, tree.
    ///
    /// Original: `FUN_1400a6780`.
    pub natural_resources: Vec<String>,
    // +0x80
    /// Not identified yet.
    pub structures: Vec<PlacedStructure>,
    // +0x90
    /// Population managers (deer).
    pub populations: Vec<String>,
    // +0xa0
    /// Starting raw materials.
    pub raw_materials: Vec<String>,
    // +0xb0
    /// Crop templates; `crop_picks` are chosen at random.
    pub crops: Vec<String>,
    // +0xc0
    /// Orchard tree templates; `tree_picks` are chosen at random.
    pub orchard_trees: Vec<String>,
    // +0xd0
    /// Livestock templates; `livestock_picks` are chosen at random.
    pub livestock: Vec<String>,
    pub crop_picks: i32,
    pub tree_picks: i32,
    pub livestock_picks: i32,
    // +0xec..+0xf4
    /// Not identified yet.
    pub livestock_floats: [f32; 3],
    // +0xf8; +0xf8..+0x114
    /// `livestock_count` then seven more integers.
    pub ints: [i32; 8],
    pub string_118: String,
    pub string_138: String,
    pub float_130: f32,
    pub int_150: i32,
    pub string_158: String,
    pub float_170: f32,
}

/// Read a [`MapGenerator`] from the bytes of its resource.
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

/// An `ExternalList`: an ordered list of resource references, for example
/// `Game/Terrain/TerrainList.rsc`, which fills the terrain selector of the New Game dialog.
///
/// Original: the New Game lists are loaded by `FUN_14009f280`.
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

// +0x38
/// The first fields of a `ClimateGenerator` (a climate profile of the New Game dialog): the string
/// table, the name key and the "listed" flag. The climate values that follow are not decoded
/// here.
#[derive(Debug, Clone)]
pub struct ClimateHeader {
    pub string_table: Option<String>,
    pub name: String,
    pub listed: bool,
}

/// Read a [`ClimateHeader`] from the bytes of its resource.
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

/// `MapDescription`: how an entity stamps the gameplay map.
///
/// Original: `FUN_140129a30`.
#[derive(Debug, Clone)]
pub struct MapDescription {
    // +0x20
    /// Cell type written under the footprint (4 for natural resources).
    pub cell_type: u32,
    // +0x28
    /// Footprint rows (`#` own type, `_` ground, `.` untouched); empty = whole area.
    pub footprint: String,
    // +0x30
    /// Stamp on activation.
    pub stamp: bool,
    // +0x31
    /// Never stamp.
    pub no_stamp: bool,
    // +0x32
    /// Draw on the minimap with `color`.
    pub minimap: bool,
    pub color: u32,
}

/// Read a [`MapDescription`] from the bytes of its resource.
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
