//! Fires and tornadoes.

use super::*;

/// `FireDescription` (serializer `FUN_14004a560`; `Template\Common.rsc:fire*`): a fire burns
/// `+0x24` × 300 + `+0x20` s; `+0x28` the ParticleDescription slot, `+0x2c` the ambient emitter;
/// `+0x30` × 300 s between spread attempts (and `+0x30` × 300 / `+0x3c` s off per bucket of
/// water), `+0x34` the odds it catches from a neighbour, `+0x38` the emergency radius, `+0x3c`
/// the buckets that put it out, `+0x40` the most firefighters, `+0x44` / `+0x48` the years and
/// odds of catching fire by itself (`FUN_1400ae760`).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct FireDescription {
    pub burn: f32,
    pub burn_months: f32,
    pub particle: i32,
    pub emitter: i32,
    pub interval: f32,
    pub spread: f32,
    pub radius: i32,
    pub buckets: i32,
    pub workers: i32,
    pub years: f32,
    pub odds: f32,
}

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

/// `TornadoDescription` (serializer `FUN_1400596f0`): `+0x20` speed, `+0x24` the ends' least
/// distance, `+0x30` the reach of a wrecking pass, `+0x28` the time between passes, `+0x34`
/// how far a pass throws its point.
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
