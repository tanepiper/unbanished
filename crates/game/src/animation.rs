//! Original animation sampling, unique playback clocks and event intervals. Runtime addresses:
//! Animation::GetPoseAtTime 0x1800fd530, Quaternion::Lerp 0x18001dde0,
//! AnimationState::Update 0x1801003b0 and Animation::ApplyEvents 0x1800fd230.

use crate::scheduler::{seconds, ticks};
use banished_formats::skeleton::Transform;
use banished_formats::types::AnimationClip;

/// Single, unique animation playback. Tree expiry starts from an empty animation state.
/// Blended stacks and AnimationManager's shared instances require their separate lifecycle port.
#[derive(Debug, Clone, PartialEq)]
pub struct Playback {
    pub clip: usize,
    pub elapsed: i64,
    /// UpdateNodes publishes the pose at the OLD clock, then Update advances elapsed.
    pub sample_time: i64,
    pub sample_delta: i64,
    pub speed: f32,
}

impl Playback {
    /// Runtime AnimationState::PlayAnimation 0x1801007e0 initializes a unique state at its
    /// supplied start time and speed 1. Tree removal (FUN_14012a5c0) supplies start time zero.
    pub fn new(clip: usize, start: i64) -> Self {
        Self {
            clip,
            elapsed: start,
            sample_time: start,
            sample_delta: 0,
            speed: 1.0,
        }
    }

    /// Port of MeshInstance::IsAnimating 0x180102060. Completion uses serialized duration,
    /// including its rounded tick value, rather than recomputing frames/rate.
    pub fn is_animating(&self, clip: &AnimationClip) -> bool {
        clip.looping || self.elapsed < clip.duration_ticks
    }

    /// Port of AnimationState::Update's unique-state clock and ApplyEvents interval. Returns
    /// indices of callbacks in resource order; effect dispatch remains the caller's responsibility.
    pub fn update(&mut self, clip: &AnimationClip, dt: i64) -> Vec<usize> {
        self.sample_time = self.elapsed;
        self.sample_delta = ticks(seconds(dt) * self.speed);
        let delta = if self.speed == 1.0 {
            dt
        } else {
            self.sample_delta
        };
        let next = self.elapsed + delta;
        let events = clip
            .events
            .iter()
            .enumerate()
            .filter_map(|(index, event)| {
                let time = ticks(
                    (event.frame - clip.start_frame) as f32
                        / (clip.end_frame - clip.start_frame) as f32
                        * clip.frames.len() as f32
                        * clip.sample_interval,
                );
                let crossed = self.elapsed <= time && time < next;
                let wrapped = next >= clip.duration_ticks
                    && self.elapsed - clip.duration_ticks <= time
                    && time < next - clip.duration_ticks;
                (crossed || wrapped).then_some(index)
            })
            .collect();
        self.elapsed = next;
        if !clip.looping {
            if self.elapsed >= clip.duration_ticks {
                self.elapsed = clip.duration_ticks;
            }
        } else if clip.duration_ticks > 0 && self.elapsed >= clip.duration_ticks {
            // Original repeatedly subtracts the full duration, then adds the source loop point.
            self.elapsed %= clip.duration_ticks;
            let loop_seconds = if clip.loop_start_frame == 0 {
                0.0
            } else {
                (clip.loop_start_frame - clip.start_frame) as f32
                    / (clip.end_frame - clip.start_frame) as f32
                    * clip.frames.len() as f32
                    * clip.sample_interval
            };
            self.elapsed += ticks(loop_seconds);
        }
        events
    }

    pub fn pose(&self, clip: &AnimationClip) -> Vec<Transform> {
        sample_pose(clip, self.sample_time, self.sample_delta)
    }
}

/// Port of Math::Quaternion::Lerp 0x18001dde0, including its dot <= 0 sign choice and y/x/z/w
/// normalization sum. Input/output use the format reader's conjugated quaternion convention.
fn quaternion_lerp(a: [f32; 4], b: [f32; 4], fraction: f32) -> [f32; 4] {
    // Perform the arithmetic in the native convention before conjugating its result. Flipping
    // signs before summation changes signed-zero results even when the rotations are equivalent.
    let native = |mut q: [f32; 4]| {
        for value in &mut q[..3] {
            *value = -*value;
        }
        q
    };
    let a = native(a);
    let b = native(b);
    let old = 1.0 - fraction;
    let dot = a[0] * b[0] + a[1] * b[1] + a[2] * b[2] + a[3] * b[3];
    let new = if dot <= 0.0 { -fraction } else { fraction };
    let q = std::array::from_fn::<_, 4, _>(|i| a[i] * old + b[i] * new);
    let length = (q[1] * q[1] + q[0] * q[0] + q[2] * q[2] + q[3] * q[3]).sqrt();
    let inverse = 1.0 / length;
    native(q.map(|v| v * inverse))
}

/// Port of Animation::GetPoseAtTime 0x1800fd530: adjacent sample indices, original loop
/// arithmetic, normalized quaternion lerp and linear translation interpolation. The root
/// rotation is reset and root translation becomes a movement delta, as in the original.
pub fn sample_pose(clip: &AnimationClip, time: i64, delta: i64) -> Vec<Transform> {
    let count = clip.frames.len();
    if count == 0 {
        return Vec::new();
    }
    let position = seconds(time) * clip.sample_rate;
    let raw = position as i32;
    let loop_start = if clip.loop_start_frame == 0 {
        0
    } else {
        ((clip.loop_start_frame - clip.start_frame) as f32
            / (clip.end_frame - clip.start_frame) as f32
            * count as f32) as i32
    };
    // Deviation: malformed negative times and empty loop spans must not index outside resources.
    let span = (count as i32 - loop_start).max(1);
    let first = if raw < count as i32 {
        raw.max(0)
    } else if clip.looping {
        loop_start + raw % span
    } else {
        count as i32 - 1
    } as usize;
    let second = if first + 1 < count {
        first + 1
    } else if clip.looping {
        (first as i32 + 1) as usize % span as usize + loop_start as usize
    } else {
        count - 1
    };
    let fraction = position - raw as f32;
    let mut pose = if first == second || fraction == 0.0 {
        clip.frames[first].clone()
    } else {
        clip.frames[first]
            .iter()
            .zip(&clip.frames[second])
            .map(|(a, b)| Transform {
                rotation: quaternion_lerp(a.rotation, b.rotation, fraction),
                translation: std::array::from_fn(|i| {
                    a.translation[i] + (b.translation[i] - a.translation[i]) * fraction
                }),
            })
            .collect()
    };
    if let Some(root) = pose.first_mut() {
        root.rotation = Transform::IDENTITY.rotation;
        let scale = seconds(delta) * clip.sample_rate;
        root.translation = root.translation.map(|v| v * scale);
    }
    pose
}
