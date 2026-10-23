//! `Graphics::AnimationManager` (Runtime): the shared instances of instanced clips. Each clip a
//! character plays with its flag set is followed through one of shared clocks (3, 4 or 8 for the
//! citizens' and animals'), so the characters on one walk or work in step.
//!
//! - `GetInstanceId`: the first request for a group's clip makes its instances, the `k`-th starting
//!   `k × duration / count` into the clip (and updated twice at no time); each request counts one
//!   more user and gets the instance whose clock is earliest.
//! - `Update`: every instance advances, before the characters; its pose's root is the identity
//!   turned and its movement.
//! - `FreeInstanceId`: the last user gone, the clip's instances go.
//!
//! The original keeps one manager for the process; the port one per thread, reset with each town
//! ([`shared::reset`]).
//!
//! Original: Runtime `0x1800fe260`, Runtime `0x1800fead0`, Runtime `0x1800fe200`.

use super::Playback;
use std::cell::RefCell;
use unbanished_formats::skeleton::Transform;
use unbanished_formats::types::{AnimationClip, AnimationGroup};

struct Entry {
    group: String,
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

// Each instance's clock and speed come from its own AnimationState (FUN_18005cd90); the pose is
// sampled again here and replaced by the next Update before any character reads it.
/// One shared entry as a save holds it: the group and clip, the number of users, and each
/// instance's clock and speed.
pub struct Saved<'g> {
    /// The animation group.
    pub group: &'g AnimationGroup,
    /// The clip.
    pub clip: usize,
    /// The users the original counted (characters that left without freeing stay counted).
    pub users: i32,
    /// Each instance's clock and speed.
    pub instances: Vec<(i64, f32)>,
}

/// The shared instances of a saved game, slot by slot (`None` for a free slot), so that every
/// saved instance id names the same entry and instance again.
///
/// Original: `AnimationManager::Serialize`, Runtime `0x1800fea20` (reading).
pub fn restore(slots: Vec<Option<Saved>>) {
    let entries = slots
        .into_iter()
        .map(|slot| {
            let s = slot?;
            let data = s.group.clips.get(s.clip)?.clone();
            let instances = s
                .instances
                .into_iter()
                .map(|(elapsed, speed)| {
                    let mut p = Playback::new(s.clip, elapsed);
                    p.speed = speed;
                    let pose = p.pose(&data);
                    (p, pose)
                })
                .collect();
            Some(Entry {
                group: key(s.group),
                clip: s.clip,
                users: s.users,
                data,
                instances,
            })
        })
        .collect();
    ENTRIES.with(|e| *e.borrow_mut() = entries);
}

/// The shared instances slot by slot (`None` for a free slot): each entry's group (its resource,
/// the key requests match), clip, users and its instances' clocks.
pub fn snapshot() -> Vec<Option<(String, usize, i32, Vec<Playback>)>> {
    ENTRIES.with(|e| {
        e.borrow()
            .iter()
            .map(|x| {
                x.as_ref().map(|x| {
                    (
                        x.group.clone(),
                        x.clip,
                        x.users,
                        x.instances.iter().map(|(p, _)| p.clone()).collect(),
                    )
                })
            })
            .collect()
    })
}

/// The group's key: its resource, as the original's one object per resource; a group with no name
/// (a test's) by its address.
fn key(group: &AnimationGroup) -> String {
    if group.name.is_empty() {
        format!("@{:p}", group)
    } else {
        group.name.clone()
    }
}

/// `GetInstanceId`: `entry << 8 | instance`.
pub fn instance_id(group: &AnimationGroup, clip: usize) -> i32 {
    let key = key(group);
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
                    group: key.clone(),
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

/// The poses of one slot's instances (none for a free slot).
pub fn poses(slot: usize) -> Vec<Vec<Transform>> {
    ENTRIES.with(|e| match e.borrow().get(slot) {
        Some(Some(entry)) => entry.instances.iter().map(|(_, p)| p.clone()).collect(),
        _ => Vec::new(),
    })
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
