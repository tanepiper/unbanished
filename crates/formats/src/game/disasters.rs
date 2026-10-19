//! Fires and tornadoes.

use super::*;

// Template\Common.rsc:fire*.
/// A `FireDescription`: how a fire burns, spreads and is put out.
///
/// Original: `FUN_14004a560`; fires that start by themselves, `FUN_1400ae760`.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct FireDescription {
    // +0x20
    /// The burning time: `burn_months × 300 + burn` seconds.
    pub burn: f32,
    // +0x24
    /// The burning time in months of 300 seconds (see `burn`).
    pub burn_months: f32,
    // +0x28
    /// The particle slot of the flames.
    pub particle: i32,
    // +0x2c
    /// The ambient emitter slot of the sound.
    pub emitter: i32,
    // +0x30
    /// The time between attempts to spread, in months of 300 seconds. Each bucket of water
    /// takes `interval × 300 / buckets` seconds off.
    pub interval: f32,
    // +0x34
    /// The odds that a neighbour catches fire.
    pub spread: f32,
    // +0x38
    /// The emergency radius, in cells: citizens in it come to put the fire out.
    pub radius: i32,
    // +0x3c
    /// The buckets of water that put the fire out.
    pub buckets: i32,
    // +0x40
    /// The most firefighters.
    pub workers: i32,
    // +0x44
    /// How often a building catches fire by itself, in years (with `odds`).
    pub years: f32,
    // +0x48
    /// The odds that a building catches fire by itself (with `years`).
    pub odds: f32,
}

/// Read a [`FireDescription`] from the bytes of its resource.
pub fn fire_description(data: &[u8]) -> Result<FireDescription> {
    let (_, value) = crs::parse(data, |r: &mut Reader, h| {
        if h.type_id != type_id("FireDescription") {
            return format_err(format!(
                "expected FireDescription, found {:#010x}",
                h.type_id
            ));
        }
        r.block(|r, _| {
            let mut d = FireDescription {
                burn: r.f32()?,
                burn_months: r.f32()?,
                particle: r.i32()?,
                emitter: r.i32()?,
                interval: r.f32()?,
                spread: r.f32()?,
                ..Default::default()
            };
            d.buckets = r.i32()?;
            d.years = r.f32()?;
            d.odds = r.f32()?;
            d.radius = r.i32()?;
            d.workers = r.i32()?;
            Ok(d)
        })
    })?;
    Ok(value)
}

// +0x20; +0x24; +0x30; +0x28; +0x34
/// `TornadoDescription`: speed, the ends' least distance, the reach of a wrecking pass, the time
/// between passes, how far a pass throws its point.
///
/// Original: `FUN_1400596f0`.
pub fn tornado_description(data: &[u8]) -> Result<(f32, i32, i32, i64, i32)> {
    let (_, value) = crs::parse(data, |r: &mut Reader, h| {
        if h.type_id != type_id("TornadoDescription") {
            return format_err(format!(
                "expected TornadoDescription, found {:#010x}",
                h.type_id
            ));
        }
        r.block(|r, _| {
            let speed = r.f32()?;
            let distance = r.i32()?;
            let reach = r.i32()?;
            let interval = r.i64()?;
            let throw = r.i32()?;
            Ok((speed, distance, reach, interval, throw))
        })
    })?;
    Ok(value)
}
