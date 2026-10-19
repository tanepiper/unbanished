//! Goods and where they go: raw materials, storage, residences, markets, trade and values.

use super::*;

/// `RawMaterialDescription`: a storable resource.
///
/// Original: `FUN_14012e9f0`.
#[derive(Debug, Clone)]
pub struct RawMaterialDescription {
    /// The icon's sprite sheet.
    pub sprite_sheet: Option<String>,
    /// The string table.
    pub string_table: Option<String>,
    // +0x30, +0x38
    /// The name's key (`Log`).
    pub name: String,
    /// The short name's key (`LogSmall`).
    pub name_small: String,
    // +0x40; +0x44
    /// Not identified yet.
    pub value_40: i32,
    /// Not identified yet.
    pub value_44: i32,
    // +0x48
    /// Storage units per item.
    ///
    /// Original: `FUN_140132250`.
    pub size: i32,
    // +0x4c
    /// Not identified yet.
    pub value_4c: i32,
    // +0x5c
    /// The category flags (food, fuel, clothing and so on), which the game maps to its stock
    /// table.
    ///
    /// Original: mapped by `FUN_14008f3a0`.
    pub flags: u32,
    // +0x50
    /// Not identified yet.
    pub value_50: f32,
    // +0x54
    /// Not identified yet.
    pub value_54: i32,
    // +0x58
    /// Not identified yet.
    pub flag_58: bool,
}

/// Read a [`RawMaterialDescription`] from the bytes of its resource.
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

/// `StorageDescription`.
///
/// Original: `FUN_140131e70`.
#[derive(Debug, Clone)]
pub struct StorageDescription {
    // +0x28
    /// Raw-material flags this storage accepts.
    pub accepts: u32,
    /// Whether the capacity is per footprint cell.
    pub flag_20: bool,
    /// Not identified yet.
    pub flag_21: bool,
    // +0x24
    /// Capacity in storage units.
    pub capacity: i32,
    // +0x30
    /// `(count, raw material)` entries.
    pub items: Vec<(i32, Option<String>)>,
}

/// Read a [`StorageDescription`] from the bytes of its resource.
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

// +0x20
/// The mask of a `StorageLocationDescription`: the kinds of goods a storage place accepts.
///
/// Original: `FUN_140132940` (version 2).
pub fn storage_location_description(data: &[u8]) -> Result<u32> {
    let (_, value) = crs::parse(data, |r: &mut Reader, h| {
        if h.type_id != type_id("StorageLocationDescription") {
            return format_err("expected StorageLocationDescription");
        }
        r.block(|r, _| r.u32())
    })?;
    Ok(value)
}

// +0x20
/// `AutoPickupDescription`:, whether the workplace's workers pick up the piles they make first.
///
/// Original: `FUN_1400454c0`.
pub fn auto_pickup_description(data: &[u8]) -> Result<bool> {
    let (_, value) = crs::parse(data, |r: &mut Reader, h| {
        if h.type_id != type_id("AutoPickupDescription") {
            return format_err(format!(
                "expected AutoPickupDescription, found {:#010x}",
                h.type_id
            ));
        }
        r.block(|r, _| r.bool())
    })?;
    Ok(value)
}

/// `ResidenceDescription` (version 2).
///
/// Original: `FUN_14012f8a0`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResidenceDescription {
    // +0x20; +0x50; +0x58
    /// Family slots (ResidenceComponent, count).
    pub families: i32,
    // +0x24
    /// Residents per family before it counts as full.
    ///
    /// Original: `FUN_14012f9c0`.
    pub family_size: i32,
    // +0x28
    /// At or above this temperature the home is warm without fuel.
    ///
    /// Original: `FUN_140130100`.
    pub warm_temperature: f32,
    // +0x2c
    /// Seconds one unit of firewood keeps the home warm.
    ///
    /// Original: `FUN_14012ee40`.
    pub fuel_seconds: f32,
}

/// Read a [`ResidenceDescription`] from the bytes of its resource.
pub fn residence_description(data: &[u8]) -> Result<ResidenceDescription> {
    let (_, value) = crs::parse(data, |r: &mut Reader, h| {
        if h.type_id != type_id("ResidenceDescription") {
            return format_err(format!(
                "expected ResidenceDescription, found {:#010x}",
                h.type_id
            ));
        }
        r.block(|r, _| {
            Ok(ResidenceDescription {
                families: r.i32()?,
                family_size: r.i32()?,
                warm_temperature: r.f32()?,
                fuel_seconds: r.f32()?,
            })
        })
    })?;
    Ok(value)
}

// +0x20; +0x28
/// `MarketDescription`: vendors stop while the market is fuller than `threshold`; the categories (raw-material
/// flags) and each one's share of the market's room.
///
/// Original: `FUN_14004f510`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MarketDescription {
    /// How full the market may be before vendors stop.
    pub threshold: f32,
    /// Each category's raw-material flags and share of the market's room.
    pub quotas: Vec<(u32, f32)>,
}

/// Read a [`MarketDescription`] from the bytes of its resource.
pub fn market_description(data: &[u8]) -> Result<MarketDescription> {
    let (_, value) = crs::parse(data, |r: &mut Reader, h| {
        if h.type_id != type_id("MarketDescription") {
            return format_err(format!(
                "expected MarketDescription, found {:#010x}",
                h.type_id
            ));
        }
        r.block(|r, _| {
            let threshold = r.f32()?;
            let n = r.i32()?;
            let quotas = (0..n.max(0))
                .map(|_| Ok((r.u32()?, r.f32()?)))
                .collect::<Result<_>>()?;
            Ok(MarketDescription { threshold, quotas })
        })
    })?;
    Ok(value)
}

// +0x20; +0x30; +0x40; +0x10; +0x14; +0x18
/// A TradeDescription merchant (0x48 bytes): its name (string table, key), the templates it always
/// offers (seeds, livestock), integers the game does not read, its lots before the town's divisor, the raw-material
/// flags it sells, and more flag masks.
///
/// Original: `FUN_14005d510`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Merchant {
    /// The name: string table and key.
    pub name: (Option<String>, String),
    /// The templates it always offers (seeds, livestock).
    pub items: Vec<Option<String>>,
    /// Integers the game does not read.
    pub values: Vec<i32>,
    /// Its lots, before the town's divisor.
    pub lots: i32,
    /// The raw-material flags it sells.
    pub sells: u32,
    /// More flag masks; not identified yet.
    pub flags: (u32, u32),
}

// +0; +0xc; +0x10
/// A TradeDescription row by population (0x14 bytes): towns below `population` people see a merchant
/// every `(months + RandFloatTol × range)` × 300 s, and divide the merchant's lots and each lot's amount.
///
/// Original: `FUN_14005d480`.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct TradeRow {
    /// The population this row applies below.
    pub population: i32,
    /// The base time between merchants, in units of 300 seconds.
    pub months: f32,
    /// The random part of that time.
    pub range: f32,
    /// What the merchant's lots are divided by.
    pub lot_divisor: i32,
    /// What each lot's amount is divided by.
    pub amount_divisor: i32,
}

/// A `TradeDescription`: the trading post's boat, merchants and trade window.
///
/// Original: `FUN_14005d150`, `FUN_14005d400`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TradeDescription {
    // +0x30
    /// The trader boat template.
    pub boat: Option<String>,
    // +0x48
    /// The merchants that can come.
    pub merchants: Vec<Merchant>,
    // +0x58
    /// The rows of the trade table, by population.
    pub rows: Vec<TradeRow>,
    // +0x20
    /// The trade window's sprite sheet.
    pub sprites: Option<String>,
    // +0x28
    /// The trade window's string table.
    pub strings: Option<String>,
    // +0x38
    /// The trade window's categories: flags and two string keys each.
    pub categories: Vec<(u32, String, String)>,
    // +0x68
    /// Not identified yet (0.33 in the game data).
    pub value_68: f32,
    // +0x6c
    /// Not identified yet (3 in the game data).
    pub value_6c: i32,
}

/// Read a [`TradeDescription`] from the bytes of its resource.
pub fn trade_description(data: &[u8]) -> Result<TradeDescription> {
    let (_, value) = crs::parse(data, |r: &mut Reader, h| {
        if h.type_id != type_id("TradeDescription") {
            return format_err(format!(
                "expected TradeDescription, found {:#010x}",
                h.type_id
            ));
        }
        r.block(|r, _| {
            let boat = r.external_ref()?;
            let merchants = (0..r.count()?)
                .map(|_| {
                    let name = (r.external_ref()?, r.string()?);
                    let items = (0..r.count()?)
                        .map(|_| r.external_ref())
                        .collect::<Result<_>>()?;
                    let values = (0..r.count()?).map(|_| r.i32()).collect::<Result<_>>()?;
                    let lots = r.i32()?;
                    let sells = r.u32()?;
                    let flags = (r.u32()?, r.u32()?);
                    Ok(Merchant {
                        name,
                        items,
                        values,
                        lots,
                        sells,
                        flags,
                    })
                })
                .collect::<Result<_>>()?;
            let rows = (0..r.count()?)
                .map(|_| {
                    Ok(TradeRow {
                        population: r.i32()?,
                        months: r.f32()?,
                        range: r.f32()?,
                        lot_divisor: r.i32()?,
                        amount_divisor: r.i32()?,
                    })
                })
                .collect::<Result<_>>()?;
            let sprites = r.external_ref()?;
            let strings = r.external_ref()?;
            let categories = (0..r.count()?)
                .map(|_| Ok((r.u32()?, r.string()?, r.string()?)))
                .collect::<Result<_>>()?;
            Ok(TradeDescription {
                boat,
                merchants,
                rows,
                sprites,
                strings,
                categories,
                value_68: r.f32()?,
                value_6c: r.i32()?,
            })
        })
    })?;
    Ok(value)
}

/// A `ValueDescription`: what one unit of a good is worth in trade, and how it is shown.
///
/// Original: `FUN_14005d6c0`, `FUN_14005d7e0`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ValueDescription {
    // +0x20/+0x28
    /// The icon: sprite sheet and sprite name.
    pub icon: (Option<String>, String),
    // +0x30/+0x38
    /// The name: string table and key.
    pub name: (Option<String>, String),
    // +0x40
    /// The value of one unit.
    pub value: i32,
    // +0x44
    /// A merchant's lot, before the town's divisor (100).
    pub lot: i32,
    // +0x48
    /// The smallest lot.
    pub min_lot: i32,
}

/// Read a [`ValueDescription`] from the bytes of its resource.
pub fn value_description(data: &[u8]) -> Result<ValueDescription> {
    let (_, value) = crs::parse(data, |r: &mut Reader, h| {
        if h.type_id != type_id("ValueDescription") {
            return format_err(format!(
                "expected ValueDescription, found {:#010x}",
                h.type_id
            ));
        }
        r.block(|r, _| {
            Ok(ValueDescription {
                icon: (r.external_ref()?, r.string()?),
                name: (r.external_ref()?, r.string()?),
                value: r.i32()?,
                lot: r.i32()?,
                min_lot: r.i32()?,
            })
        })
    })?;
    Ok(value)
}
