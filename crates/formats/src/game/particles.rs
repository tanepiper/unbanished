//! Particle systems, including the tornado's.

use super::*;

/// A particle system: an emitter of quads that face the camera (smoke, fire, dust, snow). Times are
/// in microseconds. See docs/PARTICLES.md.
///
/// Original: `Graphics::ParticleSystem::Serialize`, Runtime `0x1800a5c70`, after the
/// `Graphics::Particle` block.
///
/// Original: Runtime `0x180106790`.
#[derive(Debug, Clone, PartialEq)]
pub struct ParticleSystem {
    // +0x20; +0x24
    /// Particle: whether it is instanced.
    pub instanced: bool,
    /// Particle: the shared instance count.
    pub instances: u32,
    // +0x28
    /// Particle: the most particles alive.
    pub max_particles: u32,
    // +0x30
    /// Particle: the MaterialInstance.
    pub material: Option<String>,
    // +0x48; +0x4c × +0x50
    /// Particles per second.
    pub rate: f32,
    /// The change of the rate per second squared, multiplied by `rate_scale`.
    pub rate_change: f32,
    /// The factor of `rate_change`.
    pub rate_scale: f32,
    // +0x58
    /// How long it emits (0 for ever).
    pub duration: i64,
    // +0x60
    /// The emission direction (model space).
    pub direction: [f32; 3],
    // +0x78, +0x6c
    /// The spawn box's size: each axis spans `± axis × size / 2`.
    pub spawn_size: f32,
    /// The spawn box's axes.
    pub spawn_box: [f32; 3],
    // +0x7c, +0x80
    /// The emission cone's angles (degrees).
    pub cone: [f32; 2],
    // +0x84, +0x88
    /// Speed (base, ± range).
    pub speed: [f32; 2],
    // +0x8c; +0x90
    /// How strongly the force acts.
    pub force_scale: f32,
    /// The share of the velocity kept at each update.
    pub damping: f32,
    // +0x94, +0x98
    /// Rotation (degrees, base ± range).
    pub rotation: [f32; 2],
    // +0x9c, +0xa0
    /// Spin (degrees/s, base ± range, either way).
    pub spin: [f32; 2],
    // +0xa4, +0xa8
    /// The change of the spin.
    pub spin_change: f32,
    /// The share of the spin kept at each update.
    pub spin_damping: f32,
    // +0xac, +0xb0
    /// Life (seconds, base ± range).
    pub life: [f32; 2],
    // +0xb4, +0xb8
    /// The strength of the force.
    pub force_amount: f32,
    /// The force's direction (for example the wind).
    pub force: [f32; 3],
    // +0xc4, +0xc8; +0xcc; +0xd0
    /// The size (base ± range).
    pub size: [f32; 2],
    /// How fast the size grows.
    pub growth: f32,
    /// The share of the growth kept at each update.
    pub growth_damping: f32,
    // +0xdc, +0xe0
    /// Start and end colours (BGRA bytes).
    pub colors: [u32; 2],
    // +0xd4, +0xd8
    /// The part of life the colour moves over.
    pub color_span: [f32; 2],
    // +0xe8, +0xf0
    /// How long it fades in.
    pub fade_in: i64,
    /// How long it fades out.
    pub fade_out: i64,
}

/// Read a [`ParticleSystem`] from the bytes of its resource.
pub fn particle_system(data: &[u8]) -> Result<ParticleSystem> {
    let (_, value) = crs::parse(data, |r: &mut Reader, h| {
        if h.type_id != type_id("ParticleSystem") {
            return format_err(format!(
                "expected ParticleSystem, found {:#010x}",
                h.type_id
            ));
        }
        let (instanced, instances, max_particles, material) =
            r.block(|r, _| Ok((r.bool()?, r.u32()?, r.u32()?, r.external_ref()?)))?;
        r.block(|r, _| {
            let rate = r.f32()?;
            let rate_change = r.f32()?;
            let rate_scale = r.f32()?;
            let duration = r.i64()?;
            let direction = r.f32s::<3>()?;
            let spawn_size = r.f32()?;
            let spawn_box = r.f32s::<3>()?;
            let f = r.f32s::<15>()?;
            let force = r.f32s::<3>()?;
            let g = r.f32s::<4>()?;
            let colors = [r.u32()?, r.u32()?];
            let color_span = r.f32s::<2>()?;
            Ok(ParticleSystem {
                instanced,
                instances,
                max_particles,
                material: material.clone(),
                rate,
                rate_change,
                rate_scale,
                duration,
                direction,
                spawn_size,
                spawn_box,
                cone: [f[0], f[1]],
                speed: [f[2], f[3]],
                force_scale: f[4],
                damping: f[5],
                rotation: [f[6], f[7]],
                spin: [f[8], f[9]],
                spin_change: f[10],
                spin_damping: f[11],
                life: [f[12], f[13]],
                force_amount: f[14],
                force,
                size: [g[0], g[1]],
                growth: g[2],
                growth_damping: g[3],
                colors,
                color_span,
                fade_in: r.i64()?,
                fade_out: r.i64()?,
            })
        })
    })?;
    Ok(value)
}

/// The Tornado particle class (`Graphics::Particle` subclass): particles spiralling up a funnel.
/// Colours are BGRA bytes, times `System::Time` microseconds. See docs/PARTICLES.md.
///
/// Original: `FUN_1400d76e0`, `FUN_1400d7940`.
#[derive(Debug, Clone, PartialEq)]
pub struct TornadoParticle {
    // +0x28; +0x30
    /// The most particles alive at once.
    pub max_particles: u32,
    /// The material the particles are drawn with.
    pub material: Option<String>,
    // +0x48; +0x4c; +0x50
    /// The scale of the spiral's radius.
    pub radius: f32,
    /// The rise over a particle's life.
    pub rise: f32,
    /// The wobble's radius.
    pub wobble: f32,
    // +0x54; +0x58
    /// The wobble's turn, in degrees a second.
    pub wobble_turn: f32,
    /// The spiral's turn over a particle's life.
    pub spiral_turn: f32,
    // +0x5c; +0x60; +0x64
    /// Particles made per second.
    pub rate: f32,
    /// A particle's life in seconds (base ± range).
    pub life: [f32; 2],
    // +0x68, +0x6c; +0x70
    /// The smallest and largest size.
    pub size: [f32; 2],
    /// The size per unit of radius.
    pub size_scale: f32,
    // +0x74, +0x78
    /// The start and end colours.
    pub colors: [u32; 2],
    // +0x80, +0x88
    /// How long it fades in.
    pub fade_in: i64,
    /// How long it fades out.
    pub fade_out: i64,
}

/// Read a [`TornadoParticle`] from the bytes of its resource.
pub fn tornado_particle(data: &[u8]) -> Result<TornadoParticle> {
    let (_, value) = crs::parse(data, |r: &mut Reader, h| {
        if h.type_id != type_id("Tornado") {
            return format_err(format!("expected Tornado, found {:#010x}", h.type_id));
        }
        let (_, _, max_particles, material) =
            r.block(|r, _| Ok((r.bool()?, r.u32()?, r.u32()?, r.external_ref()?)))?;
        r.block(|r, _| {
            let f = r.f32s::<9>()?;
            let colors = [r.u32()?, r.u32()?];
            let fade_in = r.i64()?;
            let fade_out = r.i64()?;
            let wobble_turn = r.f32()?;
            let size_scale = r.f32()?;
            Ok(TornadoParticle {
                max_particles,
                material: material.clone(),
                radius: f[0],
                rise: f[1],
                wobble: f[2],
                wobble_turn,
                spiral_turn: f[3],
                rate: f[4],
                life: [f[5], f[6]],
                size: [f[7], f[8]],
                size_scale,
                colors,
                fade_in,
                fade_out,
            })
        })
    })?;
    Ok(value)
}

/// One of a `ParticleDescription`'s systems (`0x28` bytes).
#[derive(Debug, Clone, PartialEq)]
pub struct ParticleSlot {
    // +0x00
    /// The ParticleSystem.
    pub system: Option<String>,
    // +0x08
    /// The model node it sits on (empty: the model's root).
    pub node: String,
    // +0x10..+0x13; +0x11; +0x12
    /// Flags: the system starts when the entity is activated, and follows entity events 2 (start)
    /// and 3 (fade out).
    pub flags: [bool; 4],
    // +0x20; +0x18
    /// How long the alpha fades in and out, in microseconds.
    pub fade: i64,
    /// How long the system runs before it is shown, in microseconds.
    pub prewarm: i64,
}

// +0x20
/// `ParticleDescription`: the entity's particle systems, then one more reference.
///
/// Original: `FUN_140053dc0`.
#[derive(Debug, Clone, PartialEq)]
pub struct ParticleDescription {
    /// The particle systems.
    pub slots: Vec<ParticleSlot>,
    /// One more reference; not identified yet.
    pub extra: Option<String>,
}

/// Read a [`ParticleDescription`] from the bytes of its resource.
pub fn particle_description(data: &[u8]) -> Result<ParticleDescription> {
    let (_, value) = crs::parse(data, |r: &mut Reader, h| {
        if h.type_id != type_id("ParticleDescription") {
            return format_err(format!(
                "expected ParticleDescription, found {:#010x}",
                h.type_id
            ));
        }
        r.block(|r, _| {
            let slots = (0..r.count()?)
                .map(|_| {
                    Ok(ParticleSlot {
                        system: r.external_ref()?,
                        node: r.string()?,
                        flags: [r.bool()?, r.bool()?, r.bool()?, r.bool()?],
                        fade: r.i64()?,
                        prewarm: r.i64()?,
                    })
                })
                .collect::<Result<_>>()?;
            Ok(ParticleDescription {
                slots,
                extra: r.external_ref()?,
            })
        })
    })?;
    Ok(value)
}
