//! GrowthComponent's scheduled and visual phases. Description field names remain numeric where the
//! original names are unknown.
//!
//! Original: `FUN_140125520`, `FUN_140125370`.

use crate::random::Random;
use crate::scheduler::{seconds, ticks};
use unbanished_formats::game::GrowthDescription;

/// DAT_140178934 / DAT_1401788d0 / DAT_14017898c.
const MONTH_SECONDS: f32 = 300.0;
const FREEZING: f32 = 32.0;
const FROZEN_WEIGHT: f32 = -10.0;
/// DAT_1401787b8 / DAT_140178844 / DAT_1401787c0.
const EPSILON: f32 = 0.001;
const OLD_WEIGHT: f32 = 0.99;
const NEW_WEIGHT: f32 = 0.009_999_99;

/// The growth state of a natural resource (a tree, a crop, a herb).
#[derive(Debug, Clone, PartialEq)]
pub struct Growth {
    // GrowthComponent +0x4c; drawn by FUN_140125bc0.
    /// The growth at which the resource is fully grown (a random value).
    pub maximum: f32,
    // +0x40
    /// System::Time, not an accumulated f32.
    pub timer: i64,
    // +0x48; +0x28
    /// Visible value, separate from NaturalResourceComponent.
    pub smoothed: f32,
    /// Whether the scheduled update still runs (false once fully grown).
    pub scheduled: bool,
    /// Whether the visible value still moves towards the growth.
    pub smoothing: bool,
    /// NaturalResourceComponent capability bit 0x100000; false on newly seeded resources.
    pub allow_negative: bool,
}

/// What one growth update changed.
#[derive(Debug, Default)]
pub struct GrowthTick {
    /// How many times it tried to spawn.
    pub spawn_attempts: usize,
    /// A value passed on when the growth ends; its meaning is not identified yet.
    ///
    /// Original: the argument of `FUN_140125940`.
    pub map_adjustment: Option<f32>,
}

impl Growth {
    /// Set the growth, the visible scale and the random timer, without another random draw for
    /// the maximum (seeding has drawn it already).
    ///
    /// Original: `FUN_140126280`.
    pub fn initialise(
        d: &GrowthDescription,
        maximum: f32,
        fraction: f32,
        rng: &mut Random,
    ) -> (Self, f32, [f32; 4]) {
        let value = fraction * maximum;
        let smoothed = value.min(1.0);
        let timer = ticks(rng.rand_float() * d.timer_months as f32 * MONTH_SECONDS);
        (
            Self {
                maximum,
                timer,
                smoothed,
                scheduled: true,
                smoothing: false,
                allow_negative: false,
            },
            value,
            scale(smoothed, d.tall),
        )
    }

    // +0x18; checked against the SSE disassembly.
    /// One scheduled update of a growing resource. The season comes from the environment's
    /// temperature, not from the renderer's autumn and leaf values.
    ///
    /// Original: `FUN_140125520`.
    pub fn update(
        &mut self,
        d: &GrowthDescription,
        value: &mut f32,
        dt: i64,
        temperature: f32,
        disabled: bool,
    ) -> GrowthTick {
        if !self.scheduled {
            return GrowthTick::default();
        }
        let rate = 1.0 / (d.f28 * MONTH_SECONDS);
        let weight = if d.b2c {
            let distance = (temperature - d.f30).abs() / d.f34;
            if !(0.0..=1.0).contains(&distance) {
                if temperature <= FREEZING {
                    FROZEN_WEIGHT
                } else {
                    0.0
                }
            } else {
                1.0 - distance * distance
            }
        } else {
            1.0
        };
        let next = *value + seconds(dt) * rate * weight;
        let mut result = GrowthTick::default();
        if next < 0.0 && !self.allow_negative {
            result.map_adjustment = Some(d.f40);
        } else {
            *value = next;
            self.smoothing = true;
            // Both comparisons in the original are strict. Preserve the float remainder's
            // conversion back to System::Time only when an interval was actually consumed.
            if d.b44 && next > d.threshold {
                self.timer += dt;
                let mut time = seconds(self.timer);
                let interval = d.timer_months as f32 * MONTH_SECONDS;
                // Deviation: malformed zero/negative intervals must not hang the browser.
                if interval > 0.0 && time > interval {
                    while time > interval {
                        result.spawn_attempts += 1;
                        time -= interval;
                    }
                    self.timer = ticks(time);
                }
            }
            if self.maximum > 0.0 && next > self.maximum && !self.allow_negative && !disabled {
                result.map_adjustment = Some(next.min(1.0) * d.f40);
            }
        }
        if result.map_adjustment.is_some() {
            *value = 0.0;
            self.scheduled = false;
            self.smoothing = false;
        }
        result
    }

    /// One smoothing step per active rendered update, including pause. Returns whether the instance
    /// scale changed.
    ///
    /// Original: `FUN_140125370`.
    pub fn smooth(&mut self, d: &GrowthDescription, value: f32, output: &mut [f32; 4]) -> bool {
        if !self.smoothing {
            return false;
        }
        let target = value.min(1.0).max(d.f60);
        let interpolating = (target - self.smoothed).abs() >= EPSILON;
        self.smoothed = if interpolating {
            self.smoothed * OLD_WEIGHT + target * NEW_WEIGHT
        } else {
            target
        };
        let next = scale(self.smoothed, d.tall);
        let changed = *output != next;
        *output = next;
        if !interpolating {
            self.smoothing = false;
        }
        changed
    }
}

// DAT_1401787f4: the width divisor.
/// The visible scale of a growing resource for its growth value: `[x, y, z, w]`. A `tall`
/// resource reaches its full width at growth 0.15 and then only grows in height; others scale
/// evenly.
///
/// Original: `FUN_140126280`, `FUN_140125370`.
pub fn scale(value: f32, tall: bool) -> [f32; 4] {
    if tall {
        let width = (value / 0.15).clamp(0.0, 1.0);
        [width, width, value, 1.0]
    } else {
        [value; 4]
    }
}

/// Retain the squared byte-fraction arithmetic and integer truncation.
///
/// Original: `FUN_140125940`.
pub fn adjust_map_byte(previous: u8, adjustment: f32) -> u8 {
    let fraction = previous as f32 / 255.0; // DAT_14017892c
    let next = if adjustment >= 0.0 {
        fraction * fraction * adjustment
    } else {
        fraction - adjustment
    };
    ((next * 255.0) as i32).clamp(0, 255) as u8
}
