//! Natural resources, growth, spawning, herds, animals and fruit.

use super::*;

/// `NaturalResourceDescription`. Field offsets are in the runtime object; see
/// docs/RESOURCE-SEEDING.md and GAME-LOOP-STEP-14.md.
///
/// Original: `FUN_14012b2e0`.
#[derive(Debug, Clone)]
pub struct NaturalResourceDescription {
    // +0x20
    /// The raw material it yields.
    pub raw_material: Option<String>,
    // +0x28, +0x2c
    /// Placement noise persistence and octaves.
    pub noise_persistence: f32,
    pub noise_octaves: f32,
    // +0x30
    /// Placement noise scale (sampled at `cell / scale`).
    pub noise_scale: f32,
    // +0x34
    /// Minimum `noise · 0.5 + 0.5`.
    pub noise_min: f32,
    // +0x38
    /// `RandFloat` must be at most this.
    pub chance: f32,
    // +0x48
    /// Choose the model variant by noise clusters.
    pub cluster: bool,
    // +0x4c, +0x50
    /// Cluster noise persistence and octaves.
    pub cluster_persistence: f32,
    pub cluster_octaves: f32,
    // +0x54, +0x58
    /// Cluster noise scale and threshold.
    pub cluster_scale: f32,
    pub cluster_threshold: f32,
    // +0x5c
    /// 1-in-N chance of a random variant instead.
    pub random_variant_one_in: i32,
    // +0x3c, +0x40
    /// Ground height range.
    pub height_min: f32,
    pub height_max: f32,
    // +0x44
    /// Minimum ground normal z.
    pub normal_min: f32,
    // +0x64
    /// Transition flag (GAME-LOOP-STEP-14).
    pub flag: bool,
    // +0x68, +0x70
    /// `System::Time` values (microseconds).
    pub time_a: i64,
    pub time_b: i64,
    // +0x80
    /// Not identified yet.
    pub value: u32,
    // +0x78
    /// Not identified yet.
    pub related: Option<String>,
    // +0x60
    /// Used by the positioning component.
    pub position_scale: f32,
}

/// Read a [`NaturalResourceDescription`] from the bytes of its resource.
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

/// `GrowthDescription`. See GAME-LOOP-STEP-11/12 and RESOURCE-SEEDING.md for the consumers.
///
/// Original: `FUN_140125e50`.
#[derive(Debug, Clone)]
pub struct GrowthDescription {
    // +0x20 / +0x24
    /// Maximum growth `= RandFloatTol · range + base` when base > 0; otherwise −1. Also the spawn
    /// step.
    ///
    /// Original: `FUN_140125bc0`, `FUN_140056620`.
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
    // +0x4c
    /// Spawn radius in cells (`RandTol`).
    pub spawn_radius: i32,
    // +0x50
    /// Growth timer scale (`RandFloat · value · 300` seconds).
    pub timer_months: i32,
    // +0x48
    /// Spawn chance is `1 / value`.
    pub spawn_one_in: i32,
    // +0x54
    /// Not identified yet.
    pub threshold: f32,
    // +0x58
    /// Maximum occupied neighbours.
    ///
    /// Original: `FUN_140126100`.
    pub max_neighbours: i32,
    // +0x5c
    /// Maximum neighbours of the same growth type.
    pub max_same: i32,
    pub f60: f32,
    // +0x64
    /// Scale shape: false = uniform growth, true = `(g/0.15, g/0.15, g, 1)` clamped.
    pub tall: bool,
}

/// Read a [`GrowthDescription`] from the bytes of its resource.
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

/// `SpawnDescription`.
///
/// Original: `FUN_140056a10`.
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

/// `HerdDescription`.
///
/// Original: `FUN_1401276b0`.
#[derive(Debug, Clone, PartialEq)]
pub struct HerdDescription {
    // +0x20
    /// The animal template.
    pub animal: Option<String>,
    // +0x28, +0x2c
    /// The most and fewest animals.
    pub max: i32,
    pub min: i32,
    // +0x30, +0x34
    /// The rest between moves (× 300 s), base and random part.
    pub rest: f32,
    pub rest_range: f32,
    // +0x38
    /// The side of the herd's area in cells.
    pub area: i32,
    // +0x3c
    /// The regrowth interval (× 300 s).
    pub regrowth: f32,
}

/// Read a [`HerdDescription`] from the bytes of its resource.
pub fn herd_description(data: &[u8]) -> Result<HerdDescription> {
    let (_, value) = crs::parse(data, |r: &mut Reader, h| {
        if h.type_id != type_id("HerdDescription") {
            return format_err(format!(
                "expected HerdDescription, found {:#010x}",
                h.type_id
            ));
        }
        r.block(|r, _| {
            Ok(HerdDescription {
                animal: r.external_ref()?,
                max: r.i32()?,
                min: r.i32()?,
                rest: r.f32()?,
                rest_range: r.f32()?,
                area: r.i32()?,
                regrowth: r.f32()?,
            })
        })
    })?;
    Ok(value)
}

// +0x40
/// `AnimalDescription` (the idle time, is not stored and keeps the constructor's 4).
///
/// Original: `FUN_14010c520`.
#[derive(Debug, Clone, PartialEq)]
pub struct AnimalDescription {
    // +0x48
    /// The raw materials a hunted animal leaves.
    pub materials: Vec<Option<String>>,
    // +0x20
    /// The side of the animal's home area in cells.
    pub area: i32,
    // +0x24
    /// Map cells a pasture gives each animal.
    ///
    /// Original: `FUN_14012da40`.
    pub space: i32,
    // +0x28, +0x2c
    /// The lifespan in years, base and tolerance.
    pub life: f32,
    pub life_range: f32,
    // +0x30
    /// The age at full size (years).
    pub adult: f32,
    // +0x34; +0x38
    /// A breeding male's years between young (and as much again at random);.
    ///
    /// Original: `FUN_14010b8f0`.
    pub breed: f32,
    pub value_38: f32,
    // +0x3c
    /// Turn to a random heading when idling.
    pub turn: bool,
    // +0x40
    /// Not identified yet.
    pub idle: f32,
}

/// Read a [`AnimalDescription`] from the bytes of its resource.
pub fn animal_description(data: &[u8]) -> Result<AnimalDescription> {
    let (_, value) = crs::parse(data, |r: &mut Reader, h| {
        if h.type_id != type_id("AnimalDescription") {
            return format_err(format!(
                "expected AnimalDescription, found {:#010x}",
                h.type_id
            ));
        }
        r.block(|r, _| {
            let n = r.i32()?;
            let materials = (0..n.max(0))
                .map(|_| r.external_ref())
                .collect::<Result<Vec<_>>>()?;
            Ok(AnimalDescription {
                materials,
                area: r.i32()?,
                space: r.i32()?,
                life: r.f32()?,
                life_range: r.f32()?,
                adult: r.f32()?,
                breed: r.f32()?,
                value_38: r.f32()?,
                turn: r.bool()?,
                idle: 4.0,
            })
        })
    })?;
    Ok(value)
}

// +0x20; +0x24; +0x28; +0x30
/// `FruitDescription`: ripening time (× 300 s), the best temperature, its range, the fruit's raw
/// material.
///
/// Original: `FUN_14004ab40`.
pub fn fruit_description(data: &[u8]) -> Result<(f32, f32, f32, Option<String>)> {
    let (_, value) = crs::parse(data, |r: &mut Reader, h| {
        if h.type_id != type_id("FruitDescription") {
            return format_err(format!(
                "expected FruitDescription, found {:#010x}",
                h.type_id
            ));
        }
        r.block(|r, _| Ok((r.f32()?, r.f32()?, r.f32()?, r.external_ref()?)))
    })?;
    Ok(value)
}
