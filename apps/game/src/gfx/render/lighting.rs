//! The sun and ambient light by month, weather and season.

use super::*;

impl Lighting {
    /// One month of `EnvironmentDescription`, as `FUN_140098da0` applies it.
    pub fn from_month(m: &unbanished_formats::game::EnvironmentMonth) -> Lighting {
        Lighting::between_months(m, m, 0.0)
    }

    /// Interpolate adjacent month records as `FUN_140098f90` does before applying the sun and fog.
    pub fn between_months(
        from: &unbanished_formats::game::EnvironmentMonth,
        to: &unbanished_formats::game::EnvironmentMonth,
        amount: f32,
    ) -> Lighting {
        let amount = amount.clamp(0.0, 1.0);
        let blend = |a: f32, b: f32| a + (b - a) * amount;
        let (yaw, pitch) = (
            blend(from.sun_yaw, to.sun_yaw).to_radians(),
            blend(from.sun_pitch, to.sun_pitch).to_radians(),
        );
        // Math::Vector3::FromYawPitch points towards the sun; the shader wants the travel
        // direction (inferred from the sign of its diffuse term).
        let to_sun = Vec3::new(
            -pitch.cos() * yaw.sin(),
            pitch.cos() * yaw.cos(),
            pitch.sin(),
        );
        Lighting {
            light_dir: -to_sun,
            light_color: color(from.sun_color).lerp(color(to.sun_color), amount),
            ambient_color: color(from.ambient_color).lerp(color(to.ambient_color), amount),
            fog_color: color(from.fog_color).lerp(color(to.fog_color), amount),
            fog_max: blend(from.fog_max, to.fog_max),
            fog_start: blend(from.fog_start, to.fog_start),
            fog_end: blend(from.fog_end, to.fog_end),
            user: Vec4::ZERO,
        }
    }

    /// `FUN_140098f90` with ShowWeather: a weather effect's lighting record (Weather `+0x60`
    /// ambient, `+0x64` sun, `+0x68` fog colours, `+0x6c..+0x74` fog start, end and most) pulls
    /// the lighting towards it by the effect's intensity (`FUN_140099ec0` without the sun's
    /// angles).
    pub fn toward_weather(&mut self, w: &unbanished_formats::weather::Weather, amount: f32) {
        let t = amount.clamp(0.0, 1.0);
        let blend = |a: f32, b: f32| a + (b - a) * t;
        self.ambient_color = self.ambient_color.lerp(color(w.colors[0]), t);
        self.light_color = self.light_color.lerp(color(w.colors[1]), t);
        self.fog_color = self.fog_color.lerp(color(w.colors[2]), t);
        self.fog_start = blend(self.fog_start, w.floats_6c[0]);
        self.fog_end = blend(self.fog_end, w.floats_6c[1]);
        self.fog_max = blend(self.fog_max, w.floats_6c[2]);
    }

    /// Apply the date-driven foliage controls computed by `FUN_1400988f0`.
    pub fn set_seasonal_foliage(&mut self, month_position: f32) {
        self.user.y = ((month_position - 6.0) / 1.5).clamp(0.0, 1.0);
        self.user.z = if month_position < 0.5 {
            (1.0 - month_position / 0.5).clamp(0.0, 1.0)
        } else if month_position <= 7.5 {
            0.0
        } else {
            ((month_position - 7.5) * 2.0).clamp(0.0, 1.0)
        };
    }
}
