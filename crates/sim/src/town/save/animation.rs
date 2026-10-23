//! A character's animation into a save: the layers of its model's animation state, and the shared
//! instances of instanced clips.

use unbanished_formats::savegame::{Fields, GameState, Value};
use unbanished_formats::skeleton::Transform;
use unbanished_game::behaviour::animation::{shared, AnimationStack, Layer, Playback};

use super::{normalise, Refs, StartingTown};

// AnimationState entries (FUN_18005d610): group +0x00, clip +0x08, instance +0x0c, speed +0x10,
// clock +0x18, blend +0x20, blend left +0x28, updated once +0x30.
fn state(group: &Value, l: &Layer) -> Fields {
    Fields(vec![
        ("animation", group.clone()),
        ("clip", Value::I32(l.playback.clip as i32)),
        ("speed", Value::F32(l.playback.speed)),
        ("elapsed", Value::I64(l.playback.elapsed)),
        ("blend", Value::I64(l.blend)),
        ("left", Value::I64(l.left)),
        ("instance", Value::I32(l.instance.unwrap_or(-1))),
        ("started", Value::Bool(l.started)),
    ])
}

/// A model's saved animation states made the character's layers, newest first. The group is the
/// one its saved states name, else `fallback` (the reference of the group the character plays);
/// with neither, the list stays as it is.
///
/// Original: `MeshInstance::Serialize`, Runtime `0x180101f10`, with `FUN_18005d610`.
pub(super) fn write_layers(model: &mut Fields, stack: &AnimationStack, fallback: Option<u32>) {
    let group = match model.at("model.animations") {
        Some(Value::List(states)) => states.first().and_then(|s| s.get("animation")).cloned(),
        _ => None,
    };
    let had_none = group.is_none();
    let Some(group) = group.or(fallback.map(Value::Ext)) else {
        return;
    };
    let states: Vec<Fields> = stack.layers.iter().map(|l| state(&group, l)).collect();
    // A model given its first layers here (a new game's) is playing them. Inferred: the
    // original's saves have `animating` set on a model with layers (Save2: all 115, Save5:
    // 116 of 117, Save83: 376 of 381), and the few exceptions are kept as loaded.
    if had_none && !states.is_empty() {
        model.set_bool("model.animating", true);
    }
    model.set("model.animations", Value::List(states));
}

// An instance (FUN_18005cd90): its own AnimationState, a flag, the nodes' model-space transforms
// now and one update before (FUN_18005cd00), the root's movement, and the pose (FUN_18005a2f0).
// The original's rotations are the port's conjugated.
fn pose_value(pose: &[Transform]) -> Value {
    Value::List(
        pose.iter()
            .map(|t| {
                let [x, y, z, w] = t.rotation;
                Fields(vec![
                    ("rotation", Value::F32s(vec![-x, -y, -z, w])),
                    ("translation", Value::F32s(t.translation.to_vec())),
                ])
            })
            .collect(),
    )
}

fn instance_state(group: &Value, p: &Playback) -> Fields {
    Fields(vec![
        ("animation", group.clone()),
        ("clip", Value::I32(p.clip as i32)),
        ("speed", Value::F32(p.speed)),
        ("elapsed", Value::I64(p.elapsed)),
        ("blend", Value::I64(0)),
        ("left", Value::I64(0)),
        ("instance", Value::I32(-1)),
        ("started", Value::Bool(true)),
    ])
}

impl StartingTown {
    // AnimationManager entries (FUN_18005d130): group +0x00, clip +0x08, users +0x0c, the
    // instances +0x20 (FUN_18005cd90) and the nodes +0x10 (FUN_18005cbf0).
    /// The shared instances into the save's animation block, slot by slot: each entry's group,
    /// clip and users, and each instance's clock, speed, pose and root movement. A free slot is
    /// written free.
    ///
    /// Not ported: the group's nodes (its skeleton's bind transforms) and each instance's
    /// model-space transforms are not kept here. A slot keeps those it was saved with; a slot
    /// the save did not hold for this group copies them from one that it did, and writes none
    /// when there is no such slot.
    ///
    /// Original: `AnimationManager::Serialize`, Runtime `0x1800fea20` (writing).
    pub(super) fn export_animations(&self, g: &mut GameState, refs: &mut Refs) {
        let slots = shared::snapshot();
        let saved: Vec<Fields> = match g.animations.at("animations.entries") {
            Some(Value::List(entries)) => entries.clone(),
            _ => Vec::new(),
        };
        let name_of = |g: &GameState, e: &Fields| match e.get("group") {
            Some(Value::Ext(id)) => g.references.get(id).map(|n| normalise(n)),
            _ => None,
        };
        // A saved slot of each group, for the nodes and transforms the port does not keep.
        let mut model: std::collections::HashMap<String, Fields> = Default::default();
        for e in &saved {
            if let Some(n) = name_of(g, e) {
                model.entry(n).or_insert_with(|| e.clone());
            }
        }
        let mut entries = Vec::with_capacity(slots.len());
        for (i, slot) in slots.iter().enumerate() {
            let Some((name, clip, users, instances)) = slot else {
                entries.push(Fields(vec![
                    ("group", Value::Ext(u32::MAX)),
                    ("clip", Value::I32(-1)),
                    ("users", Value::I32(0)),
                    ("instances", Value::List(Vec::new())),
                    ("nodes", Value::List(Vec::new())),
                ]));
                continue;
            };
            let same = saved
                .get(i)
                .filter(|e| name_of(g, e).as_ref() == Some(name));
            let base = same.or_else(|| model.get(name));
            let group = match base.and_then(|b| b.get("group")) {
                Some(v) => v.clone(),
                None => Value::Ext(refs.id(g, name)),
            };
            let poses = shared::poses(i);
            let base_instances = match base.and_then(|b| b.get("instances")) {
                Some(Value::List(l)) => l.clone(),
                _ => Vec::new(),
            };
            let list = instances
                .iter()
                .enumerate()
                .map(|(k, p)| {
                    // An instance that has not moved since loading keeps all it was saved with.
                    let kept = same
                        .and(base_instances.get(k))
                        .filter(|b| match b.get("states") {
                            Some(Value::List(st)) => st.first().is_some_and(|st| {
                                st.i32("clip") == Some(p.clip as i32)
                                    && st.f32("speed").map(f32::to_bits) == Some(p.speed.to_bits())
                                    && st.get("elapsed") == Some(&Value::I64(p.elapsed))
                            }),
                            _ => false,
                        });
                    if let Some(b) = kept {
                        return b.clone();
                    }
                    let pose = poses.get(k).cloned().unwrap_or_default();
                    let root = pose
                        .first()
                        .map_or(vec![0.0; 3], |t| t.translation.to_vec());
                    let pairs = base_instances
                        .get(k)
                        .and_then(|b| b.get("pairs"))
                        .cloned()
                        .unwrap_or(Value::List(Vec::new()));
                    Fields(vec![
                        ("states", Value::List(vec![instance_state(&group, p)])),
                        ("flag", Value::Bool(false)),
                        ("pairs", pairs),
                        ("vector", Value::F32s(root)),
                        ("pose", pose_value(&pose)),
                    ])
                })
                .collect();
            let nodes = base
                .and_then(|b| b.get("nodes"))
                .cloned()
                .unwrap_or(Value::List(Vec::new()));
            entries.push(Fields(vec![
                ("group", group),
                ("clip", Value::I32(*clip as i32)),
                ("users", Value::I32(*users)),
                ("instances", Value::List(list)),
                ("nodes", nodes),
            ]));
        }
        if !g
            .animations
            .set("animations.entries", Value::List(entries.clone()))
        {
            g.animations = Fields(vec![(
                "animations",
                Value::Block(0, Fields(vec![("entries", Value::List(entries))])),
            )]);
        }
    }
}
