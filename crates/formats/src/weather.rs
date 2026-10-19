//! `Weather`: a precipitation effect description (`Environment/Rain.rsc`, `Environment/Snow.rsc`),
//! referenced from the EnvironmentDescription's weather list.
//!
//! Original: `FUN_1400d9160`.

use crate::crc::type_id;
use crate::crs::{self, Reader};
use crate::error::{format_err, Result};

/// A weather effect: rain or snow particles and how they fall.
#[derive(Debug, Clone)]
pub struct Weather {
    // +0x20
    /// Particle material.
    pub material: Option<String>,
    // +0x28
    /// Particle count.
    pub count: i32,
    // +0x2c; normalised by FUN_1400d9450.
    /// The direction the particles fall in (the game normalises it).
    pub direction: [f32; 3],
    // +0x38, +0x3c
    /// Particle speed base and random range.
    pub speed: [f32; 2],
    // +0x40
    /// Not identified yet.
    pub flag: bool,
    // +0x44..+0x58; +0x48
    /// is the particle volume's size.
    pub floats_44: [f32; 6],
    // +0x5c; geometry +0xc0
    /// Seconds to fade the intensity (`geometry`) from 0 to 1.
    ///
    /// Original: `FUN_1400d8330`.
    pub fade_time: f32,
    // +0x60..+0x68
    /// Not identified yet.
    pub colors: [u32; 3],
    // +0x6c..+0x7c
    /// Not identified yet.
    pub floats_6c: [f32; 5],
}

/// Read a [`Weather`] from the bytes of its resource.
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
