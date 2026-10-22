//! Animation groups: clips, events and blends.

use super::*;

/// An animation clip: the bones' transforms for each frame, and its events.
#[derive(Debug, Clone)]
pub struct AnimationClip {
    /// The clip's name, for example `Walk`.
    pub name: String,
    /// The mesh it was made for.
    pub mesh_name: String,
    /// The original animation file.
    pub source_name: String,
    /// Whether the clip loops.
    pub looping: bool,
    /// The frame rate.
    pub sample_rate: f32,
    // +0x30/+0x34
    /// Animation: source frame range for loop-point conversion.
    pub start_frame: i32,
    /// The last frame of the source range.
    pub end_frame: i32,
    // +0x38 / +0x48
    /// Declared rate and stored sample interval.
    pub declared_rate: f32,
    /// The time between stored samples.
    pub sample_interval: f32,
    // +0x40
    /// Source frame at which a looping animation resumes.
    pub loop_start_frame: i32,
    // +0x28 / +0x2c
    /// Shared animation-instance configuration.
    pub instance_count: i32,
    /// Whether all entities share one instance of the clip.
    pub instanced: bool,
    /// The frame at which a looping clip starts again.
    pub loop_start: usize,
    /// The number of bones in each frame.
    pub bone_count: usize,
    /// Frame-major quaternion (xyzw) + translation (xyz) samples.
    pub frames: Vec<Vec<crate::skeleton::Transform>>,
    // +0x58: a serialised System::Time (microseconds, Runtime Timer::Startup 0x1800cd030).
    /// The clip's length, in microseconds.
    pub duration_ticks: i64,
    /// The events of the clip.
    pub events: Vec<AnimationEvent>,
}

/// An event in a clip (for example a footstep sound), at a time.
#[derive(Debug, Clone)]
pub struct AnimationEvent {
    /// The frame at which it happens.
    pub frame: i32,
    /// The resource it plays (for example a sound effect).
    pub resource: Option<String>,
}

/// How the game blends from one clip to another.
#[derive(Debug, Clone)]
pub struct AnimationBlend {
    /// The clip that ends.
    pub source: String,
    /// The clip that begins.
    pub target: String,
    /// The blend time.
    pub seconds: f32,
}

/// A group of animation clips for one skeleton.
#[derive(Debug, Clone)]
pub struct AnimationGroup {
    /// The clips.
    pub clips: Vec<AnimationClip>,
    // +0x30; +0x38/+0x40
    /// The blend time between clips that have no blend of their own, in seconds.
    pub default_blend: f32,
    /// The blends between named clips.
    pub blends: Vec<AnimationBlend>,
    // +0x48/+0x50
    /// Precompiled [new clip][previous clip] transition matrix.
    pub blend_matrix: Vec<f32>,
}

impl AnimationGroup {
    /// The resource class name.
    pub const CLASS: &'static str = "AnimationGroup";

    /// `Graphics::AnimationGroup::Serialize` and `Graphics::Animation::Serialize` from the Ghidra
    /// Runtime export. Animation samples are 4 quaternion floats and 3 translation floats;
    /// UpdateNodes interpolates adjacent samples per mesh node.
    ///
    /// Original: Runtime `0x1800fdf30`, Runtime `0x1800fd910`.
    pub fn read(r: &mut Reader) -> Result<Self> {
        r.block(|r, _| {
            let clip_count = r.count()?;
            let mut clips = Vec::with_capacity(clip_count);
            for _ in 0..clip_count {
                let name = r.string()?;
                let mesh_name = r.string()?;
                let source_name = r.string()?;
                let start_frame = r.i32()?;
                let end_frame = r.i32()?;
                let declared_rate = r.f32()?;
                let looping = r.bool()?;
                let loop_start_frame = r.i32()?;
                let instance_count = r.i32()?;
                let instanced = r.bool()?;
                let event_count = r.count()?;
                let events = (0..event_count).map(|_| Ok(AnimationEvent {
                    frame: r.i32()?, resource: r.external_ref()?,
                })).collect::<Result<Vec<_>>>()?;
                let frame_count = r.count()?;
                let sample_rate = r.f32()?;
                let sample_interval = r.f32()?;
                let bone_count = r.count()?;
                let sample_count = r.count()?;
                if sample_count != frame_count.saturating_mul(bone_count) {
                    return Err(crate::error::Error::Format(format!(
                        "animation {name}: {sample_count} samples != {frame_count} frames x {bone_count} bones"
                    )));
                }
                let loop_start = if loop_start_frame == 0 || end_frame == start_frame {
                    0
                } else {
                    ((((loop_start_frame - start_frame) as f32
                        / (end_frame - start_frame) as f32)
                        * frame_count as f32) as usize)
                        .min(frame_count.saturating_sub(1))
                };
                let mut frames = Vec::with_capacity(frame_count);
                for _ in 0..frame_count {
                    let mut frame = Vec::with_capacity(bone_count);
                    for _ in 0..bone_count {
                        frame.push(crate::skeleton::Transform {
                            // Engine quaternions use the inverse convention to our column-vector
                            // transform math, as do the serialized mesh bind rotations.
                            rotation: {
                                let mut q = r.f32s()?;
                                q[..3].iter_mut().for_each(|v| *v = -*v);
                                q
                            },
                            translation: r.f32s()?,
                        });
                    }
                    frames.push(frame);
                }
                let duration_ticks = r.i64()?;
                clips.push(AnimationClip {
                    name,
                    mesh_name,
                    source_name,
                    looping,
                    sample_rate,
                    start_frame,
                    end_frame,
                    declared_rate,
                    sample_interval,
                    loop_start_frame,
                    instance_count,
                    instanced,
                    loop_start,
                    bone_count,
                    frames,
                    duration_ticks,
                    events,
                });
            }
            let blend_count = r.count()?;
            let blends = (0..blend_count).map(|_| Ok(AnimationBlend {
                source: r.string()?, target: r.string()?, seconds: r.f32()?,
            })).collect::<Result<Vec<_>>>()?;
            let default_blend = r.f32()?;
            let blend_matrix_count = r.count()?;
            let blend_matrix = (0..blend_matrix_count).map(|_| r.f32()).collect::<Result<Vec<_>>>()?;
            Ok(AnimationGroup { clips, default_blend, blends, blend_matrix })
        })
    }
}
