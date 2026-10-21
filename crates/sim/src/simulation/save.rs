//! Writing the simulation into a save's state: the random stream, the calendar, the
//! environment, the event log, the statistics, the stock and its warnings, and a loaded game's
//! achievement progress. The game app writes the rest (the town's entities, the camera, the
//! ambience).

use unbanished_formats::savegame::{Fields, GameState, Value};

use super::Simulation;
use crate::achievements::write_instance;
use crate::strings::{format_event, Strings};

impl Simulation {
    /// Writes the simulation's own state into `g`. The event log keeps each entry's text.
    ///
    /// Original: `FUN_14009a790`.
    pub fn write_state(&self, strings: &mut dyn Strings, g: &mut GameState) {
        g.random_index = self.rng.index();
        g.random_state = self.rng.state().to_vec();
        g.managers
            .set_i32("calendar.value", self.calendar.year as i32);
        g.managers.set_time("calendar.time", self.calendar.elapsed);
        if let Some(env) = &self.environment {
            let m = &mut g.managers;
            let ((baseline, baseline_target), (bias, bias_target), slice) = env.saved();
            m.set_i32("environment.state", env.state);
            m.set_i32("environment.level", env.level);
            m.set_f32("environment.temperature", env.temperature);
            let s = env.season;
            m.set(
                "environment.season",
                Value::F32s(vec![s.snow, s.autumn, s.leaf_loss, s.rain]),
            );
            m.set_f32("environment.baseline", baseline);
            m.set_f32("environment.baseline_target", baseline_target);
            m.set_f32("environment.bias", bias);
            m.set_f32("environment.bias_target", bias_target);
            m.set_i32("environment.slice", slice);
            if let Some(Value::List(effects)) = m.at_mut("environment.effects") {
                for (item, e) in effects.iter_mut().zip(&env.effects) {
                    *item = match e {
                        Some(e) => Fields(vec![
                            ("active", Value::Bool(true)),
                            ("direction", Value::F32(e.direction)),
                            ("intensity", Value::F32(e.intensity)),
                        ]),
                        None => Fields(vec![("active", Value::Bool(false))]),
                    };
                }
            }
        }
        // The event log keeps each entry's text (`FUN_14009a790`).
        let m = &mut g.managers;
        for (k, n) in self.events.notify.iter().enumerate() {
            m.set_bool(&format!("event_log.notify_{k}"), *n);
        }
        let entries: Vec<Fields> = self
            .events
            .entries
            .iter()
            .map(|e| {
                Fields(vec![
                    ("color", Value::U32(e.color)),
                    ("text", Value::Str(format_event(strings, e))),
                    ("kind", Value::I32(e.kind)),
                    ("severity", Value::I32(e.severity)),
                    ("cell", Value::I32(e.cell)),
                    ("category", Value::I32(e.category)),
                    ("icon", Value::I32(e.icon)),
                ])
            })
            .collect();
        m.set("event_log.entries", Value::List(entries));
        let t = &mut g.town;
        let history: Vec<Fields> = self
            .statistics
            .series
            .iter()
            .map(|v| {
                Fields(vec![
                    ("count", Value::I32(v.len() as i32)),
                    ("values", Value::U32s(v.iter().map(|&x| x as u32).collect())),
                ])
            })
            .collect();
        if let Some(Value::List(old)) = t.at("town.history") {
            if old.len() == history.len() {
                t.set("town.history", Value::List(history));
            }
        }
        t.set_time("town.time_198", self.statistics.timer);
        if let Some(Value::List(slots)) = t.at_mut("town.stock") {
            let w = &self.stock_warnings;
            for (k, s) in slots.iter_mut().enumerate().take(w.count.len()) {
                s.set_i32("count", w.count[k]);
                s.set_i32("previous", w.previous[k]);
                s.set_i32("limit", w.limits[k]);
                s.set_time("cooldown", w.cooldown[k]);
            }
        }
        if let Some(Value::List(records)) = t.at_mut("town.capacity_records") {
            for (r, &(used, capacity, cooldown)) in
                records.iter_mut().zip(&self.stock_warnings.capacity)
            {
                r.set_i32("used", used);
                r.set_i32("capacity", capacity);
                r.set_time("cooldown", cooldown);
            }
        }
    }

    /// A loaded game's achievement instances (`AchievementManager::Persist`): its saved
    /// instances take the progress, each found by its achievement's reference.
    pub fn write_loaded_achievements(&self, g: &mut GameState) {
        let items: Vec<_> = match g.achievements.at("achievements.instances") {
            Some(Value::List(items)) => items.iter().filter_map(|f| f.ptr("instance")).collect(),
            _ => Vec::new(),
        };
        for id in items {
            let reference = match g
                .objects
                .get(&id)
                .map(|o| o.fields.at("instance.achievement"))
            {
                Some(Some(Value::Ext(r))) => g.references.get(r).cloned().unwrap_or_default(),
                _ => continue,
            };
            let found = self
                .achievements
                .iter()
                .position(|a| crate::achievements::same_reference(&a.reference, &reference));
            if let Some(i) = found {
                if let Some(x) = self.achievement_manager.instances.get(i) {
                    write_instance(g, id, &self.achievements[i], x);
                }
            }
        }
    }
}
