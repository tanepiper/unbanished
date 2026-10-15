//! `Weather` (serializer `FUN_1400d9160`): a precipitation effect description
//! (`Environment/Rain.rsc`, `Environment/Snow.rsc`), referenced from the EnvironmentDescription's
//! weather list.

use crate::crc::type_id;
use crate::crs::{self, Reader};
use crate::error::{format_err, Result};

#[derive(Debug, Clone)]
pub struct Weather {
    /// `+0x20`: particle material.
    pub material: Option<String>,
    /// `+0x28`: particle count.
    pub count: i32,
    /// `+0x2c`: fall direction (normalised by `FUN_1400d9450`).
    pub direction: [f32; 3],
    /// `+0x38`, `+0x3c`: particle speed base and random range.
    pub speed: [f32; 2],
    /// `+0x40`.
    pub flag: bool,
    /// `+0x44..+0x58`: `+0x48` is the particle volume's size.
    pub floats_44: [f32; 6],
    /// `+0x5c`: seconds to fade the intensity (`geometry +0xc0`) from 0 to 1 (`FUN_1400d8330`).
    pub fade_time: f32,
    /// `+0x60..+0x68`.
    pub colors: [u32; 3],
    /// `+0x6c..+0x7c`.
    pub floats_6c: [f32; 5],
}

pub fn weather(data: &[u8]) -> Result<Weather> {
    let (_, value) = crs::parse(data, |r: &mut Reader, h| {
        if h.type_id != type_id("Weather") {
            return format_err(format!("expected Weather, found {:#010x}", h.type_id));
        }
        r.block(|r, _| {
            Ok(Weather {
                material: r.external_ref()?,
                count: r.u32()? as i32,
                direction: r.f32s()?,
                speed: r.f32s()?,
                flag: r.bool()?,
                floats_44: r.f32s()?,
                fade_time: r.f32()?,
                colors: [r.u32()?, r.u32()?, r.u32()?],
                floats_6c: r.f32s()?,
            })
        })
    })?;
    Ok(value)
}
