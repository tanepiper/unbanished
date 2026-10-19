//! Particle systems, including the tornado's.

use super::*;

/// `Graphics::ParticleSystem` (Runtime `ParticleSystem::Serialize` `0x1800a5c70`, after the
/// `Graphics::Particle` block `0x180106790`): an emitter of camera-facing quads. Times are
/// `System::Time` microseconds. See docs/PARTICLES.md.
#[derive(Debug, Clone, PartialEq)]
pub struct ParticleSystem {
    /// Particle `+0x20`, `+0x24`: instanced, the shared instance count.
    pub instanced: bool,
    pub instances: u32,
    /// Particle `+0x28`: the most particles alive.
    pub max_particles: u32,
    /// Particle `+0x30`: the MaterialInstance.
    pub material: Option<String>,
    /// `+0x48`: particles per second; `+0x4c × +0x50`: its change per second squared.
    pub rate: f32,
    pub rate_change: f32,
    pub rate_scale: f32,
    /// `+0x58`: how long it emits (0 for ever).
    pub duration: i64,
    /// `+0x60`: the emission direction (model space).
    pub direction: [f32; 3],
    /// `+0x78`, `+0x6c`: the spawn box: each axis `± axis × size / 2`.
    pub spawn_size: f32,
    pub spawn_box: [f32; 3],
    /// `+0x7c`, `+0x80`: the emission cone's angles (degrees).
    pub cone: [f32; 2],
    /// `+0x84`, `+0x88`: speed (base, ± range).
    pub speed: [f32; 2],
    /// `+0x8c`: force scale; `+0x90`: velocity kept per update.
    pub force_scale: f32,
    pub damping: f32,
    /// `+0x94`, `+0x98`: rotation (degrees, base ± range).
    pub rotation: [f32; 2],
    /// `+0x9c`, `+0xa0`: spin (degrees/s, base ± range, either way).
    pub spin: [f32; 2],
    /// `+0xa4`, `+0xa8`: spin change and kept part.
    pub spin_change: f32,
    pub spin_damping: f32,
    /// `+0xac`, `+0xb0`: life (seconds, base ± range).
    pub life: [f32; 2],
    /// `+0xb4`, `+0xb8`: the force (wind) and its scale.
    pub force_amount: f32,
    pub force: [f32; 3],
    /// `+0xc4`, `+0xc8`: size (base ± range); `+0xcc`, `+0xd0`: growth and its kept part.
    pub size: [f32; 2],
    pub growth: f32,
    pub growth_damping: f32,
    /// `+0xdc`, `+0xe0`: start and end colours (BGRA bytes).
    pub colors: [u32; 2],
    /// `+0xd4`, `+0xd8`: the part of life the colour moves over.
    pub color_span: [f32; 2],
    /// `+0xe8`, `+0xf0`: fade in and fade out.
    pub fade_in: i64,
    pub fade_out: i64,
}

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

/// The Tornado particle class (`Graphics::Particle` subclass, serializer `FUN_1400d76e0`,
/// update `FUN_1400d7940`): particles spiralling up a funnel. Colours are BGRA bytes, times
/// `System::Time` microseconds. See docs/PARTICLES.md.
#[derive(Debug, Clone, PartialEq)]
pub struct TornadoParticle {
    /// Particle `+0x28`, `+0x30`: the most particles alive and the MaterialInstance.
    pub max_particles: u32,
    pub material: Option<String>,
    /// `+0x48`: the spiral's radius scale; `+0x4c`: the rise over a life; `+0x50`: the
    /// wobble's radius.
    pub radius: f32,
    pub rise: f32,
    pub wobble: f32,
    /// `+0x54`: the wobble's turn (degrees a second); `+0x58`: the spiral's turn over a life.
    pub wobble_turn: f32,
    pub spiral_turn: f32,
    /// `+0x5c`: particles a second; `+0x60`, `+0x64`: life (seconds, base ± range).
    pub rate: f32,
    pub life: [f32; 2],
    /// `+0x68`, `+0x6c`: the least and most size; `+0x70`: size per radius.
    pub size: [f32; 2],
    pub size_scale: f32,
    /// `+0x74`, `+0x78`: the start and end colours.
    pub colors: [u32; 2],
    /// `+0x80`, `+0x88`: fade in and fade out.
    pub fade_in: i64,
    pub fade_out: i64,
}

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
    /// `+0x00`: the ParticleSystem.
    pub system: Option<String>,
    /// `+0x08`: the model node it sits on (empty: the model's root).
    pub node: String,
    /// `+0x10..+0x13`: `+0x11` starts on activation, `+0x12` follows entity events 2 (start)
    /// and 3 (fade out).
    pub flags: [bool; 4],
    /// `+0x20`: the alpha's fade in and out; `+0x18`: how long the system runs before it is
    /// shown (`System::Time`).
    pub fade: i64,
    pub prewarm: i64,
}

/// `ParticleDescription` (serializer `FUN_140053dc0`): the entity's particle systems, then
/// `+0x20`.
#[derive(Debug, Clone, PartialEq)]
pub struct ParticleDescription {
    pub slots: Vec<ParticleSlot>,
    pub extra: Option<String>,
}

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
