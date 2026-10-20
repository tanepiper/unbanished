//! Saving a running game: its state and file, the save dialog, autosave, the minimap images and the achievements stored with it.

use super::*;

/// Where a save goes: the save store under a name (the Save dialog, autosave), or a file
/// (the `save PATH` script command).
#[derive(Debug)]
pub(super) enum SaveTarget {
    Store(String),
    #[cfg(not(target_arch = "wasm32"))]
    File(std::path::PathBuf),
}

/// An achievement's instance as `new_game` writes it: per goal its reference, its instance
/// class, its value (bit 0 met, bit 1 failed) and a TimeGoal's time.
pub(super) fn achievement_save<'a>(
    a: &'a crate::achievements::Achievement,
    instance: Option<&crate::achievements::AchievementInstance>,
) -> crate::start::save::new_game::AchievementSave<'a> {
    use unbanished_formats::scenario::GoalKind;
    crate::start::save::new_game::AchievementSave {
        reference: &a.reference,
        goals: a
            .goals
            .iter()
            .zip(&a.goal_references)
            .enumerate()
            .map(|(k, (goal, reference))| {
                let state = instance.and_then(|x| x.goals.get(k)).copied();
                let state = state.unwrap_or_default();
                let class = match goal.kind {
                    GoalKind::Time { .. } => "TimeGoalInstance",
                    GoalKind::FinishTutorial(_) => "FinishTutorialInstance",
                    _ => "GoalInstance",
                };
                let value = state.met as i32 | (state.failed as i32) << 1;
                (reference.as_str(), class, value, state.time)
            })
            .collect(),
    }
}

impl GameplayLoop {
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    /// The game state to save (docs/SAVE-WRITING.md): the loaded save's, with the town and
    /// the loop's own state written back. `None` for a game not loaded from a save.
    pub fn save_state(
        &self,
        archive: &unbanished_formats::pkg::Archive,
        ui: &mut unbanished_ui::Ui,
    ) -> Option<unbanished_formats::savegame::GameState> {
        use unbanished_formats::savegame::{Fields, Value};
        let mut g = match &self.saved {
            Some(saved) => saved.state.clone(),
            None => {
                let mut g = self.new_game_state()?;
                self.save_minimap_images(&mut g);
                g
            }
        };
        self.save_achievements(&mut g);
        g.town_name = self.params.town_name.clone();
        if let Some(town) = &self.town {
            // A new game's map ambience (a loaded one keeps its own entities), made first so
            // its regions take the scheduler entries the town keeps for them.
            let mut foreign = Vec::new();
            if let (None, Some(a)) = (&self.saved, &self.ambience) {
                let regions = crate::start::save::new_game::ambience(
                    archive,
                    &mut g,
                    &a.saved(),
                    town.entity_time(),
                );
                foreign = town.ambience_handles.iter().copied().zip(regions).collect();
            }
            if self.saved.is_some() {
                town.export_into(archive, &mut g);
            } else {
                town.export_into_with(archive, &mut g, &foreign);
            }
        }
        g.random_index = self.rng.index();
        g.random_state = self.rng.state().to_vec();
        g.managers
            .set_i32("calendar.value", self.calendar.year as i32);
        g.managers.set_time("calendar.time", self.calendar.elapsed);
        if let (Some(view), Some(c)) = (&self.view, g.camera.as_mut()) {
            if let SceneCamera::Map(camera) = &view.camera {
                c.target[0] = camera.target.x;
                c.target[1] = camera.target.y;
                c.floats = [camera.yaw, camera.pitch, camera.distance];
                // A new game's transform follows them, and the controller's zoom and pitch
                // targets and slider (`FUN_140107df0`); a loaded one keeps its own.
                if self.saved.is_none() {
                    let m = &mut g.managers;
                    m.set_f32("tools.value_3c", camera.distance_target);
                    m.set_f32("tools.value_40", camera.pitch_target());
                    m.set_f32("tools.value_44", camera.pitch_slider);
                    *c = crate::start::save::new_game::camera(
                        camera.target.to_array(),
                        camera.yaw,
                        camera.pitch,
                        camera.distance,
                    );
                }
            }
        }
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
                    ("text", Value::Str(crate::ui::utility::format_event(ui, e))),
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
        Some(g)
    }

    /// The whole `.sav` file of [`GameplayLoop::save_state`]. Its metadata (`SaveMetaData`,
    /// Runtime `0x1800c3d00`) records the enabled mods and names it after the town, as the Save dialog does (the string at
    /// `+0x28` of the object it is given; the town's name in every sample, inferred). It
    /// carries `preview` (see [`crate::saves::saved::preview`]) and keeps the loaded save's other
    /// values. `name` replaces the town's (the autosave's `Autosave` string).
    pub fn save_file(
        &self,
        archive: &unbanished_formats::pkg::Archive,
        ui: &mut unbanished_ui::Ui,
        mods: &crate::mods::Modification,
        name: Option<&str>,
        preview: Option<Vec<u8>>,
    ) -> Option<Vec<u8>> {
        let g = self.save_state(archive, ui)?;
        let state = g.encode().map_err(|e| log::error!("save: {e}")).ok()?;
        let mut metadata = match &self.saved {
            Some(saved) => saved.metadata.clone(),
            // `SaveMetaData`'s block at version 3: the extension value is 2 and no mods.
            None => unbanished_formats::save::SaveMetadata {
                name: String::new(),
                width: 0,
                height: 0,
                image: Vec::new(),
                value: 2,
                mods: Vec::new(),
                wide: false,
            },
        };
        metadata.name = name.unwrap_or(&self.params.town_name).to_string();
        metadata.mods = mods.saved();
        if let Some(image) = preview {
            let (w, h) = crate::saves::saved::PREVIEW_SIZE;
            metadata.width = w as i32;
            metadata.height = h as i32;
            metadata.image = image;
        }
        Some(unbanished_formats::save::Save::write(&metadata, &state))
    }

    /// Write a save whose preview has been drawn.
    pub(super) fn finish_save(&mut self, ctx: &mut Ctx) {
        let Some((path, name, Some(image))) = self.pending_save.take_if(|(_, _, i)| i.is_some())
        else {
            return;
        };
        self.store_window_settings(ctx.ui, true);
        let Some(bytes) =
            self.save_file(ctx.archive, ctx.ui, ctx.mods, name.as_deref(), Some(image))
        else {
            log::error!("save: the game state could not be written");
            return;
        };
        let result = match &path {
            SaveTarget::Store(name) => crate::saves::save_store::write(name, &bytes),
            #[cfg(not(target_arch = "wasm32"))]
            SaveTarget::File(path) => std::fs::write(path, &bytes).map_err(|e| e.to_string()),
        };
        match result {
            Ok(()) => log::info!("saved {path:?}"),
            Err(e) => log::error!("save {path:?}: {e}"),
        }
    }

    /// `Dialog/AutoSave.rsc`'s frame: at its interval, an autosave into `Auto.sav` named by the
    /// game's `Autosave` string, unless a dialog pauses the game (the pause menu), the game is
    /// a tutorial (flag bit 10) or a save runs (`FUN_14005e820`).
    pub(super) fn update_autosave(&mut self, ctx: &mut Ctx, dt: f32) {
        let interval = ctx.settings.game.autosave_interval;
        let flags = self.town.as_ref().map_or(0, |t| t.construction.game_flags);
        let dialog_saving = self.save_dialog.as_ref().is_some_and(|d| d.saving());
        let can_save = self.pause_menu.is_none()
            && flags & 0x400 == 0
            && self.pending_save.is_none()
            && !dialog_saving;
        let Some(a) = self.autosave.as_mut() else {
            return;
        };
        if !a.update(ctx.ui, dt, interval, can_save, dialog_saving) {
            return;
        }
        let path = SaveTarget::Store(crate::ui::save_list::AUTO_SAVE.to_string());
        let name = ctx
            .ui
            .res
            .string(Some(crate::ui::save_dialog::GLOBAL_NAMES), "Autosave");
        self.pending_save = Some((path, Some(name), None));
    }

    /// The Save and Load dialog's frame: a save it starts waits for the next frame's preview;
    /// a load it finishes starts that game.
    pub(super) fn update_save_dialog(&mut self, ctx: &mut Ctx, dt: f32) -> Option<Next> {
        let (Some(d), Some(screen)) = (self.save_dialog.as_mut(), self.screen.as_mut()) else {
            return None;
        };
        let outcome = d.update(ctx.ui, screen, dt);
        self.save_outcome(ctx, outcome)
    }

    pub(super) fn save_outcome(
        &mut self,
        ctx: &mut Ctx,
        outcome: crate::ui::save_dialog::Outcome,
    ) -> Option<Next> {
        use crate::ui::save_dialog::Outcome;
        match outcome {
            Outcome::Stay => None,
            Outcome::Closed => {
                self.save_dialog = None;
                None
            }
            Outcome::Save(path) => {
                self.pending_save = Some((SaveTarget::Store(path), None, None));
                None
            }
            // `FUN_1400bce80` → `FUN_14009d480`: the game fades out for the loaded one.
            Outcome::Load(saved) => {
                self.save_dialog = None;
                self.close_pause_menu(ctx);
                log::info!("loading {}", saved.metadata.name);
                let screen_ref = format!("Dialog/Loading.rsc:gameLoad{}", ctx.rng.rand_below(12));
                Some(Next::Switch(Box::new(crate::loops::DialogLoaderLoop::new(
                    screen_ref,
                    0.5,
                    Box::new(GameplayLoop::from_save(saved)),
                ))))
            }
        }
    }

    /// A new game's minimap (`FUN_1400aacc0`): the terrain image read back when it was drawn
    /// for the save (`FUN_1400aa610`) and the overlay of placed buildings (`FUN_1400aa200`, as
    /// `0xAARRGGBB` texels).
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    pub(super) fn save_minimap_images(&self, g: &mut unbanished_formats::savegame::GameState) {
        use unbanished_formats::savegame::Value;
        let Some((n, terrain)) = &self.save_minimap else {
            return;
        };
        let overlay: Vec<u32> = self.town.as_ref().map_or_else(Vec::new, |t| {
            t.minimap_overlay()
                .chunks(4)
                .map(|p| {
                    (p[3] as u32) << 24 | (p[0] as u32) << 16 | (p[1] as u32) << 8 | p[2] as u32
                })
                .collect()
        });
        let m = &mut g.managers;
        m.set_i32("minimap.count_28", overlay.len() as i32);
        m.set("minimap.image_28", Value::U32s(overlay));
        m.set_i32("minimap.count_18", terrain.len() as i32);
        m.set("minimap.image_18", Value::U32s(terrain.clone()));
        m.set_i32("minimap.size", *n as i32);
    }

    /// A new game's state before the town and the loop are written into it
    /// (`crate::start::save::new_game`).
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    pub(super) fn new_game_state(&self) -> Option<unbanished_formats::savegame::GameState> {
        let mut map = self.new_map.clone()?;
        if let Some(view) = &self.view {
            map.heights = view.terrain.heights.clone();
            map.surface = view.terrain.surface.clone();
        }
        let flags = self.town.as_ref().map_or(0, |t| t.construction.game_flags);
        Some(crate::start::save::new_game::state(
            crate::start::save::new_game::NewGame {
                params: &self.params,
                flags,
                map,
                effects: self.environment.as_ref().map_or(0, |e| e.effects.len()),
                history: self.statistics.series.len(),
                stock: self.stock_warnings.count.len(),
                capacity: self.stock_warnings.capacity.len(),
            },
        ))
    }

    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    /// The achievement instances (`AchievementManager::Persist`) and the game's goal
    /// (`+0x150`, its met flag `+0x158`). A loaded game's saved instances take the progress;
    /// a new game's are made.
    pub(super) fn save_achievements(&self, g: &mut unbanished_formats::savegame::GameState) {
        use crate::achievements::write_instance;
        use crate::start::save::new_game::{achievements, instance};
        use unbanished_formats::savegame::Value;
        if self.saved.is_some() {
            let items: Vec<_> = match g.achievements.at("achievements.instances") {
                Some(Value::List(items)) => {
                    items.iter().filter_map(|f| f.ptr("instance")).collect()
                }
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
        } else {
            let list: Vec<_> = self
                .achievements
                .iter()
                .enumerate()
                .map(|(i, a)| achievement_save(a, self.achievement_manager.instances.get(i)))
                .collect();
            achievements(g, &list);
        }
        let Some(goal) = &self.goal else {
            return;
        };
        g.goal_done = goal.done;
        match g.goal {
            Some(id) if g.objects.contains_key(&id) => {
                write_instance(g, id, &goal.achievement, &goal.instance);
            }
            _ => {
                g.goal = instance(
                    g,
                    &achievement_save(&goal.achievement, Some(&goal.instance)),
                )
            }
        }
    }
}
