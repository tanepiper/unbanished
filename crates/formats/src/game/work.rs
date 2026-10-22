//! Workplaces and production: quarries, herbalists, foresters, fishing, hunting, fields, orchards, pastures, livestock, recipes and professions.

mod produce;
pub use produce::{consume_produce_description, ConsumeProduceDescription, ProduceRecipe};

use super::*;

// +0x20; +0x28; +0x2c
/// `QuarryDescription`: the pit the quarry digs, a picking mesh in model space whose vertices set
/// the terrain, no lower than a floor that goes from `shallow` (when built) to `deep` (at the end of its
/// production).
///
/// Original: `FUN_140055c80`.
#[derive(Debug, Clone, PartialEq)]
pub struct QuarryDescription {
    /// The floor at the end of its production.
    pub deep: f32,
    /// The floor when built.
    pub shallow: f32,
    /// The picking mesh, in model space.
    pub mesh: Option<String>,
}

/// Read a [`QuarryDescription`] from the bytes of its resource.
pub fn quarry_description(data: &[u8]) -> Result<QuarryDescription> {
    let (_, value) = crs::parse(data, |r: &mut Reader, h| {
        if h.type_id != type_id("QuarryDescription") {
            return format_err(format!(
                "expected QuarryDescription, found {:#010x}",
                h.type_id
            ));
        }
        r.block(|r, _| {
            Ok(QuarryDescription {
                deep: r.f32()?,
                shallow: r.f32()?,
                mesh: r.external_ref()?,
            })
        })
    })?;
    Ok(value)
}

// +0x20; +0x28; +0x30
/// `HerbalistDescription`: the natural-resource templates a gatherer or herbalist harvests (count)
/// and the stock-manager production group its limit is checked against (5 food, 6 herbs).
///
/// Original: `FUN_14004bc40`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HerbalistDescription {
    /// The natural-resource templates it harvests.
    pub resources: Vec<Option<String>>,
    /// The stock-manager group its limit is checked against.
    pub group: i32,
}

/// Read a [`HerbalistDescription`] from the bytes of its resource.
pub fn herbalist_description(data: &[u8]) -> Result<HerbalistDescription> {
    let (_, value) = crs::parse(data, |r: &mut Reader, h| {
        if h.type_id != type_id("HerbalistDescription") {
            return format_err(format!(
                "expected HerbalistDescription, found {:#010x}",
                h.type_id
            ));
        }
        r.block(|r, _| {
            let n = r.i32()?;
            let resources = (0..n.max(0))
                .map(|_| r.external_ref())
                .collect::<Result<Vec<_>>>()?;
            Ok(HerbalistDescription {
                resources,
                group: r.i32()?,
            })
        })
    })?;
    Ok(value)
}

// +0x20; +0x28
/// `ForesterDescription`: the tree template it plants and, from version 1, the stock-manager slot
/// of its limit (logs, 0).
///
/// Original: `FUN_140124f50`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForesterDescription {
    /// The tree template it plants.
    pub tree: Option<String>,
    /// The stock-manager slot of its limit (version 1).
    pub group: i32,
}

/// Read a [`ForesterDescription`] from the bytes of its resource.
pub fn forester_description(data: &[u8]) -> Result<ForesterDescription> {
    let (_, value) = crs::parse(data, |r: &mut Reader, h| {
        if h.type_id != type_id("ForesterDescription") {
            return format_err(format!(
                "expected ForesterDescription, found {:#010x}",
                h.type_id
            ));
        }
        r.block(|r, version| {
            let tree = r.external_ref()?;
            let group = if version > 0 { r.i32()? } else { 0 };
            Ok(ForesterDescription { tree, group })
        })
    })?;
    Ok(value)
}

// +0x20; +0x2c; +0x28
/// `FishingDescription`: the catch's raw-material template, the cell types counted as fishing water
/// and, from version 1, the stock-manager slot of its limit.
///
/// Original: `FUN_140124640`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FishingDescription {
    /// The catch's raw-material template.
    pub output: Option<String>,
    /// The cell types counted as fishing water.
    pub mask: u32,
    /// The stock-manager slot of its limit (version 1).
    pub group: i32,
}

/// Read a [`FishingDescription`] from the bytes of its resource.
pub fn fishing_description(data: &[u8]) -> Result<FishingDescription> {
    let (_, value) = crs::parse(data, |r: &mut Reader, h| {
        if h.type_id != type_id("FishingDescription") {
            return format_err(format!(
                "expected FishingDescription, found {:#010x}",
                h.type_id
            ));
        }
        r.block(|r, version| {
            let output = r.external_ref()?;
            let mask = r.u32()?;
            let group = if version > 0 { r.i32()? } else { 0 };
            Ok(FishingDescription {
                output,
                mask,
                group,
            })
        })
    })?;
    Ok(value)
}

// +0x20; +0x24
/// `HuntDescription` (versioned): the hunting time a kill needs (× 300 s of a full lodge's work)
/// and, from version 1, the stock slot of its limit.
///
/// Original: `FUN_140127e30`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HuntDescription {
    /// The hunting time a kill needs, in units of 300 seconds of a full lodge's work.
    pub time: f32,
    /// The stock slot of its limit (version 1).
    pub group: i32,
}

/// Read a [`HuntDescription`] from the bytes of its resource.
pub fn hunt_description(data: &[u8]) -> Result<HuntDescription> {
    let (_, value) = crs::parse(data, |r: &mut Reader, h| {
        if h.type_id != type_id("HuntDescription") {
            return format_err(format!(
                "expected HuntDescription, found {:#010x}",
                h.type_id
            ));
        }
        r.block(|r, version| {
            let time = r.f32()?;
            let group = if version > 0 { r.i32()? } else { 0 };
            Ok(HuntDescription { time, group })
        })
    })?;
    Ok(value)
}

// +0x24; +0x20
/// `CropFieldDescription` (versioned): a tending visit's growth and, from version 1, the stock
/// slot.
///
/// Original: `FUN_140120740`.
pub fn crop_field_description(data: &[u8]) -> Result<(f32, i32)> {
    let (_, value) = crs::parse(data, |r: &mut Reader, h| {
        if h.type_id != type_id("CropFieldDescription") {
            return format_err(format!(
                "expected CropFieldDescription, found {:#010x}",
                h.type_id
            ));
        }
        r.block(|r, version| {
            let tend = r.f32()?;
            let group = if version > 0 { r.i32()? } else { 0 };
            Ok((tend, group))
        })
    })?;
    Ok(value)
}

// +0x24; +0x28; +0x2c; +0x20
/// `OrchardDescription` (versioned): tree spacing, a tending visit's ripening and, from version 1,
/// the stock slot.
///
/// Original: `FUN_140052c60`.
pub fn orchard_description(data: &[u8]) -> Result<((i32, i32), f32, i32)> {
    let (_, value) = crs::parse(data, |r: &mut Reader, h| {
        if h.type_id != type_id("OrchardDescription") {
            return format_err(format!(
                "expected OrchardDescription, found {:#010x}",
                h.type_id
            ));
        }
        r.block(|r, version| {
            let spacing = (r.i32()?, r.i32()?);
            let tend = r.f32()?;
            let group = if version > 0 { r.i32()? } else { 0 };
            Ok((spacing, tend, group))
        })
    })?;
    Ok(value)
}

// +0x20. The default limit of 5 comes from the factory FUN_140037100.
/// A `PastureDescription`. From version 1, it has the stock slot of the limit of its products (5
/// by default).
///
/// Original: `FUN_14012d6f0`.
pub fn pasture_description(data: &[u8]) -> Result<i32> {
    let (_, value) = crs::parse(data, |r: &mut Reader, h| {
        if h.type_id != type_id("PastureDescription") {
            return format_err(format!(
                "expected PastureDescription, found {:#010x}",
                h.type_id
            ));
        }
        r.block(|r, version| if version > 0 { r.i32() } else { Ok(5) })
    })?;
    Ok(value)
}

// +0x20; +0x2c; +0x28
/// `LivestockDescription` (versioned): the raw material a pasture gathers from the animal (none for
/// cattle), every × 300 s, and from version 1 the stock slot.
///
/// Original: `FUN_140129430`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LivestockDescription {
    /// The raw material a pasture gathers from the animal (none for cattle).
    pub product: Option<String>,
    /// The time between products, in units of 300 seconds.
    pub interval: f32,
    /// The stock slot (version 1).
    pub group: i32,
}

/// Read a [`LivestockDescription`] from the bytes of its resource.
pub fn livestock_description(data: &[u8]) -> Result<LivestockDescription> {
    let (_, value) = crs::parse(data, |r: &mut Reader, h| {
        if h.type_id != type_id("LivestockDescription") {
            return format_err(format!(
                "expected LivestockDescription, found {:#010x}",
                h.type_id
            ));
        }
        r.block(|r, version| {
            let product = r.external_ref()?;
            let interval = r.f32()?;
            let group = if version > 0 { r.i32()? } else { 0 };
            Ok(LivestockDescription {
                product,
                interval,
                group,
            })
        })
    })?;
    Ok(value)
}

// +0x20; +0x28
/// `WorkPlaceDescription`: the profession its workers take (none for storage barns and houses) and
/// a second reference.
///
/// Original: `FUN_1401348f0`.
#[derive(Debug, Clone, Default)]
pub struct WorkPlaceDescription {
    /// The profession its workers take (none for storage barns and houses).
    pub profession: Option<String>,
    /// Not identified yet.
    pub reference_28: Option<String>,
}

/// Read a [`WorkPlaceDescription`] from the bytes of its resource.
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

/// `WorkDescription` (`Template\X.rsc:work`).
///
/// Original: `FUN_140133d80`.
#[derive(Debug, Clone, Default)]
pub struct WorkDescription {
    // +0x24
    /// Most workers.
    ///
    /// Original: `FUN_140133a90`.
    pub max_workers: i32,
    /// The fewest workers the player may set.
    pub value_28: i32,
    /// The most workers the player may set.
    pub value_2c: i32,
    // +0x34; +0x30
    /// Scale the most workers by the footprint's area over `area` cells.
    pub per_area: bool,
    /// The area, in cells, that `per_area` divides by.
    pub area: i32,
    /// Whether the player may change the most workers in the building's window.
    pub flag_20: bool,
}

/// Read a [`WorkDescription`] from the bytes of its resource.
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

/// `Profession`: a job citizens can be assigned to.
///
/// Original: `FUN_1400ae3e0`.
#[derive(Debug, Clone)]
pub struct Profession {
    // +0x20; +0x28
    /// The string table.
    pub string_table: Option<String>,
    /// The icon's sprite sheet.
    pub sprite_sheet: Option<String>,
    // +0x30
    /// Name key, also the icon's sprite name (`ProfessionBuilder`).
    pub name: String,
    // +0x38, +0x40
    /// The tooltip's key.
    pub tooltip: String,
    /// The death-message key of an accident at its work.
    pub death: String,
    /// The sort priority in the professions list.
    pub value_48: i32,
    /// Works anywhere (laborers and builders).
    pub flag_4c: bool,
    /// With a workplace, the citizen does not catch diseases (the doctor).
    pub flag_4d: bool,
    /// The accident odds' years.
    pub value_54: f32,
    /// The accident odds' factor.
    pub value_58: f32,
    /// How much a worker carries (the laborer's is what home stocking carries at least).
    pub value_50: i32,
    /// Whether it is in the professions list (*inferred*).
    pub flag_4e: bool,
}

/// Read a [`Profession`] from the bytes of its resource.
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

// +0x28; +0x20
/// RadiusDescription: radius, display reference.
///
/// Original: `FUN_140055e90`.
pub fn radius_description(data: &[u8]) -> Result<(i32, Option<String>)> {
    let (_, value) = crs::parse(data, |r: &mut Reader, h| {
        if h.type_id != type_id("RadiusDescription") {
            return format_err("expected RadiusDescription");
        }
        r.block(|r, _| Ok((r.i32()?, r.external_ref()?)))
    })?;
    Ok(value)
}
