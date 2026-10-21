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
        if let Some(town) = &self.sim.town {
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
        self.sim
            .write_state(&mut crate::ui::strings::UiStrings(&mut ui.res), &mut g);
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
        let flags = self
            .sim
            .town
            .as_ref()
            .map_or(0, |t| t.construction.game_flags);
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
        let overlay: Vec<u32> = self.sim.town.as_ref().map_or_else(Vec::new, |t| {
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
        if let Some(heights) = &self.sim.heights {
            map.heights = heights.values.clone();
        }
        if let Some(view) = &self.view {
            map.surface = view.terrain.surface.clone();
        }
        let flags = self
            .sim
            .town
            .as_ref()
            .map_or(0, |t| t.construction.game_flags);
        Some(crate::start::save::new_game::state(
            crate::start::save::new_game::NewGame {
                params: &self.params,
                flags,
                map,
                effects: self.sim.environment.as_ref().map_or(0, |e| e.effects.len()),
                history: self.sim.statistics.series.len(),
                stock: self.sim.stock_warnings.count.len(),
                capacity: self.sim.stock_warnings.capacity.len(),
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
        if self.saved.is_some() {
            self.sim.write_loaded_achievements(g);
        } else {
            let list: Vec<_> = self
                .sim
                .achievements
                .iter()
                .enumerate()
                .map(|(i, a)| achievement_save(a, self.sim.achievement_manager.instances.get(i)))
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
