//! Workplaces and production: quarries, herbalists, foresters, fishing, hunting, fields, orchards, pastures, livestock, recipes and professions.

use super::*;

/// `QuarryDescription` (serializer `FUN_140055c80`): the pit the quarry digs, a picking mesh
/// in model space (`+0x20`) whose vertices set the terrain, no lower than a floor that goes
/// from `+0x28` (when built) to `+0x2c` (at the end of its production).
#[derive(Debug, Clone, PartialEq)]
pub struct QuarryDescription {
    pub deep: f32,
    pub shallow: f32,
    pub mesh: Option<String>,
}

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

/// `HerbalistDescription` (serializer `FUN_14004bc40`): the natural-resource templates a
/// gatherer or herbalist harvests (`+0x20`, count `+0x28`) and the stock-manager production
/// group its limit is checked against (`+0x30`: 5 food, 6 herbs).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HerbalistDescription {
    pub resources: Vec<Option<String>>,
    pub group: i32,
}

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

/// `ForesterDescription` (serializer `FUN_140124f50`): the tree template it plants (`+0x20`)
/// and, from version 1, the stock-manager slot of its limit (`+0x28`; logs, 0).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForesterDescription {
    pub tree: Option<String>,
    pub group: i32,
}

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

/// `FishingDescription` (serializer `FUN_140124640`): the catch's raw-material template
/// (`+0x20`), the cell types counted as fishing water (`+0x2c`) and, from version 1, the
/// stock-manager slot of its limit (`+0x28`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FishingDescription {
    pub output: Option<String>,
    pub mask: u32,
    pub group: i32,
}

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

/// `HuntDescription` (serializer `FUN_140127e30`, versioned): the hunting time a kill needs
/// (`+0x20`, × 300 s of a full lodge's work) and, from version 1, the stock slot of its limit
/// (`+0x24`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HuntDescription {
    pub time: f32,
    pub group: i32,
}

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

/// `CropFieldDescription` (serializer `FUN_140120740`, versioned): a tending visit's growth
/// `+0x24` and, from version 1, the stock slot `+0x20`.
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

/// `OrchardDescription` (serializer `FUN_140052c60`, versioned): tree spacing `+0x24`,
/// `+0x28`, a tending visit's ripening `+0x2c` and, from version 1, the stock slot `+0x20`.
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

/// `PastureDescription` (serializer `FUN_14012d6f0`, versioned): from version 1 the stock slot
/// of its products' limit `+0x20` (5 by the factory `FUN_140037100`).
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

/// `LivestockDescription` (serializer `FUN_140129430`, versioned): the raw material a pasture
/// gathers from the animal `+0x20` (none for cattle), every `+0x2c` × 300 s, and from version 1
/// the stock slot `+0x28`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LivestockDescription {
    pub product: Option<String>,
    pub interval: f32,
    pub group: i32,
}

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

/// One production alternative of a [`ConsumeProduceDescription`] (`0x28`-byte record read by
/// `FUN_140033050`).
#[derive(Debug, Clone, Default)]
pub struct ProduceRecipe {
    /// `+0x00/+0x08`: input raw-material templates and counts.
    pub inputs: Vec<(Option<String>, i32)>,
    /// `+0x10`: the output raw-material template.
    pub output: Option<String>,
    /// `+0x18`: stock group (manager `+0x170` slot of the limit check).
    pub group: i32,
    /// `+0x20`: the UI label key.
    pub label: String,
}

/// `ConsumeProduceDescription` (`Template\X.rsc:consumeproduce`, serializer `FUN_14011e5b0`).
#[derive(Debug, Clone, Default)]
pub struct ConsumeProduceDescription {
    /// `+0x20`: string table of the recipe labels.
    pub string_table: Option<String>,
    /// `+0x28/+0x30`.
    pub recipes: Vec<ProduceRecipe>,
    /// `+0x40`: work cycles per production.
    pub cycles: i32,
    /// `+0x50`: work type (tool mesh, PerformWork animation `type + 9` below 11, walk mode).
    pub work_type: u32,
    /// `+0x38` / `+0x3c`: a message (type, value) sent to the building's component of that type
    /// after each production.
    pub message_type: u32,
    pub message_value: i32,
    /// `+0x44`: seconds per work cycle.
    pub duration: f32,
    /// `+0x48`: productions before the workplace is exhausted (−1: never).
    pub lifetime: i32,
    /// `+0x4c`: initial recipe selector (−1: weighted random).
    pub selector: i32,
}

pub fn consume_produce_description(data: &[u8]) -> Result<ConsumeProduceDescription> {
    let (_, value) = crs::parse(data, |r: &mut Reader, h| {
        if h.type_id != type_id("ConsumeProduceDescription") {
            return format_err(format!(
                "expected ConsumeProduceDescription, found {:#010x}",
                h.type_id
            ));
        }
        r.block(|r, _| {
            let string_table = r.external_ref()?;
            let n = r.count()?;
            let mut recipes = Vec::with_capacity(n);
            for _ in 0..n {
                let k = r.count()?;
                let inputs = (0..k)
                    .map(|_| Ok((r.external_ref()?, r.i32()?)))
                    .collect::<Result<_>>()?;
                recipes.push(ProduceRecipe {
                    inputs,
                    output: r.external_ref()?,
                    group: r.i32()?,
                    label: r.string()?,
                });
            }
            Ok(ConsumeProduceDescription {
                string_table,
                recipes,
                cycles: r.i32()?,
                work_type: r.u32()?,
                message_type: r.u32()?,
                message_value: r.i32()?,
                duration: r.f32()?,
                lifetime: r.i32()?,
                selector: r.i32()?,
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
