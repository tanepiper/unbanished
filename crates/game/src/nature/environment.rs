//! The environment: the temperature, the weather, and the seasonal values the renderer uses. It is
//! advanced once per simulation iteration, after the calendar. See docs/ENVIRONMENT.md,
//! docs/GAME-LOOP-STEP-04.md and docs/GAME-LOOP-STEP-05.md.
//!
//! Original: `FUN_140099430` (set-up), `FUN_140099300` (bound to the climate), `FUN_1400999b0`
//! (the update).
// Game/Game +0x130

use unbanished_formats::game::ClimateGenerator;

use crate::noise::PerlinNoise;
use crate::random::Random;

// DAT_14017895c
/// Seconds in a game year.
pub const YEAR_SECONDS: f32 = 3600.0;
/// `DAT_1401788e4`: weather slices per year.
const SLICES: f32 = 48.0;
/// `DAT_140178800`, `DAT_140178894`: temperature Perlin persistence and octaves.
const NOISE_PERSISTENCE: f32 = 0.25;
const NOISE_OCTAVES: f32 = 4.0;
/// `DAT_1401787b4` / `DAT_140178848`: smoothing weights of the annual targets.
const SMOOTH_NEW: f32 = 0.000_999_987_1;
const SMOOTH_OLD: f32 = 0.999;
/// `DAT_1401787e0`: 1/12.
const TWELFTH: f32 = 0.083_333_336;
/// `DAT_140178920`: seconds per Perlin unit.
const NOISE_PERIOD: f32 = 180.0;
/// `DAT_140178934`: seconds per month (also the sine phase offset).
const MONTH_SECONDS: f32 = 300.0;
// DAT_1401788d8
/// Rain above this temperature, Snow at or below.
pub const RAIN_SNOW_THRESHOLD: f32 = 34.0;
// DAT_1401788d0
/// Freezing point (°F).
pub const FREEZING: f32 = 32.0;
/// Game option `DAT_1401cbde0 + 0x16` (weather effects), on by default (`FUN_140042850`).
const WEATHER_EFFECTS: bool = true;

// +0x50; +0x60
// The geometry is made by FUN_1400d93e0.
/// A weather effect slot: its particle geometry and its ambient sound. Slot 0 is rain, slot 1 is
/// snow (the order of the environment's weather list).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WeatherEffect {
    // geometry +0xc0
    /// `geometry`: 0..1, also the ambient emitter's volume.
    pub intensity: f32,
    // geometry +0xc4
    /// `geometry`: +1 fading in, −1 fading out.
    pub direction: f32,
}

/// The climate fields the controller reads (`ClimateGenerator`).
///
/// Original: `FUN_140098470`.
#[derive(Debug, Clone, PartialEq)]
pub struct Climate {
    // +0x34
    /// Baseline temperature.
    pub baseline: f32,
    // +0x38
    /// Amplitude of the yearly sine.
    pub seasonal: f32,
    // +0x3c
    /// Amplitude of the Perlin noise.
    pub noise: f32,
    // +0x40
    /// Range of the annual random baseline offset.
    pub baseline_range: f32,
    // +0x44
    /// Four floats per month; the first three are cumulative weather-state cutoffs.
    pub months: [[f32; 4]; 12],
    // +0x104
    /// Range of the annual weather-cutoff bias.
    pub bias_range: f32,
    // +0x108, +0x10c
    /// A tornado's odds while it rains (years, odds).
    ///
    /// Original: `FUN_1400ae760`.
    pub tornado: (f32, f32),
}

impl From<&ClimateGenerator> for Climate {
    fn from(c: &ClimateGenerator) -> Climate {
        Climate {
            baseline: c.settings[0],
            seasonal: c.settings[1],
            noise: c.settings[2],
            baseline_range: c.settings[3],
            months: c.monthly_thresholds,
            bias_range: c.trailing_settings[0],
            tornado: (c.trailing_settings[1], c.trailing_settings[2]),
        }
    }
}

/// The values the renderer receives from the environment each update (the shaders'
/// `pc_userPosition`).
///
/// Original: `FUN_1400988f0`.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SeasonValues {
    // +0x1c
    /// Snow cover, `clamp(32 − temperature, 0, 1)`.
    pub snow: f32,
    // +0x20
    /// Autumn tint.
    pub autumn: f32,
    // +0x24
    /// Leaf loss.
    pub leaf_loss: f32,
    // +0x28; geometry +0xc0
    /// Rain, the strongest active weather effect's intensity (`geometry`).
    pub rain: f32,
}

/// The environment: temperature, weather and the renderer's seasonal values. See the [module
/// documentation](self).
#[derive(Debug, Clone)]
pub struct Environment {
    /// The climate.
    pub climate: Climate,
    // +0x10
    /// 0 precipitation, 1 cloudy, 2 partly sunny, 3 sunny.
    pub state: i32,
    // +0x14
    /// Precipitation level 0..5 (starts at 4).
    pub level: i32,
    // +0x18
    /// °F.
    pub temperature: f32,
    /// The renderer's seasonal values.
    pub season: SeasonValues,
    /// `+0x2c` / `+0x30`: smoothed and target annual baseline.
    baseline: f32,
    baseline_target: f32,
    /// `+0x34`: temperature noise.
    noise: PerlinNoise,
    /// `+0x44` / `+0x48`: smoothed and target weather-cutoff bias.
    bias: f32,
    bias_target: f32,
    /// `+0x4c`: last processed weather slice (−1 before the first update).
    slice: i32,
    /// The Rain and Snow effects while they exist.
    pub effects: [Option<WeatherEffect>; 2],
    // Weather +0x5c
    /// `Weather` of each slot's description: seconds to fade in or out. A slot without a loaded
    /// description never starts.
    pub fade_times: [Option<f32>; 2],
    // +0x70
    /// The time since the last tornado check.
    pub tornado_timer: i64,
    // Game +0xd0
    /// `Game`: bit 7 stops tornadoes (set with disasters off).
    pub game_flags: u32,
}

/// `floor(elapsed / 3600 × 48)`, wrapped to 0 past 47.
fn weather_slice(elapsed: f32) -> i32 {
    let i = ((elapsed / YEAR_SECONDS) * SLICES) as i32;
    if i > 0x2f {
        0
    } else {
        i
    }
}

impl Environment {
    // State +0x10, level +0x14, temperature +0x18, renderer values +0x1c, baseline +0x2c/+0x30,
    // bias +0x44/+0x48, last slice +0x4c; each effect re-created by FUN_140099790 with its +0xc4
    // direction and +0xc0 intensity.
    /// The environment as a save stores it: the state, the level, the temperature, the renderer's
    /// values, the baseline, the bias, the last slice, and each weather effect that exists (made
    /// again on load with its direction and intensity).
    ///
    /// Original: `FUN_140099160`.
    ///
    /// Deviation: the temperature noise (`+0x34`) is not saved; after a load the original keeps
    /// its constructor's clock-seeded field until the next reseed (slice 0 or 24). The port
    /// keeps its constructor's fixed seed.
    #[allow(clippy::too_many_arguments)]
    pub fn restore(
        &mut self,
        state: i32,
        level: i32,
        temperature: f32,
        season: SeasonValues,
        baseline: (f32, f32),
        bias: (f32, f32),
        slice: i32,
        effects: [Option<WeatherEffect>; 2],
    ) {
        self.state = state;
        self.level = level;
        self.temperature = temperature;
        self.season = season;
        (self.baseline, self.baseline_target) = baseline;
        (self.bias, self.bias_target) = bias;
        self.slice = slice;
        self.effects = effects;
    }

    /// What [`Environment::restore`] takes, for saving: baseline and bias with their targets,
    /// and the last slice.
    pub fn saved(&self) -> ((f32, f32), (f32, f32), i32) {
        (
            (self.baseline, self.baseline_target),
            (self.bias, self.bias_target),
            self.slice,
        )
    }

    /// A new environment for a climate: state 0, level 4, and the climate's baseline.
    ///
    /// Original: `FUN_140099430`, then `FUN_140099300`.
    ///
    /// Deviation: the constructor seeds its first Perlin field from two clock-seeded `Random()`s;
    /// the first update always reseeds it (slice 0), so a fixed seed is used here.
    pub fn new(climate: Climate) -> Environment {
        Environment {
            state: 0,
            level: 4,
            temperature: 0.0,
            season: SeasonValues::default(),
            baseline: climate.baseline,
            baseline_target: climate.baseline,
            noise: PerlinNoise::new(NOISE_PERSISTENCE, NOISE_OCTAVES, 0, 0),
            bias: 0.0,
            bias_target: 0.0,
            slice: -1,
            effects: [None; 2],
            fade_times: [None; 2],
            tornado_timer: 0,
            game_flags: 0,
            climate,
        }
    }

    // Game/Game +0xd8; +0x70; FUN_1400ae760 (climate +0x108)
    /// One simulation iteration. `elapsed` is the calendar's time into the current year in seconds,
    /// already advanced for this iteration; `dt` is the iteration's step; `rng` is the game's
    /// shared random stream (`Game/Game`).
    ///
    /// Returns whether a tornado forms (step 6): this can happen while it rains above 34 °F with
    /// tornadoes on. The caller then creates the environment template's entity.
    ///
    /// Original: `FUN_1400999b0`, `FUN_1400c8bf0`, `FUN_1400c9d70`.
    pub fn update(&mut self, elapsed: f32, dt: f32, rng: &mut Random) -> bool {
        self.step_effects(dt);
        self.baseline = self.baseline_target * SMOOTH_NEW + self.baseline * SMOOTH_OLD;
        self.bias = self.bias_target * SMOOTH_NEW + self.bias * SMOOTH_OLD;
        self.update_temperature(elapsed);
        self.update_season(elapsed);
        let slice = weather_slice(elapsed);
        if slice != self.slice {
            self.slice = slice;
            self.next_weather(slice, rng);
        }
        self.switch_effects();
        self.tornado_timer += crate::scheduler::ticks(dt);
        if RAIN_SNOW_THRESHOLD < self.temperature && self.state == 0 && self.game_flags & 0x80 == 0
        {
            let (years, odds) = self.climate.tornado;
            if odds <= 0.0 {
                self.tornado_timer = 0;
                return false;
            }
            let period = years * 12.0 * 300.0;
            let since = crate::scheduler::seconds(self.tornado_timer);
            self.tornado_timer = 0;
            return rng.rand_float() < since / (period * odds);
        }
        false
    }

    /// `FUN_1400999b0`'s first loop: each effect geometry's update (`FUN_1400d8330`) moves its
    /// intensity by `dt / fade × direction`, clamped to 0..1; the emitter's volume follows. At
    /// intensity 0 (or with weather effects off) the geometry and its emitter are removed.
    fn step_effects(&mut self, dt: f32) {
        for (slot, fade) in self.effects.iter_mut().zip(self.fade_times) {
            let Some(e) = slot else { continue };
            let v = dt / fade.unwrap_or(1.0) * e.direction + e.intensity;
            e.intensity = if v > 1.0 {
                1.0
            } else if v < 0.0 {
                0.0
            } else {
                v
            };
            if e.intensity == 0.0 || !WEATHER_EFFECTS {
                *slot = None;
            }
        }
    }

    /// `FUN_1400999b0` → `FUN_140099790` for slots 0 and 1: a slot is wanted when weather
    /// effects are on, the state is 0 and the temperature picks it (Rain above 34°F, Snow at or
    /// below). An unwanted existing slot fades out and forces the state to 3; a wanted missing
    /// slot starts at intensity 0 fading in and sets the state to 0.
    fn switch_effects(&mut self) {
        let snow = self.temperature <= RAIN_SNOW_THRESHOLD;
        for i in 0..2 {
            let on = WEATHER_EFFECTS && self.state == 0 && snow == (i == 1);
            if !on {
                if let Some(e) = &mut self.effects[i] {
                    e.direction = -1.0;
                    self.state = 3;
                }
            } else if self.effects[i].is_none() && self.fade_times[i].is_some() {
                self.effects[i] = Some(WeatherEffect {
                    intensity: 0.0,
                    direction: 1.0,
                });
                self.state = 0;
            }
        }
    }

    /// Which weather effect slot is active: only in state 0 with weather enabled; slot 0 Rain above
    /// 34°, slot 1 Snow at or below.
    ///
    /// Original: `FUN_1400999b0`, `FUN_140099790`.
    pub fn active_effect(&self, weather_enabled: bool) -> Option<usize> {
        if !weather_enabled || self.state != 0 {
            return None;
        }
        Some(if self.temperature <= RAIN_SNOW_THRESHOLD {
            1
        } else {
            0
        })
    }

    /// `FUN_140098ce0`: smoothed baseline + yearly sine + Perlin noise.
    fn update_temperature(&mut self, elapsed: f32) {
        let noise = self.noise.generate(elapsed / NOISE_PERIOD, 0.0);
        // `FUN_140137160`: sine of `((t − 300) / 3600) × 360 × π/180`.
        let phase = ((elapsed - MONTH_SECONDS) / YEAR_SECONDS) * 360.0 * 0.017_453_292;
        let sine = crate::crt::sinf(phase);
        let t = sine * self.climate.seasonal + self.baseline;
        self.temperature = noise * self.climate.noise + t;
    }

    /// `FUN_1400988f0`: snow cover, autumn tint, leaf loss and rain for the shaders.
    fn update_season(&mut self, elapsed: f32) {
        let clamp = |v: f32| v.clamp(0.0, 1.0);
        let snow = clamp((self.temperature - FREEZING) * -1.0);
        let month = elapsed / MONTH_SECONDS;
        let autumn = if month >= 6.0 {
            clamp((month - 6.0) / 1.5)
        } else {
            0.0
        };
        let leaf_loss = if month < 0.5 {
            1.0 - clamp(month / 0.5)
        } else if month <= 7.5 {
            0.0
        } else {
            clamp((month - 7.5) * 2.0)
        };
        let rain = self.effects.iter().flatten().fold(0.0f32, |m, e| {
            if m < e.intensity {
                e.intensity
            } else {
                m
            }
        });
        self.season = SeasonValues {
            snow,
            autumn,
            leaf_loss,
            rain,
        };
    }

    /// `FUN_140098ac0`: at slices 0 and 24 reseed the noise and pick new annual targets, then
    /// draw the weather state from the month's biased cutoffs.
    fn next_weather(&mut self, slice: i32, rng: &mut Random) {
        if slice == 0 || slice == 24 {
            let first = rng.rand();
            let second = rng.rand();
            self.noise = PerlinNoise::new(
                NOISE_PERSISTENCE,
                NOISE_OCTAVES,
                second as i32,
                first as i32,
            );
            let (mut bias, mut baseline) = (0.0f32, 0.0f32);
            for _ in 0..12 {
                bias += rng.rand_float_tol() * TWELFTH;
                baseline += rng.rand_float_tol() * TWELFTH;
            }
            self.baseline_target = baseline * self.climate.baseline_range + self.climate.baseline;
            self.bias_target = bias * self.climate.bias_range;
        }
        let sample = rng.rand_float();
        let month = &self.climate.months[(slice / 4) as usize];
        self.state = 0;
        while !(sample < self.bias + month[self.state as usize]) {
            self.state += 1;
            if self.state >= 3 {
                break;
            }
        }
        match self.state {
            0 => self.level += 1,
            3 => self.level -= 1,
            _ => {}
        }
        self.level = self.level.clamp(0, 5);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fair() -> Climate {
        Climate {
            baseline: 50.0,
            seasonal: 30.0,
            noise: 5.0,
            baseline_range: 40.0,
            months: [[0.4, 0.5, 0.7, 1.0]; 12],
            bias_range: 0.4,
            tornado: (0.0, 0.0),
        }
    }

    #[test]
    fn slices_and_reseeding() {
        assert_eq!(weather_slice(0.0), 0);
        assert_eq!(weather_slice(75.0), 1);
        assert_eq!(weather_slice(3599.0), 47);
        let mut env = Environment::new(fair());
        let mut rng = Random::new(290);
        env.update(0.0, 0.0, &mut rng);
        assert_eq!(env.slice, 0);
        // Two Rand + 24 RandFloatTol + one RandFloat consumed.
        let mut expect = Random::new(290);
        for _ in 0..2 {
            expect.rand();
        }
        for _ in 0..24 {
            expect.rand_float_tol();
        }
        expect.rand_float();
        assert_eq!(rng.rand(), expect.rand());
        assert!((0..=3).contains(&env.state));
        assert!((0..=5).contains(&env.level));
    }

    #[test]
    fn temperature_follows_the_season() {
        let mut env = Environment::new(Climate {
            noise: 0.0,
            ..fair()
        });
        let mut rng = Random::new(1);
        // Month 0 starts at sin(−30°): baseline − 15.
        env.update(0.0, 0.0, &mut rng);
        assert!((env.temperature - 35.0).abs() < 0.01, "{}", env.temperature);
        assert_eq!(env.season.snow, 0.0);
        // Peak at month 4 (sin 90°).
        env.update(1200.0, 0.0, &mut rng);
        assert!(env.temperature > env.baseline + 29.0);
    }

    #[test]
    fn rain_fades_in_and_out() {
        let mut env = Environment::new(fair());
        env.fade_times = [Some(20.0), Some(20.0)];
        env.temperature = 50.0;
        env.state = 0;
        env.switch_effects();
        assert_eq!(env.effects[0].unwrap().intensity, 0.0);
        assert!(env.effects[1].is_none());
        // 10 s of a 20 s fade.
        for _ in 0..100 {
            env.step_effects(0.1);
        }
        assert!((env.effects[0].unwrap().intensity - 0.5).abs() < 1e-3);
        // Too cold for rain: the rain fades out, the state becomes 3 and snow does not start.
        env.temperature = 30.0;
        env.switch_effects();
        assert_eq!(env.state, 3);
        assert_eq!(env.effects[0].unwrap().direction, -1.0);
        assert!(env.effects[1].is_none());
        for _ in 0..101 {
            env.step_effects(0.1);
        }
        assert!(env.effects[0].is_none());
    }

    #[test]
    fn leaves_and_autumn() {
        let mut env = Environment::new(fair());
        env.update_season(0.0);
        assert_eq!(env.season.leaf_loss, 1.0);
        env.update_season(300.0 * 6.75);
        assert!((env.season.autumn - 0.5).abs() < 1e-6);
        env.update_season(300.0 * 8.0);
        assert_eq!(env.season.leaf_loss, 1.0);
    }
}
