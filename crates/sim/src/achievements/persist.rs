//! Achievement instances in a save: reading them back and writing them.

use super::*;

// +0x10
/// A saved `AchievementInstance` for `a`: each saved goal instance's value and a TimeGoalInstance's
/// time, matched to `a`'s goals by reference (in order when a reference is not among them).
///
/// Original: Runtime `0x18009efa0`.
pub fn saved_instance(
    saved: &crate::saved::SavedGame,
    id: unbanished_formats::savegame::ObjId,
    a: &Achievement,
) -> Option<AchievementInstance> {
    use unbanished_formats::savegame::Value;
    let o = saved.object(id)?;
    let mut instance = AchievementInstance::new(a);
    let Some(Value::List(goals)) = o.fields.at("instance.goals.goals") else {
        return Some(instance);
    };
    for (k, g) in goals.iter().enumerate() {
        let Some(goal) = g.ptr("goal").and_then(|g| saved.object(g)) else {
            continue;
        };
        let reference = saved.ext(&goal.fields, "goal.goal").unwrap_or("");
        let slot = a
            .goal_references
            .iter()
            .position(|r| same_reference(r, reference))
            .unwrap_or(k);
        let Some(state) = instance.goals.get_mut(slot) else {
            continue;
        };
        let value = goal.fields.i32("goal.value").unwrap_or(0);
        state.met = value & 1 != 0;
        state.failed = value & 2 != 0;
        if let Some(Value::I64(t)) = goal.fields.at("timer.time") {
            state.time = *t;
        }
    }
    Some(instance)
}

/// Writes `x` back into the saved `AchievementInstance` `id` of a loaded game: each goal
/// instance's value and a TimeGoalInstance's time, matched as [`saved_instance`] reads them.
pub fn write_instance(
    g: &mut unbanished_formats::savegame::GameState,
    id: unbanished_formats::savegame::ObjId,
    a: &Achievement,
    x: &AchievementInstance,
) {
    use unbanished_formats::savegame::Value;
    let goals: Vec<_> = match g
        .objects
        .get(&id)
        .map(|o| o.fields.at("instance.goals.goals"))
    {
        Some(Some(Value::List(goals))) => goals.iter().map(|f| f.ptr("goal")).collect(),
        _ => return,
    };
    for (k, goal) in goals.into_iter().enumerate() {
        let Some(goal) = goal else { continue };
        let reference = match g.objects.get(&goal).map(|o| o.fields.at("goal.goal")) {
            Some(Some(Value::Ext(r))) => g.references.get(r).cloned().unwrap_or_default(),
            _ => continue,
        };
        let slot = a
            .goal_references
            .iter()
            .position(|r| same_reference(r, &reference))
            .unwrap_or(k);
        let (Some(state), Some(o)) = (x.goals.get(slot), g.objects.get_mut(&goal)) else {
            continue;
        };
        o.fields
            .set_i32("goal.value", state.met as i32 | (state.failed as i32) << 1);
        o.fields.set_time("timer.time", state.time);
    }
}
