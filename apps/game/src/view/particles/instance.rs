//! One running particle system: spawning, moving and drawing its particles.

use super::*;

/// One particle (`0x58` bytes) and its motion (`0x14` bytes).
#[derive(Clone, Copy, Debug)]
pub(super) struct Particle {
    pub(super) life: f32,
    pub(super) age: f32,
    pub(super) pos: Vec3,
    pub(super) color: Vec3,
    pub(super) alpha: f32,
    pub(super) rotation: f32,
    pub(super) size: f32,
    pub(super) velocity: Vec3,
    pub(super) spin: f32,
    pub(super) growth: f32,
}

/// `Math::Transform::FromDirection` (Runtime `0x180022650`): the rotation whose +Y is `d`
/// (`x, y, z, w`). The other axes come from an up of +Z (−Y when `d.z` is above 0.999, +Y
/// when below −0.999): `x = |d × up|`, `z = |x × d|`, then `Quaternion(x, d, z)`
/// (`0x180022450`).
pub(super) fn from_direction(d: Vec3) -> [f32; 4] {
    let up = if 0.999 < d.z {
        Vec3::new(0.0, -1.0, 0.0)
    } else if d.z < -0.999 {
        Vec3::new(0.0, 1.0, 0.0)
    } else {
        Vec3::new(0.0, 0.0, 1.0)
    };
    let unit = |v: Vec3| {
        let len = v.length();
        if len != 0.0 {
            v * (1.0 / len)
        } else {
            v
        }
    };
    let x = unit(d.cross(up));
    let z = unit(x.cross(d));
    let (a, b, c) = (x, d, z);
    let part = |v: f32| v.max(0.0).sqrt() * 0.5;
    [
        part(1.0 + a.x - b.y - c.z).copysign(c.y - b.z),
        part(1.0 - a.x + b.y - c.z).copysign(a.z - c.x),
        part(1.0 - a.x - b.y + c.z).copysign(b.x - a.y),
        part(1.0 + a.x + b.y + c.z),
    ]
}

/// `q · v · q⁻¹` as the spawn writes it (`x, y, z, w`).
pub(super) fn rotate(q: [f32; 4], v: Vec3) -> Vec3 {
    let [x, y, z, w] = q;
    let t0 = w * v.x + z * v.y - y * v.z;
    let t1 = -z * v.x + w * v.y + x * v.z;
    let t2 = y * v.x - x * v.y + w * v.z;
    let t3 = -x * v.x - y * v.y - z * v.z;
    Vec3::new(
        t0 * w - t3 * x - t2 * y + t1 * z,
        t1 * w + t2 * x - t3 * y - t0 * z,
        t2 * w - t1 * x + t0 * y - t3 * z,
    )
}

/// A running system (`ParticleSystemInstance`): its particles and emission state, at a world
/// position with no rotation.
#[derive(Clone, Debug)]
pub struct Instance {
    pub system: Arc<System>,
    pub position: Vec3,
    /// The geometry's alpha (`+0x7c`), the component's fade.
    pub alpha: f32,
    /// Emitting (a tornado stops forming particles at the end of its path).
    pub emitting: bool,
    pub(super) rate: f32,
    pub(super) elapsed: f32,
    pub(super) since_last: f32,
    /// The Tornado instance's wobble angle (`+0xd8`, degrees).
    pub(super) turn: f32,
    pub(super) particles: Vec<Particle>,
}

impl Instance {
    pub fn new(system: Arc<System>, position: Vec3) -> Instance {
        Instance {
            rate: match &*system {
                System::Standard(s) => s.rate,
                System::Tornado(t) => t.rate,
            },
            system,
            position,
            alpha: 0.0,
            emitting: true,
            elapsed: 0.0,
            since_last: 0.0,
            turn: 0.0,
            particles: Vec::new(),
        }
    }

    #[cfg(test)]
    pub(super) fn is_empty(&self) -> bool {
        self.particles.is_empty()
    }

    pub(super) fn spawn(&mut self, s: &ParticleSystem, rng: &mut Random) {
        let tol = |rng: &mut Random| rng.rand_float_tol();
        let life = tol(rng) * s.life[1] + s.life[0];
        // The spawn box: z, y, x drawn in that order.
        let rz = tol(rng);
        let ry = tol(rng);
        let rx = tol(rng);
        let half = 0.5;
        let pos = Vec3::new(
            s.spawn_box[0] * rx * s.spawn_size * half,
            s.spawn_box[1] * ry * s.spawn_size * half,
            s.spawn_box[2] * rz * s.spawn_size * half,
        );
        let color = |c: u32| {
            Vec3::new(
                ((c >> 16) & 0xff) as f32 / 255.0,
                ((c >> 8) & 0xff) as f32 / 255.0,
                (c & 0xff) as f32 / 255.0,
            )
        };
        let rotation = tol(rng) * s.rotation[1] + s.rotation[0];
        let size = tol(rng) * s.size[1] + s.size[0];
        let mut spin = tol(rng) * s.spin[1] + s.spin[0];
        if rng.rand() & 1 == 0 {
            spin = -spin;
        }
        let speed = tol(rng) * s.speed[1] + s.speed[0];
        let elevation = tol(rng) * s.cone[1] * DEG;
        let azimuth = tol(rng) * s.cone[0] * DEG;
        let (sa, ca) = azimuth.sin_cos();
        let (sb, cb) = elevation.sin_cos();
        // Turned to the emission direction (`Transform::FromDirection`; the instance has no
        // rotation, so its direction is the system's). Every shipped system emits along +Y,
        // which this leaves as is; a mod's may not.
        let cone = Vec3::new(-cb * sa * speed, cb * ca * speed, sb * speed);
        let velocity = rotate(from_direction(Vec3::from(s.direction)), cone);
        self.particles.push(Particle {
            life,
            age: 0.0,
            pos,
            color: color(s.colors[0]),
            alpha: 0.0,
            rotation,
            size,
            velocity,
            spin,
            growth: 0.0,
        });
    }

    /// One update by `dt` seconds.
    pub fn update(&mut self, dt: f32, rng: &mut Random) {
        match &*self.system.clone() {
            System::Standard(s) => self.update_standard(s, dt, rng),
            System::Tornado(t) => self.update_tornado(t, dt, rng),
        }
    }

    /// `FUN_1400d7940`: emit at `+0x5c` a second, then place every particle by its age `t`
    /// (of its life) on the funnel:
    /// - the wobble `W = (cos((t − 0.2)·2π)/2 + 1/2)(t + 1/2) − 0.33` of `+0x50` about the
    ///   instance's angle (which turns `+0x54`° a second);
    /// - the spiral `R = |(t − 0.4)³|·3.7 + 0.05 + 4·max(t − 0.3, 0)²` of `+0x48` at the
    ///   angle `+0x58·t/π` (radians);
    /// - up `+0x4c·t` plus `|rotation|/40`;
    ///
    /// spinning 90° a second, sized `R·+0x70` within `+0x68`..`+0x6c`, coloured and faded
    /// from the start colour to the end colour over its life.
    pub(super) fn update_tornado(&mut self, s: &TornadoParticle, dt: f32, rng: &mut Random) {
        self.elapsed += dt;
        self.since_last += dt;
        let interval = 1.0 / s.rate;
        let rgba = |c: u32| {
            [
                ((c >> 16) & 0xff) as f32 / 255.0,
                ((c >> 8) & 0xff) as f32 / 255.0,
                (c & 0xff) as f32 / 255.0,
                (c >> 24) as f32 / 255.0,
            ]
        };
        let (c0, c1) = (rgba(s.colors[0]), rgba(s.colors[1]));
        while interval < self.since_last && self.particles.len() < s.max_particles as usize {
            self.since_last -= interval;
            let life = rng.rand_float_tol() * s.life[1] + s.life[0];
            let rotation = rng.rand_float_tol() * 180.0;
            self.particles.push(Particle {
                life,
                age: 0.0,
                pos: Vec3::ZERO,
                color: Vec3::new(c0[0], c0[1], c0[2]),
                alpha: 0.0,
                rotation,
                size: s.size[0],
                velocity: Vec3::ZERO,
                spin: 0.0,
                growth: 0.0,
            });
        }
        self.turn += dt * s.wobble_turn;
        let (wy, wx) = (self.turn * DEG).sin_cos();
        let (fade_in, fade_out) = (s.fade_in as f32 * 1e-6, s.fade_out as f32 * 1e-6);
        for p in self.particles.iter_mut().rev() {
            p.age += dt;
            let t = p.age / p.life;
            let wobble = ((((t - 0.2) * std::f32::consts::PI * 2.0).cos() * 0.5 + 0.5) * (t + 0.5)
                - 0.33)
                * s.wobble;
            let rise = (t - 0.3).max(0.0);
            let r = ((t - 0.4) * (t - 0.4) * (t - 0.4)).abs() * 3.7 + 0.05 + rise * rise * 4.0;
            let (sy, sx) = (s.spiral_turn * std::f32::consts::FRAC_1_PI * t).sin_cos();
            p.pos = Vec3::new(
                s.radius * sy * r + wy * wobble,
                s.radius * sx * r + wx * wobble,
                p.rotation.abs() / 40.0 + t * s.rise,
            );
            p.rotation += dt * 90.0;
            if 180.0 < p.rotation {
                p.rotation -= 360.0;
            }
            p.size = (r * s.size_scale).clamp(s.size[0], s.size[1]);
            p.color = Vec3::new(
                (c1[0] - c0[0]) * t + c0[0],
                (c1[1] - c0[1]) * t + c0[1],
                (c1[2] - c0[2]) * t + c0[2],
            );
            let mut fade = p.age / fade_in;
            if 1.0 < fade {
                fade = 1.0 - (fade_out - p.life + p.age) / fade_out;
            }
            p.alpha = fade.clamp(0.0, 1.0) * ((c1[3] - c0[3]) * t + c0[3]);
        }
        self.particles.retain(|p| p.age < p.life);
    }

    /// `ParticleSystemInstance::Update` by `dt` seconds.
    pub(super) fn update_standard(&mut self, s: &ParticleSystem, dt: f32, rng: &mut Random) {
        let dt2 = dt * dt;
        self.elapsed += dt;
        self.since_last += dt;
        self.rate += dt2 * s.rate_change * s.rate_scale;
        let duration = s.duration as f32 * 1e-6;
        if self.rate < 0.0 || (0.0 < duration && duration < self.elapsed) {
            self.rate = 0.0;
        }
        if self.emitting && 0.0 < self.rate {
            let interval = 1.0 / self.rate;
            while interval < self.since_last && self.particles.len() < s.max_particles as usize {
                self.since_last -= interval;
                self.spawn(s, rng);
            }
        }
        let rgba = |c: u32| {
            [
                ((c >> 16) & 0xff) as f32 / 255.0,
                ((c >> 8) & 0xff) as f32 / 255.0,
                (c & 0xff) as f32 / 255.0,
                (c >> 24) as f32 / 255.0,
            ]
        };
        let (c0, c1) = (rgba(s.colors[0]), rgba(s.colors[1]));
        let force = Vec3::from(s.force) * s.force_amount * s.force_scale * dt2;
        let span = 1.0 / (s.color_span[1] - s.color_span[0]);
        let (fade_in, fade_out) = (s.fade_in as f32 * 1e-6, s.fade_out as f32 * 1e-6);
        for p in &mut self.particles {
            p.age += dt;
            p.velocity = (force + p.velocity) * s.damping;
            p.pos += p.velocity * dt;
            p.spin = (dt2 * s.spin_change + p.spin) * s.spin_damping;
            p.rotation += p.spin * dt;
            p.growth = (dt2 * s.growth + p.growth) * s.growth_damping;
            p.size += p.growth * dt;
            let t = ((p.age / p.life - s.color_span[0]) * span).clamp(0.0, 1.0);
            p.color = Vec3::new(
                (c1[0] - c0[0]) * t + c0[0],
                (c1[1] - c0[1]) * t + c0[1],
                (c1[2] - c0[2]) * t + c0[2],
            );
            let mut fade = p.age / fade_in;
            if 1.0 < fade {
                fade = 1.0 - (fade_out - p.life + p.age) / fade_out;
            }
            p.alpha = ((c1[3] - c0[3]) * t + c0[3]) * fade.clamp(0.0, 1.0);
        }
        self.particles.retain(|p| p.age < p.life);
    }

    /// `ParticleInstance::Draw`: newest first, a quad each.
    pub(super) fn vertices(&self, out: &mut Vec<f32>) {
        for p in self.particles.iter().rev() {
            let pos = self.position + p.pos;
            let alpha = p.alpha * self.alpha;
            for (uv, corner) in [
                ([0.0, 0.0], [-1.0, 1.0]),
                ([1.0, 0.0], [1.0, 1.0]),
                ([1.0, 1.0], [1.0, -1.0]),
                ([0.0, 1.0], [-1.0, -1.0]),
            ] {
                out.extend_from_slice(&[
                    pos.x,
                    pos.y,
                    pos.z,
                    p.color.x,
                    p.color.y,
                    p.color.z,
                    alpha,
                    uv[0],
                    uv[1],
                    p.size * corner[0],
                    p.size * corner[1],
                    p.rotation * DEG,
                ]);
            }
        }
    }
}
