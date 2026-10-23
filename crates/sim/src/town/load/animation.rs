//! A character's animation as a save holds it: the layers of its model's animation state, and the
//! shared instances that instanced clips follow.

use unbanished_formats::savegame::{Fields, Value};
use unbanished_game::behaviour::animation::{AnimationStack, Layer, Playback};

use crate::saved::SavedGame;

fn time(f: &Fields, name: &str) -> i64 {
    match f.get(name) {
        Some(Value::I64(t)) => *t,
        _ => 0,
    }
}

// AnimationState entries (FUN_18005d610): clip +0x08, instance +0x0c, speed
// +0x10, clock +0x18, blend +0x20, blend left +0x28, updated once +0x30.
/// The animation layers of a saved ModelComponent, newest first, or `None` when it saved none (the
/// character then starts from its constructor's state, as before).
///
/// Original: `MeshInstance::Serialize`, Runtime `0x180101f10`, with `FUN_18005d610`.
pub(in crate::town) fn saved_layers(model: &Fields) -> Option<AnimationStack> {
    let Some(Value::List(states)) = model.at("model.animations") else {
        return None;
    };
    let layers: Vec<Layer> = states
        .iter()
        .filter_map(|s| {
            let clip = usize::try_from(s.i32("clip")?).ok()?;
            let elapsed = time(s, "elapsed");
            let instance = s.i32("instance").filter(|&i| i >= 0);
            Some(Layer {
                playback: Playback {
                    clip,
                    elapsed,
                    sample_time: elapsed,
                    sample_delta: 0,
                    speed: s.f32("speed").unwrap_or(1.0),
                },
                blend: time(s, "blend"),
                left: time(s, "left"),
                started: matches!(s.get("started"), Some(Value::Bool(true))),
                instance,
            })
        })
        .collect();
    (!layers.is_empty()).then(|| AnimationStack::restored(layers))
}

impl super::StartingTown {
    // AnimationManager entries (FUN_18005d130): group +0x00, clip +0x08 (-1 free), users +0x0c,
    // instances +0x20, each with its own AnimationState (FUN_18005cd90).
    /// The shared instances of instanced clips, slot by slot, so that each restored layer's
    /// instance id names the same entry and instance again. A slot whose group is not loaded is
    /// left free, and a layer whose instance is not there plays on its own.
    ///
    /// Original: `AnimationManager::Serialize`, Runtime `0x1800fea20` (reading).
    pub(in crate::town) fn restore_animations(&mut self, saved: &SavedGame) {
        use std::collections::HashMap;
        use std::sync::Arc;
        use unbanished_formats::types::AnimationGroup;
        use unbanished_game::behaviour::animation::shared;
        // The citizens play the citizen model's groups; everything else plays its template's.
        let mut groups: HashMap<String, Arc<AnimationGroup>> = HashMap::new();
        let mut add = |name: Option<&str>, group: &Option<Arc<AnimationGroup>>| {
            if let (Some(name), Some(group)) = (name, group) {
                groups
                    .entry(crate::town::normalise(name))
                    .or_insert_with(|| group.clone());
            }
        };
        if let Some(model) = &self.citizen_model {
            for (v, g) in model.variants.iter().zip(&self.citizen_anims) {
                add(v.animations.as_deref(), g);
            }
        }
        for t in &self.templates {
            if let Some(model) = &t.model {
                for (v, g) in model.variants.iter().zip(&t.animations) {
                    add(v.animations.as_deref(), g);
                }
            }
        }
        let Some(Value::List(entries)) = saved.state.animations.at("animations.entries") else {
            return;
        };
        let slots = entries
            .iter()
            .map(|e| {
                let clip = usize::try_from(e.i32("clip")?).ok()?;
                let name = saved.ext(e, "group")?;
                let group = groups.get(&crate::town::normalise(name))?;
                let Some(Value::List(instances)) = e.get("instances") else {
                    return None;
                };
                let instances = instances
                    .iter()
                    .map(|i| match i.get("states") {
                        Some(Value::List(states)) if !states.is_empty() => (
                            time(&states[0], "elapsed"),
                            states[0].f32("speed").unwrap_or(1.0),
                        ),
                        _ => (0, 1.0),
                    })
                    .collect();
                Some(shared::Saved {
                    group,
                    clip,
                    users: e.i32("users").unwrap_or(0),
                    instances,
                })
            })
            .collect();
        shared::restore(slots);
        // A layer that follows a slot the save does not hold (a save the port wrote, see
        // `export_animations`) plays on its own instead.
        let orphan = |stack: &mut unbanished_game::behaviour::animation::AnimationStack| {
            for l in &mut stack.layers {
                if l.instance.is_some_and(|id| shared::pose(id).is_none()) {
                    l.instance = None;
                }
            }
        };
        for ch in &mut self.agents.characters {
            orphan(&mut ch.animation);
        }
        for a in self.wildlife.animals.values_mut() {
            orphan(&mut a.character.animation);
        }
        for b in self.wildlife.boats.values_mut() {
            orphan(&mut b.character.animation);
        }
    }
}
