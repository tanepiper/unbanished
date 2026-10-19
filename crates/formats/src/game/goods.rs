//! Goods and where they go: raw materials, storage, residences, markets, trade and values.

use super::*;

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

/// `AutoPickupDescription` (serializer `FUN_1400454c0`): `+0x20`, whether the workplace's
/// workers pick up the piles they make first.
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

/// `ResidenceDescription` (serializer `FUN_14012f8a0`, version 2).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResidenceDescription {
    /// `+0x20`: family slots (ResidenceComponent `+0x50`, count `+0x58`).
    pub families: i32,
    /// `+0x24`: residents per family before it counts as full (`FUN_14012f9c0`).
    pub family_size: i32,
    /// `+0x28`: at or above this temperature the home is warm without fuel (`FUN_140130100`).
    pub warm_temperature: f32,
    /// `+0x2c`: seconds one unit of firewood keeps the home warm (`FUN_14012ee40`).
    pub fuel_seconds: f32,
}

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

/// `MarketDescription` (serializer `FUN_14004f510`): vendors stop while the market is fuller
/// than `+0x20`; `+0x28` the categories (raw-material flags) and each one's share of the
/// market's room.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MarketDescription {
    pub threshold: f32,
    pub quotas: Vec<(u32, f32)>,
}

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

/// A TradeDescription merchant (`FUN_14005d510`, 0x48 bytes): its name (string table, key),
/// `+0x20` the templates it always offers (seeds, livestock), `+0x30` (ints, unread), `+0x40`
/// its lots before the town's divisor, `+0x10` the raw-material flags it sells, `+0x14` and
/// `+0x18` more flag masks.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Merchant {
    pub name: (Option<String>, String),
    pub items: Vec<Option<String>>,
    pub values: Vec<i32>,
    pub lots: i32,
    pub sells: u32,
    pub flags: (u32, u32),
}

/// A TradeDescription row by population (`FUN_14005d480`, 0x14 bytes): towns below `+0`
/// people see a merchant every `(+4 + RandFloatTol × +8)` × 300 s; `+0xc` and `+0x10` divide
/// the merchant's lots and each lot's amount.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct TradeRow {
    pub population: i32,
    pub months: f32,
    pub range: f32,
    pub lot_divisor: i32,
    pub amount_divisor: i32,
}

/// `TradeDescription` (serializer `FUN_14005d150`, constructor `FUN_14005d400`): `+0x30` the
/// trader boat, `+0x48` the merchants, `+0x58` the rows by population, `+0x20/+0x28` the trade
/// window's sprite sheet and strings, `+0x38` its categories (flags, two string keys),
/// `+0x68` (0.33) and `+0x6c` (3).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TradeDescription {
    pub boat: Option<String>,
    pub merchants: Vec<Merchant>,
    pub rows: Vec<TradeRow>,
    pub sprites: Option<String>,
    pub strings: Option<String>,
    pub categories: Vec<(u32, String, String)>,
    pub value_68: f32,
    pub value_6c: i32,
}

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

/// `ValueDescription` (serializer `FUN_14005d6c0`, constructor `FUN_14005d7e0`): `+0x20/+0x28`
/// the icon (sprite sheet, name), `+0x30/+0x38` the name (string table, key), `+0x40` the
/// value of one, `+0x44` a merchant's lot before the town's divisor (100), `+0x48` the
/// smallest lot.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ValueDescription {
    pub icon: (Option<String>, String),
    pub name: (Option<String>, String),
    pub value: i32,
    pub lot: i32,
    pub min_lot: i32,
}

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
