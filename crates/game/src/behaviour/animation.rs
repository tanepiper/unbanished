//! Animation as the engine plays it: sampling a pose, the clock of each playing clip, and the
//! events (such as sounds) a clip fires.
//!
//! Original: `Animation::GetPoseAtTime`, `Quaternion::Lerp`, `AnimationState::Update`,
//! `Animation::ApplyEvents`.
//!
//! Original: Runtime `0x1800fd530`, Runtime `0x18001dde0`, Runtime `0x1801003b0`, Runtime
//! `0x1800fd230`.

use crate::scheduler::{seconds, ticks};
use unbanished_formats::skeleton::Transform;
use unbanished_formats::types::AnimationClip;

/// Single, unique animation playback. Tree expiry starts from an empty animation state.
/// Blended stacks and AnimationManager's shared instances require their separate lifecycle port.
#[derive(Debug, Clone, PartialEq)]
pub struct Playback {
    /// The clip.
    pub clip: usize,
    /// The time played so far.
    pub elapsed: i64,
    /// UpdateNodes publishes the pose at the OLD clock, then Update advances elapsed.
    pub sample_time: i64,
    /// The time step of the last sample.
    pub sample_delta: i64,
    /// The playback speed.
    pub speed: f32,
}

impl Playback {
    /// A clip that starts playing at `start`, at speed 1. Tree removal starts at time zero.
    ///
    /// Original: `FUN_14012a5c0`; `AnimationState::PlayAnimation`, Runtime `0x1801007e0`.
    pub fn new(clip: usize, start: i64) -> Self {
        Self {
            clip,
            elapsed: start,
            sample_time: start,
            sample_delta: 0,
            speed: 1.0,
        }
    }

    /// Whether the clip is still playing. The end comes from the clip's stored duration (with its
    /// rounded value), not from frames and rate.
    ///
    /// Original: `MeshInstance::IsAnimating`, Runtime `0x180102060`.
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

    /// The pose of the clip at the current time.
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

/// The pose of a clip at a time: the two samples around it, interpolated (normalised quaternion
/// lerp for rotations, linear for translations), with the original's loop arithmetic. The root's
/// rotation is reset and its translation becomes a movement delta, as in the original.
///
/// Original: `Animation::GetPoseAtTime`, Runtime `0x1800fd530`.
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

// +0x20; +0x28
/// One layer of an [`AnimationStack`] (`AnimationState` entry, 0x38 bytes): its clip and clock, and
/// while it fades out under a newer layer, the blend time and what is left of it.
#[derive(Debug, Clone, PartialEq)]
pub struct Layer {
    /// The clip and its clock.
    pub playback: Playback,
    /// The blend time.
    pub blend: i64,
    /// The blend time left.
    pub left: i64,
    // +0x30
    /// The layer has been updated once.
    pub started: bool,
    // +0x0c
    /// The shared instance it follows (an instanced clip), see [`shared`].
    pub instance: Option<i32>,
}

/// `Graphics::AnimationState` with its blend stack, newest layer first. Each play pushes a
/// layer; the one it covers fades out over the group's blend time, and the pose mixes them.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AnimationStack {
    /// The layers, newest first.
    pub layers: Vec<Layer>,
    /// The pose `UpdateNodes` published at the last update.
    pose: Vec<Transform>,
}

/// `AnimationGroup` `+0x48`: the blend time from `old` to `new` (`[new][old]`, the group's
/// clips square), else the default (`+0x30`).
fn blend_seconds(group: &unbanished_formats::types::AnimationGroup, old: usize, new: usize) -> f32 {
    let n = group.clips.len();
    group
        .blend_matrix
        .get(n * new + old)
        .copied()
        .unwrap_or(group.default_blend)
}

impl AnimationStack {
    /// One layer playing `clip` from `start`.
    pub fn new(clip: usize, start: i64) -> Self {
        Self {
            layers: vec![Layer {
                playback: Playback::new(clip, start),
                blend: 0,
                left: 0,
                started: false,
                instance: None,
            }],
            pose: Vec::new(),
        }
    }

    /// The newest layer's clip.
    pub fn clip(&self) -> Option<usize> {
        self.layers.first().map(|l| l.playback.clip)
    }

    /// `AnimationState::PlayAnimation`. A looping clip already on top keeps its time (speed back to
    /// 1). Otherwise the top layer starts fading over the blend time from its clip to `clip` and
    /// the new layer goes on top; a top layer never updated is replaced instead, the one below
    /// taking the blend.
    ///
    /// Original: Runtime `0x1801007e0`.
    pub fn play(
        &mut self,
        group: &unbanished_formats::types::AnimationGroup,
        clip: usize,
        start: i64,
    ) {
        if let Some(top) = self.layers.first_mut() {
            if top.playback.clip == clip && group.clips.get(clip).is_some_and(|c| c.looping) {
                top.playback.speed = 1.0;
                return;
            }
        }
        // `MeshInstance::PlayAnimation` asks for a shared instance (`GetInstanceId`) when the
        // clip is instanced.
        let instance = group
            .clips
            .get(clip)
            .filter(|c| c.instanced && c.instance_count > 0)
            .map(|_| shared::instance_id(group, clip));
        let layer = Layer {
            playback: Playback::new(clip, start),
            blend: 0,
            left: 0,
            started: false,
            instance,
        };
        if self.layers.is_empty() {
            self.layers.push(layer);
            return;
        }
        if !self.layers[0].started && self.layers.len() != 1 {
            let t = ticks(blend_seconds(group, self.layers[1].playback.clip, clip));
            self.layers[1].blend = t;
            self.layers[1].left = t;
            self.layers[0] = layer;
            return;
        }
        let t = ticks(blend_seconds(group, self.layers[0].playback.clip, clip));
        self.layers[0].blend = t;
        self.layers[0].left = t;
        self.layers.insert(0, layer);
    }

    /// `AnimationState::Update` with `UpdateNodes`: the pose at the old clocks, the oldest layer's
    /// first and each newer one mixed over it by the older layer's share left (`Quaternion::Lerp`
    /// and a linear translation); then every clock advances (only the newest fires its events), and
    /// the fading layers count down, a layer leaving once less than a tick is left. Returns the
    /// newest layer's events.
    ///
    /// Original: Runtime `0x1801003b0`, Runtime `0x1800ff4b0`.
    pub fn update(
        &mut self,
        group: &unbanished_formats::types::AnimationGroup,
        dt: i64,
    ) -> Vec<usize> {
        let Some(top) = self.layers.first_mut() else {
            return Vec::new();
        };
        top.started = true;
        let weights: Vec<f32> = self
            .layers
            .iter()
            .map(|l| {
                if l.blend > 0 {
                    seconds(l.left) / seconds(l.blend)
                } else {
                    0.0
                }
            })
            .collect();
        let mut events = Vec::new();
        for (i, layer) in self.layers.iter_mut().enumerate() {
            if let Some(clip) = group.clips.get(layer.playback.clip) {
                let fired = layer.playback.update(clip, dt);
                if i == 0 {
                    events = fired;
                }
            }
        }
        // `UpdateNodes`: an instanced layer takes its shared instance's pose
        // (`AnimationManager::GetPose`), the others their own clocks'.
        let poses: Vec<Option<Vec<Transform>>> = self
            .layers
            .iter()
            .map(|l| match l.instance {
                Some(id) => shared::pose(id),
                None => group.clips.get(l.playback.clip).map(|c| l.playback.pose(c)),
            })
            .collect();
        let mut pose: Option<Vec<Transform>> = None;
        for i in (0..self.layers.len()).rev() {
            let Some(new) = &poses[i] else { continue };
            pose = Some(match pose {
                None => new.clone(),
                Some(old) => {
                    let w = weights.get(i + 1).copied().unwrap_or(0.0);
                    new.iter()
                        .zip(&old)
                        .map(|(n, o)| Transform {
                            rotation: quaternion_lerp(n.rotation, o.rotation, w),
                            translation: std::array::from_fn(|k| {
                                n.translation[k] + (o.translation[k] - n.translation[k]) * w
                            }),
                        })
                        .collect()
                }
            });
        }
        self.pose = pose.unwrap_or_default();
        let step = ticks(seconds(dt));
        let mut i = 0;
        while i < self.layers.len() {
            let l = &mut self.layers[i];
            if l.blend > 0 {
                l.left -= step;
                if l.left < 1 {
                    if let Some(id) = l.instance {
                        shared::free(id);
                    }
                    self.layers.remove(i);
                    continue;
                }
            }
            i += 1;
        }
        events
    }

    /// The pose of the last update, or the newest layer's at its clock before any update.
    pub fn pose(
        &self,
        group: &unbanished_formats::types::AnimationGroup,
    ) -> Option<Vec<Transform>> {
        if !self.pose.is_empty() {
            return Some(self.pose.clone());
        }
        let top = self.layers.first()?;
        group
            .clips
            .get(top.playback.clip)
            .map(|c| top.playback.pose(c))
    }
}

// +0x2c; +0x28
/// `Graphics::AnimationManager` (Runtime): the shared instances of instanced clips. Each clip a
/// character plays with its flag set is followed through one of shared clocks (3, 4 or 8 for the
/// citizens' and animals'), so the characters on one walk or work in step.
///
/// - `GetInstanceId`: the first request for a group's clip makes its instances, the `k`-th starting
///   `k × duration / count` into the clip (and updated twice at no time); each request counts one
///   more user and gets the instance whose clock is earliest.
/// - `Update`: every instance advances, before the characters; its pose's root is the identity
///   turned and its movement.
/// - `FreeInstanceId`: the last user gone, the clip's instances go.
///
/// The original keeps one manager for the process; the port one per thread, reset with each town
/// ([`shared::reset`]).
///
/// Original: Runtime `0x1800fe260`, Runtime `0x1800fead0`, Runtime `0x1800fe200`.
pub mod shared {
    use super::Playback;
    use std::cell::RefCell;
    use unbanished_formats::skeleton::Transform;
    use unbanished_formats::types::{AnimationClip, AnimationGroup};

    struct Entry {
        group: usize,
        clip: usize,
        users: i32,
        data: AnimationClip,
        instances: Vec<(Playback, Vec<Transform>)>,
    }

    thread_local! {
        static ENTRIES: RefCell<Vec<Option<Entry>>> = const { RefCell::new(Vec::new()) };
    }

    /// A new town: no shared instances.
    pub fn reset() {
        ENTRIES.with(|e| e.borrow_mut().clear());
    }

    /// One town's shared instances, kept aside while another town on the same thread runs (a
    /// lockstep test runs several towns in one process; on separate computers each has its own).
    #[derive(Default)]
    pub struct Table(Vec<Option<Entry>>);

    /// Puts `table` in place of this thread's instances, and the thread's in `table`.
    pub fn swap(table: &mut Table) {
        ENTRIES.with(|e| std::mem::swap(&mut *e.borrow_mut(), &mut table.0));
    }

    /// `GetInstanceId`: `entry << 8 | instance`.
    pub fn instance_id(group: &AnimationGroup, clip: usize) -> i32 {
        let key = group as *const AnimationGroup as usize;
        ENTRIES.with(|e| {
            let mut entries = e.borrow_mut();
            let found = entries
                .iter()
                .position(|x| x.as_ref().is_some_and(|x| x.group == key && x.clip == clip));
            let index = match found {
                Some(i) => i,
                None => {
                    let data = group.clips[clip].clone();
                    let count = data.instance_count.max(1) as usize;
                    let step = super::seconds(data.duration_ticks) * (1.0 / count as f32);
                    let mut at = 0.0f32;
                    let mut instances = Vec::with_capacity(count);
                    for _ in 0..count {
                        let mut p = Playback::new(clip, super::ticks(at));
                        for _ in 0..2 {
                            p.update(&data, 0);
                        }
                        let pose = p.pose(&data);
                        instances.push((p, pose));
                        at += step;
                    }
                    let entry = Entry {
                        group: key,
                        clip,
                        users: 0,
                        data,
                        instances,
                    };
                    match entries.iter().position(Option::is_none) {
                        Some(free) => {
                            entries[free] = Some(entry);
                            free
                        }
                        None => {
                            entries.push(Some(entry));
                            entries.len() - 1
                        }
                    }
                }
            };
            let entry = entries[index].as_mut().unwrap();
            entry.users += 1;
            let mut best = (f32::MAX, 0usize);
            for (k, (p, _)) in entry.instances.iter().enumerate() {
                let t = super::seconds(p.elapsed);
                if t < best.0 {
                    best = (t, k);
                }
            }
            ((index as i32) << 8) | best.1 as i32
        })
    }

    /// `FreeInstanceId`.
    pub fn free(id: i32) {
        ENTRIES.with(|e| {
            let mut entries = e.borrow_mut();
            let i = (id >> 8 & 0xff) as usize;
            if let Some(Some(entry)) = entries.get_mut(i) {
                entry.users -= 1;
                if entry.users == 0 {
                    entries[i] = None;
                }
            }
        });
    }

    /// `Update`: every instance by `dt`.
    pub fn update(dt: i64) {
        ENTRIES.with(|e| {
            for entry in e.borrow_mut().iter_mut().flatten() {
                for (p, pose) in &mut entry.instances {
                    p.update(&entry.data, dt);
                    *pose = p.pose(&entry.data);
                }
            }
        });
    }

    /// `GetPose`.
    pub fn pose(id: i32) -> Option<Vec<Transform>> {
        ENTRIES.with(|e| {
            let entries = e.borrow();
            let entry = entries.get((id >> 8 & 0xff) as usize)?.as_ref()?;
            entry
                .instances
                .get((id & 0xff) as usize)
                .map(|(_, pose)| pose.clone())
        })
    }
}
