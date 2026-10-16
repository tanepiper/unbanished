//! Game/Game: calendar, climate, scheduled natural-resource growth and packaged gameplay UI.
//! Citizen AI, production and the remaining component/manager callbacks still await ports.

use banished_formats::game::climate_generator;
use banished_formats::game::EnvironmentMonth;
use banished_game::calendar::Calendar;
use banished_game::environment::{Climate, Environment, YEAR_SECONDS};
use banished_game::random::Random;
use banished_game::scheduler::ticks;
use banished_ui::{Event, Ui};

use crate::building_ui::WorkPlaceClick;
use crate::create_bar::CreateBar;
use crate::entity_ui::EntityWindows;
use crate::keymap::id as keys;
use crate::loops::{Ctx, DialogLoaderLoop, GameLoop, Next, RawInput, Screen, View};
use crate::menu::MenuLoop;
use crate::placement_tool::PlacementTool;
use crate::removal_tool::{DragSound, RemovalTool};
use crate::render::Lighting;
use crate::scene::{Camera, SceneCamera, SceneView};
use crate::selection::{Entity, Selection};
use crate::start::StartingTown;
use crate::toolbar::{Action, Toolbar};
use crate::utility::{StatusValues, UtilityClick, UtilityWindows};
use crate::world::{World, WorldParams};

const MONTHS_PER_YEAR: usize = 12;
const MONTH_SECONDS: f32 = YEAR_SECONDS / MONTHS_PER_YEAR as f32;
/// Simulation iterations per frame (`Game/Game +0x120 → +0x34`).
const SPEED_LEVELS: [u32; 4] = [1, 2, 5, 10];
/// `FUN_140104390`: the pointer scrolls within this many pixels of the left/top edge, and
/// within `EDGE + 1` of the right/bottom edge. Deviation: UI units rather than the original's
/// window pixels, which differ when the UI is scaled.
const EDGE: i32 = 4;

/// A dialog the pause menu opened.
/// A window being dragged by its `borderTitle` (ToolUI `FUN_140081100` / `FUN_140080e80`).
struct WindowDrag {
    root: banished_ui::NodeId,
    /// A utility window's template (its corner is remembered), or an entity window.
    template: Option<String>,
    /// Where the button went down (`+0x50`), and the cursor's offset from the corner
    /// (`+0x44`).
    press: (i32, i32),
    offset: (i32, i32),
    /// `+0x41`: following the cursor. An unpinned entity window waits for 5 pixels of
    /// movement (`+0x4d`), then pins itself.
    active: bool,
}

enum MenuDialog {
    Achievements(crate::achievement_ui::AchievementDialog),
    Goal(crate::achievement_ui::GoalDialog),
    Map(crate::map_params::MapParamsDialog),
}

pub struct GameplayLoop {
    params: WorldParams,
    view: Option<SceneView>,
    environment_months: Vec<EnvironmentMonth>,
    /// EnvironmentDescription `+0x20`: what forms while it rains (`Template\Tornado.rsc`).
    environment_template: Option<String>,
    screen: Option<Screen>,
    /// Calendar (`Game/Game +0x128`): year count and time into the year.
    calendar: Calendar,
    /// The game's shared random stream (`Game/Game +0xd8`, `Random(seed)` in `FUN_14009bfd0`).
    rng: Random,
    environment: Option<Environment>,
    town: Option<StartingTown>,
    /// Index into the original 1x, 2x, 5x, 10x simulation iteration values.
    speed: u8,
    paused: bool,
    toolbar: Option<Toolbar>,
    utility: UtilityWindows,
    /// The active build tool (`Game +0x80 → +0x28`) and the shared RotationSetting (`+0x10`).
    placement: Option<PlacementTool>,
    /// The removal tools (RemovalTool, a DragTool).
    removal: Option<RemovalTool>,
    create_bar: Option<CreateBar>,
    rotation: i32,
    /// Last cursor position in window pixels.
    cursor: (i32, i32),
    /// The pointer has moved at least once (`cursor` is real).
    cursor_seen: bool,
    /// Key-map actions whose key is held (`KeyMap::GetValue`).
    held: [bool; keys::COUNT],
    /// The trading posts whose trade window is open (TradeComponent `+0x71`).
    trade_held: Vec<usize>,
    /// The achievement list and the manager checking it (`Achievement::AchievementManager`).
    achievements: Vec<crate::achievements::Achievement>,
    achievement_manager: crate::achievements::AchievementManager,
    /// The tutorial running (`TutorialInstance`) and the RestrictPlayer setup in force (game UI
    /// `+0x80`).
    tutorial: Option<tutorial::Tutorial>,
    restrict: Option<tutorial::Restrict>,
    goal: Option<tutorial::GameGoal>,
    /// A finished tutorial's next scenario, started after the frame.
    next_scenario: Option<usize>,
    /// The Achievement or Goal dialog opened from the pause menu.
    menu_dialog: Option<MenuDialog>,
    window_drag: Option<WindowDrag>,
    /// `Game\Names\Cities` for the Map dialog's New Name (loaded when first wanted).
    town_names: Option<banished_formats::names::NameDatabase>,
    /// The road kinds' decal materials, the templates' DecalDescriptions, and what the decals
    /// were built for (road revision, terrain revision, the entities' signature).
    road_materials: Vec<Option<String>>,
    decal_descriptions: Vec<Option<std::sync::Arc<banished_formats::game::DecalDescription>>>,
    decals_built: Option<(u32, u64, u64, Vec<((i32, i32), i32)>)>,
    /// Each template's RadiusDescription (`{file}:radius`): radius and material.
    radius_descriptions: std::collections::HashMap<usize, Option<(i32, String)>>,
    /// The entities' particle systems (presentation; their own Random).
    particles: crate::particles::Particles,
    /// Each weather slot's rain or snow (`WeatherGeometry`).
    precipitation: Vec<Option<crate::weather::Precipitation>>,
    decal_cache: crate::scene_assets::DecalCache,
    selection: Selection,
    entities: EntityWindows,
    /// The Follow dialog (`Dialog/Follow.rsc`), opened from a citizen window.
    follow: Option<crate::follow::Follow>,
    /// The Path tool (toolbar Path, `PathTool`).
    path_tool: Option<crate::path_tool::PathTool>,
    /// `+0xc0` / `+0xc1`: a screenshot asked for (`Some(with_ui)`).
    screenshot: Option<bool>,
    highlight: crate::highlight::Highlight,
    /// The event log (`Game +0x138`) and the stock manager's warning state.
    pub events: banished_game::events::EventLog,
    stock_warnings: banished_game::events::StockWarnings,
    /// The Town Info graphs' history (StockManager `+0x150`, sampled every 75 s).
    statistics: banished_game::tracker::Statistics,
    /// The event ribbon (`Dialog/EventNotify.rsc`) and its popups.
    notify: Option<crate::event_notify::EventNotify>,
    /// Screen aspect of the last frame (MapUI's frustum outline).
    aspect: f32,
    /// `Game +0x80`: the music player.
    music: Option<crate::music::MusicPlayer>,
    /// The ghost's last cell and the CreateBar's move-sound cooldown (`+0x48`, 0.05 s).
    placement_cell: Option<(i32, i32)>,
    move_cooldown: f32,
    /// 3D effects started during the simulation iterations, played on the next audio update.
    pending_sounds: Vec<(glam::Vec3, String)>,
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    /// A save waiting for its preview: the path, the name (the town's when `None`), then the
    /// image once drawn.
    pending_save: Option<(SaveTarget, Option<String>, Option<Vec<u8>>)>,
    /// `Dialog/AutoSave.rsc`: the autosave timer and the saving spinner.
    autosave: Option<crate::autosave::AutoSave>,
    /// The Save and Load dialog over the pause menu.
    save_dialog: Option<crate::save_dialog::SaveLoadDialog>,
    /// Map ambience (`FUN_1400a57c0`): environment regions and wind.
    ambience: Option<crate::ambience::Ambience>,
    /// The map changed since the ambience last weighed it.
    ambience_stale: bool,
    /// The pause menu (`Dialog/Pause.rsc`); while open `UserInterface::PausesGame` holds.
    pause_menu: Option<crate::pause_menu::PauseMenu>,
    /// The Options dialog opened from the pause menu (id 4).
    options: Option<crate::options::OptionsDialog>,
    /// Game option `ShowCelcius` (`DAT_1401cbde0 +0x15`).
    show_celsius: bool,
    /// The EnvironmentDescription's weather list: (Weather description, ambient entity).
    weather: Vec<(Option<String>, Option<String>)>,
    /// River centre lines (MapComponent `+0x1e0`).
    river_paths: Vec<Vec<[f32; 2]>>,
    /// The minimap's terrain image read back for the pending save (size, texels).
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    save_minimap: Option<(u32, Vec<u32>)>,
    /// A new game's terrain block, kept for saving: the generated surface, rivers, lakes and
    /// materials (the heights are taken from the terrain when saving, as tunnels change them).
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    new_map: Option<banished_formats::menu_scene::MapComponent>,
    /// The save this game was loaded from (docs/SAVE-LOADING.md); `None` for a New Game.
    saved: Option<std::sync::Arc<crate::saved::SavedGame>>,
    /// The loop was given its scenario (`+0x15c` not −1, `FUN_14009d530`): a scenario's start,
    /// with a new goal even from a save.
    scenario_start: bool,
}

impl GameplayLoop {
    /// A game restored from a save: its settings, the game's Random and the calendar now;
    /// the world and town when the loop loads.
    pub fn from_save(saved: crate::saved::SavedGame) -> Self {
        // Mods are not loaded by the port: a modded save keeps its vanilla content and loses
        // what the mods added (their templates are skipped when the town is restored).
        if !saved.metadata.mods.is_empty() {
            let names: Vec<&str> = saved
                .metadata
                .mods
                .iter()
                .map(|m| m.name.as_str())
                .collect();
            log::warn!(
                "{} was saved with mods, which are not loaded: {names:?}",
                saved.metadata.name
            );
        }
        let mut g = Self::new(saved.params());
        // A save's scenario is its own (the Load dialog gives `FUN_1400b9ec0` −1).
        g.scenario_start = false;
        let s = &saved.state;
        if let Ok(state) = s.random_state.as_slice().try_into() {
            g.rng = Random::from_parts(s.random_index, state);
        }
        g.calendar.year = s.managers.i32("calendar.value").unwrap_or(0).max(0) as u32;
        if let Some(banished_formats::savegame::Value::I64(t)) = s.managers.at("calendar.time") {
            g.calendar.elapsed = *t;
        }
        g.stock_warnings = saved.stock_warnings();
        g.events = saved.event_log();
        g.statistics = saved.statistics();
        g.saved = Some(std::sync::Arc::new(saved));
        g
    }

    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    /// The game state to save (docs/SAVE-WRITING.md): the loaded save's, with the town and
    /// the loop's own state written back. `None` for a game not loaded from a save.
    pub fn save_state(
        &self,
        archive: &banished_formats::pkg::Archive,
        ui: &mut banished_ui::Ui,
    ) -> Option<banished_formats::savegame::GameState> {
        use banished_formats::savegame::{Fields, Value};
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
                    ("text", Value::Str(crate::utility::format_event(ui, e))),
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

    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    /// The whole `.sav` file of [`GameplayLoop::save_state`]. Its metadata (`SaveMetaData`,
    /// Runtime `0x1800c3d00`) names it after the town, as the Save dialog does (the string at
    /// `+0x28` of the object it is given; the town's name in every sample, inferred). It
    /// carries `preview` (see [`crate::saved::preview`]) and keeps the loaded save's other
    /// values. `name` replaces the town's (the autosave's `Autosave` string).
    pub fn save_file(
        &self,
        archive: &banished_formats::pkg::Archive,
        ui: &mut banished_ui::Ui,
        name: Option<&str>,
        preview: Option<Vec<u8>>,
    ) -> Option<Vec<u8>> {
        let g = self.save_state(archive, ui)?;
        let state = g.encode().map_err(|e| log::error!("save: {e}")).ok()?;
        let mut metadata = match &self.saved {
            Some(saved) => saved.metadata.clone(),
            // `SaveMetaData`'s block at version 3: the extension value is 2 and no mods.
            None => banished_formats::save::SaveMetadata {
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
        if let Some(image) = preview {
            let (w, h) = crate::saved::PREVIEW_SIZE;
            metadata.width = w as i32;
            metadata.height = h as i32;
            metadata.image = image;
        }
        Some(banished_formats::save::Save::write(&metadata, &state))
    }

    /// Write a save whose preview has been drawn.
    fn finish_save(&mut self, ctx: &mut Ctx) {
        let Some((path, name, Some(image))) = self.pending_save.take_if(|(_, _, i)| i.is_some())
        else {
            return;
        };
        let Some(bytes) = self.save_file(ctx.archive, ctx.ui, name.as_deref(), Some(image)) else {
            log::error!("save: the game state could not be written");
            return;
        };
        let result = match &path {
            SaveTarget::Store(name) => crate::save_store::write(name, &bytes),
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
    fn update_autosave(&mut self, ctx: &mut Ctx, dt: f32) {
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
        let path = SaveTarget::Store(crate::save_list::AUTO_SAVE.to_string());
        let name = ctx
            .ui
            .res
            .string(Some(crate::save_dialog::GLOBAL_NAMES), "Autosave");
        self.pending_save = Some((path, Some(name), None));
    }

    /// The Save and Load dialog's frame: a save it starts waits for the next frame's preview;
    /// a load it finishes starts that game.
    fn update_save_dialog(&mut self, ctx: &mut Ctx, dt: f32) -> Option<Next> {
        let (Some(d), Some(screen)) = (self.save_dialog.as_mut(), self.screen.as_mut()) else {
            return None;
        };
        let outcome = d.update(ctx.ui, screen, dt);
        self.save_outcome(ctx, outcome)
    }

    fn save_outcome(
        &mut self,
        ctx: &mut Ctx,
        outcome: crate::save_dialog::Outcome,
    ) -> Option<Next> {
        use crate::save_dialog::Outcome;
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
    fn save_minimap_images(&self, g: &mut banished_formats::savegame::GameState) {
        use banished_formats::savegame::Value;
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
    fn new_game_state(&self) -> Option<banished_formats::savegame::GameState> {
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

    /// The achievement instances with their goals' progress, for a new game. Deviation: a
    /// loaded game keeps the instances it was saved with (the port does not read them back).
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    /// The achievement instances (`AchievementManager::Persist`) and the game's goal
    /// (`+0x150`, its met flag `+0x158`). A loaded game's saved instances take the progress;
    /// a new game's are made.
    fn save_achievements(&self, g: &mut banished_formats::savegame::GameState) {
        use crate::achievements::write_instance;
        use crate::start::save::new_game::{achievements, instance};
        use banished_formats::savegame::Value;
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

    pub fn new(params: WorldParams) -> Self {
        Self {
            scenario_start: params.scenario.is_some(),
            rng: Random::new(params.seed),
            params,
            view: None,
            environment_months: Vec::new(),
            environment_template: None,
            screen: None,
            calendar: Calendar::default(),
            environment: None,
            town: None,
            speed: 0,
            paused: false,
            toolbar: None,
            utility: UtilityWindows::default(),
            placement: None,
            removal: None,
            create_bar: None,
            rotation: 0,
            cursor: (0, 0),
            cursor_seen: false,
            held: [false; keys::COUNT],
            trade_held: Vec::new(),
            achievements: Vec::new(),
            achievement_manager: Default::default(),
            tutorial: None,
            restrict: None,
            goal: None,
            next_scenario: None,
            menu_dialog: None,
            window_drag: None,
            town_names: None,
            road_materials: Vec::new(),
            decal_descriptions: Vec::new(),
            decals_built: None,
            radius_descriptions: Default::default(),
            particles: Default::default(),
            precipitation: Vec::new(),
            decal_cache: Default::default(),
            selection: Selection::default(),
            entities: EntityWindows::default(),
            follow: None,
            path_tool: None,
            screenshot: None,
            highlight: Default::default(),
            events: Default::default(),
            stock_warnings: Default::default(),
            statistics: Default::default(),
            notify: None,
            aspect: 1.6,
            music: None,
            placement_cell: None,
            move_cooldown: 0.0,
            pending_sounds: Vec::new(),
            pending_save: None,
            autosave: None,
            save_dialog: None,
            ambience: None,
            ambience_stale: false,
            pause_menu: None,
            options: None,
            show_celsius: false,
            weather: Vec::new(),
            river_paths: Vec::new(),
            new_map: None,
            save_minimap: None,
            saved: None,
        }
    }

    /// `FUN_14009d0d0` installs the toolbar with the other in-game dialogs.
    fn build_hud(
        &mut self,
        ui: &mut Ui,
        archive: &banished_formats::pkg::Archive,
        registry: &banished_formats::registry::Registry,
        keymap: &crate::keymap::KeyMap,
        #[cfg_attr(target_arch = "wasm32", allow(unused_variables))] autosave_interval: i32,
    ) {
        let mut screen = Screen::new(ui);
        let mut toolbar = Toolbar::load(archive, registry);
        toolbar.install(ui, &mut screen, keymap);
        self.toolbar = Some(toolbar);
        self.notify = crate::event_notify::EventNotify::open(ui, &mut screen, &self.events);
        {
            self.autosave = crate::autosave::AutoSave::open(ui, &mut screen, autosave_interval);
        }
        self.screen = Some(screen);
        self.update_hud(ui);
    }

    /// The values the Status Bar's FUN_14007eda0 reads from the starting town and environment.
    fn status_values(&self) -> StatusValues {
        let summary = self.town.as_ref().map_or_else(Default::default, |t| {
            banished_game::status::aggregate(
                t.citizens
                    .iter()
                    .zip(&t.agents.states)
                    .filter(|(c, _)| c.alive)
                    .map(|(c, s)| banished_game::status::CitizenValue {
                        age: c.age,
                        adult_age: t.rules.adult_age,
                        school: c.school.is_some(),
                        health: s.health,
                        happiness: s.happiness,
                        status: s.status,
                    }),
            )
        });
        StatusValues {
            events: Some(self.events.clone()),
            map_size: self.town.as_ref().map_or(0, |t| t.cells.size as u32),
            frustum: self.frustum(),
            town_name: self.params.town_name.clone(),
            year: self.calendar.year,
            month: self.month(),
            temperature: self.environment.as_ref().map_or(0.0, |e| e.temperature),
            weather: self.environment.as_ref().map_or(3, |e| e.state),
            adults: summary.adults,
            students: summary.students,
            children: summary.children,
            status: summary.counts,
            health: summary.health,
            happiness: summary.happiness,
            health_max: self
                .town
                .as_ref()
                .map_or(1.0, |t| t.agents.rules().health_max),
            happiness_max: self
                .town
                .as_ref()
                .map_or(1.0, |t| t.agents.rules().happiness),
            // Adults per profession (`FUN_14007ba70`, citizen `+0x1f0`) and workplace places.
            professions: self.town.as_ref().map_or(vec![self.population().0], |t| {
                t.profession_counts().0.iter().map(|&n| n as u32).collect()
            }),
            profession_capacity: self
                .town
                .as_ref()
                .map_or(Vec::new(), |t| t.profession_counts().1),
            // `FUN_14008f3a0`, recomputed from the town storages.
            stock_limits: self.stock_warnings.limits,
            stock: self.town.as_ref().map_or([0; 21], |t| {
                banished_game::stock::stock_table(&t.registered_storages(), &t.materials)
            }),
            show_celsius: self.show_celsius,
            citizens: self.town.as_ref().map_or(Vec::new(), |t| {
                t.citizens
                    .iter()
                    .zip(&t.agents.states)
                    .enumerate()
                    .filter(|(_, (c, _))| c.alive)
                    .map(|(i, (c, s))| {
                        (i, s.profession, c.is_adult(&t.rules) && c.school.is_none())
                    })
                    .collect()
            }),
            ..StatusValues::default()
        }
    }

    fn update_hud(&self, ui: &mut Ui) {
        if let Some(toolbar) = &self.toolbar {
            toolbar.refresh(ui, SPEED_LEVELS[self.speed as usize], self.paused);
        }
        self.utility.update(ui, &self.status_values());
    }

    fn population(&self) -> (u32, u32, u32) {
        self.town.as_ref().map_or((0, 0, 0), |t| {
            banished_game::citizens::population(&t.citizens, &t.rules)
        })
    }

    /// FUN_140105a70: ray from the camera eye through the viewport coordinate.
    fn cursor_ray(&self, screen: [i32; 2]) -> Option<(glam::Vec3, glam::Vec3)> {
        if screen[0] <= 0 || screen[1] <= 0 {
            return None;
        }
        let view = self.view.as_ref()?;
        let inv = view
            .view_proj(screen[0] as f32 / screen[1] as f32)
            .inverse();
        let ndc = glam::Vec3::new(
            self.cursor.0 as f32 / screen[0] as f32 * 2. - 1.,
            1. - self.cursor.1 as f32 / screen[1] as f32 * 2.,
            1.,
        );
        let eye = view.eye_target().0;
        Some((eye, (inv.project_point3(ndc) - eye).normalize()))
    }

    fn select_at_cursor(&mut self, ctx: &mut Ctx) {
        let picked = self.entity_at_cursor(ctx);
        let Some(town) = self.town.as_ref() else {
            return;
        };
        // `FUN_140102500`: under a RestrictPlayer setup only its template is selected.
        let picked = picked.filter(|e| {
            self.restrict.as_ref().is_none_or(|r| {
                !r.select.is_empty()
                    && e.template(town)
                        .split(':')
                        .next()
                        .unwrap_or("")
                        .eq_ignore_ascii_case(r.select.split(':').next().unwrap_or(""))
            })
        });
        if let Some(entity) = picked {
            if self.selection.selected != Some(entity) {
                self.selection.selected = Some(entity);
                log::info!("selected {entity:?}");
                if let Some(screen) = self.screen.as_mut() {
                    self.entities
                        .select(ctx.ui, screen, ctx.archive, town, entity);
                }
            }
        }
    }

    /// What is under the cursor: the map's object at the cell (`+8`, `FUN_1400a5750`), else
    /// the picking scene's hit (`FUN_140102500`, `FUN_1400e3b80`).
    fn entity_at_cursor(&mut self, ctx: &mut Ctx) -> Option<Entity> {
        let (origin, direction) = self.cursor_ray(ctx.ui.screen)?;
        let town = self.town.as_ref()?;
        let view = self.view.as_ref().unwrap();
        let grid = banished_game::terrain::HeightGrid {
            width: view.terrain.size + 1,
            heights: &view.terrain.heights,
        };
        let ground = grid.intersect(origin.to_array(), direction.to_array(), 400.);
        let cell = ground
            .map(|(_, hit)| {
                (
                    (hit[0] + (town.cells.size / 2) as f32) as i32,
                    (hit[1] + (town.cells.size / 2) as f32) as i32,
                )
            })
            .filter(|&(x, y)| {
                x >= 0 && y >= 0 && x < town.cells.size as i32 && y < town.cells.size as i32
            });
        let fallback = cell
            .and_then(|(x, y)| {
                town.cells
                    .objects
                    .get(y as usize * town.cells.size + x as usize)
                    .copied()
                    .flatten()
            })
            .map(Entity::Placed);
        self.selection.cast(
            ctx.archive,
            town,
            origin,
            direction,
            ground.map_or(400., |g| g.0),
            fallback,
        )
    }

    /// PathTool's input (`FUN_1400e3b80`): pick, highlight it (`FUN_1400e25c0`) and draw its
    /// paths (`FUN_1400e2880`).
    fn path_click(&mut self, ctx: &mut Ctx) {
        let picked = self.entity_at_cursor(ctx);
        let (Some(entity), Some(tool), Some(town)) =
            (picked, self.path_tool.as_mut(), self.town.as_mut())
        else {
            return;
        };
        tool.pick(town, &mut self.rng, entity);
        self.selection.selected = Some(entity);
        let n = town.cells.size as i32;
        let (lines, cells) = tool.overlay(n);
        if let Some(view) = self.view.as_mut() {
            let grid = banished_game::terrain::HeightGrid {
                width: view.terrain.size + 1,
                heights: &view.terrain.heights,
            };
            let height = |x: f32, y: f32| {
                let mut normal = [0.0; 3];
                grid.ground(x, y, &mut normal)
            };
            let mut vertices = crate::selection_overlay::line_vertices(&lines, n, height);
            vertices.extend(crate::selection_overlay::vertices(&cells, n, height));
            view.cell_overlay = vertices;
            if view.overlay_texture.is_none() {
                view.overlay_texture = crate::selection_overlay::load_texture(ctx.archive);
            }
        }
    }

    fn position_entity_window(&mut self, ctx: &mut Ctx) {
        let (Some(entity), Some(town), Some(view)) = (
            self.selection.selected,
            self.town.as_ref(),
            self.view.as_ref(),
        ) else {
            return;
        };
        let Some([lo, hi]) = self.selection.bounds(ctx.archive, town, entity) else {
            return;
        };
        let screen = ctx.ui.screen;
        let vp = view.view_proj(screen[0] as f32 / screen[1] as f32);
        // `FUN_140105850` → `FUN_140103e30`: the corners in front of the camera; the rectangle
        // must touch the screen, and is clamped to it.
        let (w, h) = (screen[0] as f32, screen[1] as f32);
        let mut r = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
        let mut any = false;
        for x in [lo.x, hi.x] {
            for y in [lo.y, hi.y] {
                for z in [lo.z, hi.z] {
                    let p = vp * glam::Vec4::new(x, y, z, 1.);
                    if p.w <= 0. {
                        continue;
                    }
                    any = true;
                    let x = (p.x / p.w * 0.5 + 0.5) * w;
                    let y = (0.5 - p.y / p.w * 0.5) * h;
                    r = [r[0].min(x), r[1].min(y), r[2].max(x), r[3].max(y)];
                }
            }
        }
        if !any || w < r[0] || r[2] < 0.0 || h < r[1] || r[3] < 0.0 {
            // `FUN_140102160`: off the screen, an unpinned host closes and the entity is
            // deselected.
            if let Some(screen) = self.screen.as_mut() {
                if self.entities.retire(ctx.ui, screen, entity) {
                    self.selection.selected = None;
                }
            }
            return;
        }
        let r = [
            r[0].clamp(0.0, w),
            r[1].clamp(0.0, h),
            r[2].clamp(0.0, w),
            r[3].clamp(0.0, h),
        ];
        self.entities.position(ctx.ui, entity, r);
    }

    /// The windows' title dragging (ToolUI `FUN_140081100`, `FUN_140080e80`, `FUN_140080e10`)
    /// and EntityUI's press (`FUN_140067430`). Returns whether the input was taken.
    fn drag_window(&mut self, ctx: &mut Ctx, input: &RawInput) -> bool {
        match *input {
            RawInput::UiPress => {
                let (x, y) = self.cursor;
                if let Some((template, root)) = self.utility.title_at(ctx.ui, x, y) {
                    // A utility window (ToolUI `+0x40` set at bind) follows at once.
                    let r = ctx.ui.node(root).rect;
                    self.window_drag = Some(WindowDrag {
                        root,
                        template: Some(template),
                        press: (x, y),
                        offset: (x - r[0], y - r[1]),
                        active: true,
                    });
                    return false;
                }
                let Some((i, title)) = self.entities.window_at(ctx.ui, x, y) else {
                    return false;
                };
                let w = &self.entities.open[i];
                let (root, entity, pinned) = (w.root, w.entity, w.pinned);
                if title {
                    let r = ctx.ui.node(root).rect;
                    self.window_drag = Some(WindowDrag {
                        root,
                        template: None,
                        press: (x, y),
                        offset: (x - r[0], y - r[1]),
                        active: pinned,
                    });
                }
                // `FUN_140067430`: a press on a window selects its entity again.
                if self.selection.selected != Some(entity) {
                    self.selection.selected = Some(entity);
                }
                false
            }
            RawInput::Cursor(x, y) => {
                let Some(d) = &mut self.window_drag else {
                    return false;
                };
                if !d.active {
                    if (x - d.press.0).abs() <= 5 && (y - d.press.1).abs() <= 5 {
                        return false;
                    }
                    d.active = true;
                    if let Some(w) = self.entities.open.iter_mut().find(|w| w.root == d.root) {
                        w.pinned = true;
                        // EntityUI's update: `checkPin` shows ToolUI `+0x40`.
                        if let Some(pin) = ctx.ui.find(w.root, "checkPin") {
                            ctx.ui.set_checked(pin, true);
                        }
                    }
                }
                let at = ((x - d.offset.0).max(0), (y - d.offset.1).max(0));
                let node = ctx.ui.node_mut(d.root);
                node.alignment = 0;
                node.margins = [at.0, 0, at.1, 0];
                match &d.template {
                    Some(t) => self.utility.remember(t, at),
                    None => {
                        if let Some(w) = self.entities.open.iter_mut().find(|w| w.root == d.root) {
                            w.at = Some(at);
                        }
                    }
                }
                self.cursor = (x, y);
                d.active
            }
            RawInput::Release => {
                self.window_drag = None;
                false
            }
            _ => false,
        }
    }

    /// `FUN_140106740`: the map cell under the cursor. The original casts the camera ray
    /// against the terrain (`FUN_1400afa00`); this marches the ray and refines the hit by
    /// bisection on the exact ground query.
    fn cursor_cell(&self, screen: [i32; 2]) -> Option<(i32, i32)> {
        let (view, town) = (self.view.as_ref()?, self.town.as_ref()?);
        if screen[0] <= 0 || screen[1] <= 0 {
            return None;
        }
        let aspect = screen[0] as f32 / screen[1] as f32;
        let inv = view.view_proj(aspect).inverse();
        let ndc = glam::Vec2::new(
            self.cursor.0 as f32 / screen[0] as f32 * 2.0 - 1.0,
            1.0 - self.cursor.1 as f32 / screen[1] as f32 * 2.0,
        );
        let near = inv.project_point3(ndc.extend(0.0));
        let far = inv.project_point3(ndc.extend(1.0));
        let grid = banished_game::terrain::HeightGrid {
            width: view.terrain.size + 1,
            heights: &view.terrain.heights,
        };
        let below = |p: glam::Vec3| {
            let mut normal = [0.0; 3];
            p.z <= grid.ground(p.x, p.y, &mut normal)
        };
        let length = (far - near).length();
        let dir = (far - near) / length;
        let step = 0.5;
        let mut t = 0.0;
        let mut prev = 0.0;
        while t < length {
            if below(near + dir * t) {
                let (mut lo, mut hi) = (prev, t);
                for _ in 0..20 {
                    let mid = (lo + hi) * 0.5;
                    if below(near + dir * mid) {
                        hi = mid;
                    } else {
                        lo = mid;
                    }
                }
                let hit = near + dir * hi;
                let n = town.cells.size as i32;
                let x = ((hit.x + (n >> 1) as f32) as i32).clamp(0, n - 1);
                let y = ((hit.y + (n >> 1) as f32) as i32).clamp(0, n - 1);
                return Some((x, y));
            }
            prev = t;
            t += step;
        }
        None
    }

    /// Update the placement ghost after the cursor, rotation or variant changed.
    fn refresh_placement(&mut self, ctx: &mut Ctx) {
        let cell = self.cursor_cell(ctx.ui.screen);
        let (Some(tool), Some(town), Some(view)) = (
            self.placement.as_mut(),
            self.town.as_mut(),
            self.view.as_mut(),
        ) else {
            return;
        };
        let grid = banished_game::terrain::HeightGrid {
            width: view.terrain.size + 1,
            heights: &view.terrain.heights,
        };
        tool.hover(town, &grid, cell, self.rotation);
        // `FUN_1401050c0`: a new cell plays the move sound, at most every 0.05 s.
        if cell.is_some() && cell != self.placement_cell {
            self.placement_cell = cell;
            if self.move_cooldown <= 0.0 {
                ctx.audio.play(
                    ctx.archive,
                    crate::sound::global::PLACEMENT_MOVE,
                    crate::sound::sound_type::UI,
                );
                self.move_cooldown = 0.05;
            }
        }
        // `FUN_140103190`: the footprint's cells under the cursor, then (game UI `+0x80`) the
        // tutorial's target in yellow, with the building's footprint and its rotation.
        let degrees = self.rotation * banished_game::placement::ROTATION_STEP as i32;
        let mut cells = tool.rect().map_or_else(Vec::new, |rect| {
            tool.overlay(town, &grid, rect, degrees, false)
        });
        if let Some(r) = self.restrict.as_ref().filter(|r| !r.place.is_empty()) {
            let rect = banished_game::placement::Rect {
                x0: r.rect[0],
                y0: r.rect[1],
                x1: r.rect[2],
                y1: r.rect[3],
            };
            cells.extend(tool.overlay(town, &grid, rect, r.rotation, true));
        }
        let n = town.cells.size as i32;
        view.cell_overlay = crate::selection_overlay::vertices(&cells, n, |x, y| {
            let mut normal = [0.0; 3];
            grid.ground(x, y, &mut normal)
        });
        if view.overlay_texture.is_none() && !cells.is_empty() {
            view.overlay_texture = crate::selection_overlay::load_texture(ctx.archive);
        }
        let ghost = tool.ghost(self.rotation);
        if ghost != town.ghost {
            town.ghost = ghost;
            if let Some(models) = &mut view.models {
                town.refresh_models(ctx.archive, models);
            }
        }
        // `FUN_140104080` / `FUN_140103e30`: the ghost's screen rectangle for the CreateBar.
        let screen = ctx.ui.screen;
        let rect = tool.ghost_corners(self.rotation).and_then(|corners| {
            if screen[0] <= 0 || screen[1] <= 0 {
                return None;
            }
            let vp = view.view_proj(screen[0] as f32 / screen[1] as f32);
            let mut r = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
            for c in corners {
                let p = vp * c.extend(1.0);
                if p.w <= 0.0 {
                    return None;
                }
                let (x, y) = (
                    (p.x / p.w * 0.5 + 0.5) * screen[0] as f32,
                    (0.5 - p.y / p.w * 0.5) * screen[1] as f32,
                );
                r = [r[0].min(x), r[1].min(y), r[2].max(x), r[3].max(y)];
            }
            Some(r)
        });
        let allowed = Self::restrict_allows(self.restrict.as_ref(), tool, self.rotation);
        if let Some(bar) = &mut self.create_bar {
            bar.update(ctx.ui, tool.size(), tool.valid() && allowed, rect);
        }
    }

    /// `FUN_140069860`.
    fn close_follow(&mut self, ctx: &mut Ctx) {
        if let (Some(f), Some(screen)) = (self.follow.take(), self.screen.as_mut()) {
            f.close(ctx.ui, screen);
        }
    }

    /// The Follow dialog each frame (`FUN_140069bb0`, `FUN_1400698d0`): the camera on the
    /// citizen and its texts; it closes with its citizen window (`FUN_140065a90`) or citizen.
    fn update_follow(&mut self, ctx: &mut Ctx) {
        let Some(f) = &self.follow else { return };
        let target = self.town.as_ref().and_then(|t| f.target(t));
        let (Some(target), true) = (target, self.entities.is_open(f.owner)) else {
            self.close_follow(ctx);
            return;
        };
        if let (Some(town), Some(view)) = (self.town.as_ref(), self.view.as_mut()) {
            if let SceneCamera::Map(camera) = &mut view.camera {
                camera.target = target;
            }
            f.layout(ctx.ui, town);
        }
    }

    /// `FUN_140102b20` / `FUN_1401033b0`: a RestrictPlayer setup allows the placement only
    /// on its cells and turned its way.
    fn restrict_allows(
        restrict: Option<&tutorial::Restrict>,
        tool: &PlacementTool,
        rotation: i32,
    ) -> bool {
        let Some(r) = restrict else { return true };
        let Some(rect) = tool.rect() else {
            return false;
        };
        rect.x0 == r.rect[0]
            && rect.y0 == r.rect[1]
            && rect.x1 == r.rect[2]
            && rect.y1 == r.rect[3]
            && rotation * 90 == r.rotation
    }

    /// `FUN_140107b70`: with a template to place, the player's tool is its.
    fn force_restricted_tool(&mut self, ctx: &mut Ctx) {
        let Some(place) = self
            .restrict
            .as_ref()
            .filter(|r| !r.place.is_empty())
            .map(|r| r.place.split(':').next().unwrap_or("").to_string())
        else {
            return;
        };
        let current = self
            .placement
            .as_ref()
            .map(|t| t.reference.clone())
            .or_else(|| self.removal.as_ref().map(|t| t.reference.clone()));
        let same = current.as_deref().is_some_and(|c| {
            c.split(':')
                .next()
                .unwrap_or("")
                .eq_ignore_ascii_case(&place)
        });
        if !same {
            self.start_tool(ctx, &format!("{place}:resource"));
        }
    }

    /// Toolbar kind 1 → `FUN_140108030`: start the tool for `template`.
    fn start_tool(&mut self, ctx: &mut Ctx, template: &str) {
        // `FUN_140108030`: the tool class that handles one of the template's
        // descriptions (CreatePlacedDescription, CreateRemovalDescription, ...).
        self.removal = None;
        self.placement = self
            .town
            .as_mut()
            .and_then(|town| PlacementTool::start(ctx.archive, town, template, &mut self.rng));
        if self.placement.is_none() {
            self.removal = self
                .town
                .as_mut()
                .and_then(|town| RemovalTool::start(ctx.archive, town, template));
        }
        // A DragTool opens its CreateBar only while dragging (`FUN_140104eb0`).
        let bar = self
            .placement
            .as_ref()
            .and_then(|tool| tool.toolbar.as_ref().map(|t| (t, tool.build.as_ref())));
        if let Some(screen) = self.screen.as_mut() {
            if let Some(old) = self.create_bar.take() {
                old.close(ctx.ui, screen);
            }
            self.create_bar =
                bar.and_then(|(t, build)| CreateBar::open(ctx.ui, screen, ctx.archive, t, build));
        }
        // `FUN_1400e2720`: the Path tool's highlight goes with it.
        if self.path_tool.take().is_some_and(|t| t.entity.is_some()) {
            self.selection.selected = None;
        }
        if self.placement.is_none() && self.removal.is_none() {
            self.path_tool = crate::path_tool::PathTool::start(ctx.archive, template);
        }
        if self.placement.is_none() && self.removal.is_none() && self.path_tool.is_none() {
            log::info!("{template}: no tool for it");
        }
        self.refresh_placement(ctx);
    }

    /// The removal tool's input (DragTool `FUN_140101b80`) with the cell under the cursor
    /// (`FUN_140106740`, −1 off the map).
    fn removal_input(&mut self, ctx: &mut Ctx, input: DragInput) {
        let cell = self.cursor_cell(ctx.ui.screen);
        let (Some(tool), Some(town), Some(view)) = (
            self.removal.as_mut(),
            self.town.as_mut(),
            self.view.as_mut(),
        ) else {
            return;
        };
        let n = town.cells.size as i32;
        let cell = cell.map_or(-1, |(x, y)| y * n + x);
        let grid = banished_game::terrain::HeightGrid {
            width: view.terrain.size + 1,
            heights: &view.terrain.heights,
        };
        tool.rotation = self.rotation;
        tool.restrict = self.restrict.as_ref().map(|r| r.rect);
        tool.manhattan = !self.held[keys::PATH_METHOD];
        let sound = match input {
            DragInput::Press => tool.press(ctx.archive, town, &grid, cell),
            DragInput::Move => tool.motion(ctx.archive, town, &grid, cell),
            DragInput::Release => tool.release(ctx.archive, town, &grid, cell),
        };
        let cells: Vec<_> = tool
            .cells(town, &grid, cell)
            .into_iter()
            .map(|(x, y, acts)| crate::selection_overlay::OverlayCell::removal(x, y, acts))
            .collect();
        view.cell_overlay = crate::selection_overlay::vertices(&cells, n, |x, y| {
            let mut normal = [0.0; 3];
            grid.ground(x, y, &mut normal)
        });
        if view.overlay_texture.is_none() && !cells.is_empty() {
            view.overlay_texture = crate::selection_overlay::load_texture(ctx.archive);
        }
        // DragTool `+0x30`: the CreateBar is open while dragging (`FUN_140104eb0`), shows the
        // drag's size and the toolbar message 0 (`FUN_140105130`) and closes with the drag.
        let screen_size = ctx.ui.screen;
        match (tool.drag_ends(), tool.drag_info(town, &grid)) {
            (Some((a, b)), Some(info)) => {
                if self.create_bar.is_none() {
                    if let (Some(t), Some(screen)) = (tool.toolbar.as_ref(), self.screen.as_mut()) {
                        // The bar lists the tool template's build requirements.
                        let file = tool.reference.split(':').next().unwrap_or("");
                        let build = ctx
                            .archive
                            .read_ref(&format!("{}:build", file.replace('\\', "/")))
                            .ok()
                            .and_then(|d| banished_formats::game::build_description(&d).ok());
                        self.create_bar =
                            CreateBar::open(ctx.ui, screen, ctx.archive, t, build.as_ref());
                    }
                }
                let aspect = screen_size[0].max(1) as f32 / screen_size[1].max(1) as f32;
                let vp = view.view_proj(aspect);
                let half = (n >> 1) as f32 - 0.5;
                let mut r = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
                let mut visible = screen_size[0] > 0 && screen_size[1] > 0;
                for (x, y) in [
                    (a % n, a / n),
                    (a % n, b / n),
                    (b % n, a / n),
                    (b % n, b / n),
                ] {
                    let (px, py) = (x as f32 - half, y as f32 - half);
                    let mut normal = [0.0; 3];
                    let p = vp * glam::Vec4::new(px, py, grid.ground(px, py, &mut normal), 1.0);
                    if p.w <= 0.0 {
                        visible = false;
                        break;
                    }
                    let (sx, sy) = (
                        (p.x / p.w * 0.5 + 0.5) * screen_size[0] as f32,
                        (0.5 - p.y / p.w * 0.5) * screen_size[1] as f32,
                    );
                    r = [r[0].min(sx), r[1].min(sy), r[2].max(sx), r[3].max(sy)];
                }
                if let Some(bar) = &mut self.create_bar {
                    bar.update_drag(ctx.ui, info, visible.then_some(r));
                }
            }
            _ => {
                if let (Some(bar), Some(screen)) = (self.create_bar.take(), self.screen.as_mut()) {
                    bar.close(ctx.ui, screen);
                }
            }
        }
        let sound = match sound {
            Some(DragSound::Move) if self.move_cooldown > 0.0 => None,
            Some(DragSound::Move) => {
                self.move_cooldown = 0.05;
                Some(crate::sound::global::PLACEMENT_MOVE)
            }
            Some(DragSound::Thunk) => Some(crate::sound::global::PLACEMENT_THUNK),
            Some(DragSound::Error) => Some(crate::sound::global::PLACEMENT_ERROR),
            None => None,
        };
        if let Some(sound) = sound {
            ctx.audio
                .play(ctx.archive, sound, crate::sound::sound_type::UI);
        }
    }

    fn end_placement(&mut self, ctx: &mut Ctx) {
        self.placement = None;
        self.removal = None;
        if self.path_tool.take().is_some_and(|t| t.entity.is_some()) {
            self.selection.selected = None;
        }
        if let Some(view) = self.view.as_mut() {
            view.cell_overlay.clear();
        }
        if let (Some(bar), Some(screen)) = (self.create_bar.take(), self.screen.as_mut()) {
            bar.close(ctx.ui, screen);
        }
        if let (Some(town), Some(view)) = (self.town.as_mut(), self.view.as_mut()) {
            town.ghost = None;
            if let Some(models) = &mut view.models {
                town.refresh_models(ctx.archive, models);
            }
        }
    }

    /// `floor(elapsed / 3600 × 12)`.
    fn month(&self) -> usize {
        ((self.calendar.seconds() / YEAR_SECONDS * MONTHS_PER_YEAR as f32) as usize)
            .min(MONTHS_PER_YEAR - 1)
    }

    /// Port of FUN_14009b850: entity scheduler, integer calendar, then environment.
    /// Growth reads the prior environment temperature; weather draws follow entity callbacks.
    /// Returns whether spawning/removal changed the model instances.
    fn iterate(&mut self, archive: &banished_formats::pkg::Archive, dt: f32) -> bool {
        let mut models_changed = false;
        if let (Some(town), Some(view)) = (&mut self.town, &self.view) {
            let grid = banished_game::terrain::HeightGrid {
                width: view.terrain.size + 1,
                heights: &view.terrain.heights,
            };
            let temperature = self.environment.as_ref().map_or(0.0, |e| e.temperature);
            // The crop fields read the calendar (`Game +0x128`).
            town.construction.calendar = self.calendar.seconds();
            town.construction.year = self.calendar.year;
            // Fire spreads less in rain (environment `+0x10`).
            town.construction.precipitation =
                self.environment.as_ref().is_some_and(|e| e.state == 0);
            // Player commands and placed sites take effect between iterations.
            town.apply_commands(&grid, &mut self.rng);
            let changed = town.iterate(&grid, &mut self.rng, dt, temperature);
            // AnimationManager::Update runs inside the same iteration (FUN_14009b850).
            let stock = std::array::from_fn(|i| {
                (self.stock_warnings.count[i], self.stock_warnings.limits[i])
            });
            let changed =
                town.update_citizens(&grid, &mut self.rng, dt, temperature, stock) | changed;
            // AnimAudioEvents that passed their draw: `PlayEffect3D(effect, 4)` at the entity.
            for (entity, effect) in town.animation_events.drain(..) {
                let p = town.placed[entity].position;
                self.pending_sounds
                    .push((glam::Vec3::new(p[0], p[1], p[2]), effect));
            }
            models_changed = changed;
        }
        // The tunnels' terraforming and the flattening under stamped footprints and roads, on
        // the terrain the town reads (`FUN_140057c70`, `FUN_1400a35b0`).
        if let (Some(town), Some(view)) = (&mut self.town, &mut self.view) {
            let c = &town.construction;
            if !c.terraform_requests.is_empty() || !c.flatten_requests.is_empty() {
                let width = view.terrain.width();
                if town.apply_terraforms(&mut view.terrain.heights, width) {
                    view.terrain.reshaped();
                }
            }
        }
        self.calendar.advance(ticks(dt));
        let mut tornado = false;
        if let Some(env) = &mut self.environment {
            tornado = env.update(self.calendar.seconds(), dt, &mut self.rng);
        }
        // `FUN_1400999b0` step 6: the environment template's entity.
        if tornado {
            if let (Some(town), Some(view), Some(reference)) = (
                &mut self.town,
                &self.view,
                self.environment_template.as_deref(),
            ) {
                let grid = banished_game::terrain::HeightGrid {
                    width: view.terrain.size + 1,
                    heights: &view.terrain.heights,
                };
                town.spawn_tornado(archive, reference, &grid, &mut self.rng);
                models_changed = true;
            }
        }
        // `FUN_140091780`: recompute stock (`FUN_14008f3a0`), then its warnings
        // (`FUN_1400915d0`, posted through `FUN_14009aa50` at severity 3).
        if let Some(town) = &mut self.town {
            self.stock_warnings
                .recompute(&town.registered_storages(), &town.materials);
            // StockManager::Update's commute optimisation (every 300 s).
            if let Some(view) = &self.view {
                let grid = banished_game::terrain::HeightGrid {
                    width: view.terrain.size + 1,
                    heights: &view.terrain.heights,
                };
                town.update_commute(&grid, &mut self.rng, dt);
            }
            // The job manager's pass and clock (`FUN_14009b850`, after the stock update).
            town.construction.jobs.step(ticks(dt));
            // `FUN_14008fc00` every 75 s: the citizens and every stock slot.
            let life = &town.agents.life;
            let samples: Vec<banished_game::tracker::CitizenSample> = town
                .citizens
                .iter()
                .zip(&town.agents.states)
                .filter(|(c, _)| c.alive)
                .map(|(c, s)| banished_game::tracker::CitizenSample {
                    adult: life.adult_age() <= c.age,
                    school: s.school.is_some(),
                    health: s.health,
                    happiness: s.happiness,
                    educated: life.i(0x118) <= c.education,
                    coat: s.clothing != 0,
                })
                .collect();
            let (health_max, happiness_max) = (life.f(0x70), life.f(0xa4));
            self.statistics.update(
                ticks(dt),
                &samples,
                health_max,
                happiness_max,
                &self.stock_warnings.count,
                &mut town.construction.counters,
            );
            let (month, year) = (self.month(), self.calendar.year as i32);
            for kind in self.stock_warnings.check(ticks(dt)) {
                // AutoSlow is applied once the frame's events are in (`slow_request`).
                self.events
                    .add(kind, 3, -1, month, year, Default::default(), true);
            }
        }
        models_changed
    }

    /// The map's decals when what they are built from changed: the entities' (`+0x1f0`,
    /// `FUN_1400b2310`), then the roads' (`FUN_1400b4840`).
    fn refresh_decals(&mut self, archive: &banished_formats::pkg::Archive) {
        let (Some(town), Some(view)) = (self.town.as_ref(), self.view.as_mut()) else {
            return;
        };
        // Templates are added as they are first placed.
        let known = self.decal_descriptions.len();
        if known < town.templates.len() {
            self.decal_descriptions
                .extend(crate::building_decals::descriptions(
                    archive,
                    &town.templates[known..],
                ));
        }
        let decals = &town.construction.decals;
        // Work radii (`FUN_140055d90`): the placement ghost's, then each open window's with a
        // RadiusUI about its building's centre (`FUN_14007cb20`).
        let mut rings: Vec<((i32, i32), i32, String)> = self
            .placement
            .as_ref()
            .and_then(|t| t.radius_decal())
            .map(|(cell, r, m)| (cell, r, m.to_string()))
            .into_iter()
            .collect();
        for e in self.entities.radius_windows() {
            let Some(p) = town.placed.get(e).filter(|p| p.alive) else {
                continue;
            };
            let template = p.template;
            let radius = self.radius_descriptions.entry(template).or_insert_with(|| {
                let file = town.templates[template]
                    .reference
                    .split(':')
                    .next()?
                    .replace('\\', "/");
                let data = archive.read_ref(&format!("{file}:radius")).ok()?;
                let (r, m) = banished_formats::game::radius_description(&data).ok()?;
                Some((r, m?))
            });
            if let Some((r, m)) = radius {
                let cell = (
                    p.cell.0 + (p.footprint.0 >> 1),
                    p.cell.1 + (p.footprint.1 >> 1),
                );
                rings.push((cell, *r, m.clone()));
            }
        }
        let key = (
            town.cells.road_revision,
            view.terrain.revision,
            crate::building_decals::signature(&town.placed, &self.decal_descriptions, decals),
            rings.iter().map(|(c, r, _)| (*c, *r)).collect::<Vec<_>>(),
        );
        if self.decals_built.as_ref() == Some(&key) {
            return;
        }
        self.decals_built = Some(key);
        let grid = banished_game::terrain::HeightGrid {
            width: view.terrain.size + 1,
            heights: &view.terrain.heights,
        };
        let mut list = crate::building_decals::build(
            archive,
            &town.placed,
            &self.decal_descriptions,
            decals,
            &grid,
            town.cells.size,
            &mut self.decal_cache,
        );
        for (cell, r, material) in &rings {
            list.extend(crate::building_decals::radius(
                archive,
                *cell,
                *r,
                material,
                &grid,
                town.cells.size,
                &mut self.decal_cache,
            ));
        }
        let entities = list.len();
        list.extend(crate::road_decals::build(
            archive,
            &town.cells,
            &grid,
            &self.road_materials,
            &mut self.decal_cache,
        ));
        log::debug!("decals rebuilt: {entities} entities, {} in all", list.len());
        if let Some(e) = &mut view.extras {
            e.decals = list;
            view.decals_revision += 1;
        }
    }

    /// The ParticleComponents' slots against the town, their systems run by the game time
    /// (`FUN_140053490`, the manager's delta), and this frame's draws.
    fn refresh_particles(&mut self, archive: &banished_formats::pkg::Archive, dt: f32) {
        let (Some(town), Some(view)) = (self.town.as_ref(), self.view.as_mut()) else {
            return;
        };
        let grid = banished_game::terrain::HeightGrid {
            width: view.terrain.size + 1,
            heights: &view.terrain.heights,
        };
        self.particles.drive(archive, town, &grid);
        if 0.0 < dt {
            self.particles.update(dt);
        }
        view.particles = self.particles.batches(archive);
        for b in &view.particles {
            if !view.particle_textures.contains_key(&b.material) {
                let textures = crate::scene_assets::particle_textures(archive, &b.material);
                view.particle_textures.insert(b.material.clone(), textures);
            }
        }
    }

    /// The rain and snow (`WeatherGeometry::Update` by the game time): each slot's particles at
    /// its effect's intensity, none with ShowWeather off (`FUN_1400999b0`).
    fn refresh_weather(&mut self, ctx: &mut Ctx, dt: f32) {
        let show = ctx.settings.game.show_weather;
        let (Some(view), Some(env)) = (self.view.as_mut(), self.environment.as_ref()) else {
            return;
        };
        view.weather.clear();
        for (slot, p) in self.precipitation.iter_mut().enumerate() {
            let Some(p) = p else { continue };
            let intensity = env
                .effects
                .get(slot)
                .copied()
                .flatten()
                .filter(|_| show)
                .map_or(0.0, |e| e.intensity);
            if intensity <= 0.0 {
                continue;
            }
            if 0.0 < dt {
                p.update(dt);
            }
            if let Some(b) = p.batch(intensity) {
                if !view.weather_textures.contains_key(&b.material) {
                    let t = crate::scene_assets::particle_textures(ctx.archive, &b.material).0;
                    view.weather_textures.insert(b.material.clone(), t);
                }
                view.weather.push(b);
            }
        }
    }

    /// `AchievementManager::Update(dt × iterations)` after the frame's iterations; one awarded
    /// is stored and logged (event `0x19`, `FUN_14009be30`, `@2` its title).
    fn update_achievements(&mut self, ctx: &mut Ctx, dt: i64) {
        let Some(town) = &self.town else { return };
        // The TerrainGenerator's resource is named for the terrain.
        let terrain = format!(r"Game\Terrain\{}.rsc", self.params.terrain);
        let x = crate::achievements::GoalContext {
            town,
            statistics: &self.statistics,
            year: self.calendar.year,
            year_seconds: self.calendar.seconds(),
            terrain: &terrain,
            climate: &self.params.climate_ref,
            size: self.params.size as i32,
        };
        let won = self
            .achievement_manager
            .update(&self.achievements, dt, &x, Some(ctx.archive));
        if let Some(i) = won {
            let a = &self.achievements[i].desc;
            let title = ctx.ui.res.string(a.strings.as_deref(), &a.title);
            ctx.settings
                .persistence
                .set_bool("Achievement", &a.title, true);
            ctx.settings.persistence.write();
            let (month, year) = (self.month(), self.calendar.year as i32);
            self.events.add(
                0x19,
                4,
                -1,
                month,
                year,
                [title, String::new(), String::new()],
                false,
            );
        }
    }

    /// `FUN_140118fc0`: the citizens' births, deaths and coming of age posted to the event
    /// log with `@2` the citizen's name, `@3` its role (profession, child or student) and `@4`
    /// the disease's or accident's name.
    fn log_life_events(&mut self, ctx: &mut Ctx) {
        let (month, year) = (self.month(), self.calendar.year as i32);
        let Some(town) = &mut self.town else { return };
        let events = std::mem::take(&mut town.agents.events);
        if events.is_empty() {
            return;
        }
        for e in events {
            let Some(c) = town.citizens.get(e.citizen) else {
                continue;
            };
            let state = &town.agents.states[e.citizen];
            let child = c.age < town.rules.adult_age;
            let key = if c.school.is_some() {
                Some("student")
            } else if child {
                Some("child")
            } else {
                None
            };
            let role = match key {
                Some(k) => town.profession(k).map(|p| (p.string_table, p.name)),
                None => town
                    .construction
                    .professions
                    .list
                    .get(state.profession)
                    .map(|p| (p.string_table.clone(), p.name.clone())),
            }
            .map(|(t, k)| ctx.ui.res.string(t.as_deref(), &k))
            .unwrap_or_default();
            let extra = e
                .extra
                .map(|(t, k)| ctx.ui.res.string(t.as_deref(), &k))
                .unwrap_or_default();
            let auto_slow = ctx.settings.game.auto_slow;
            self.events.add(
                e.kind,
                e.severity,
                -1,
                month,
                year,
                [c.name.clone(), role, extra],
                auto_slow,
            );
        }
    }

    /// The sites' events (`FUN_14009a2d0` with the entity): `@2` its toolbar name, at its
    /// map-rectangle centre cell (`FUN_14009a500`).
    fn log_site_events(&mut self, ctx: &mut Ctx) {
        let (month, year) = (self.month(), self.calendar.year as i32);
        let Some(town) = &mut self.town else { return };
        let events = std::mem::take(&mut town.construction.site_events);
        for (kind, severity, e) in events {
            let Some(p) = town.placed.get(e) else {
                continue;
            };
            let reference = &town.templates[p.template].reference;
            let file = reference.split(':').next().unwrap_or("");
            let name = ctx
                .archive
                .read_ref(&format!("{file}:toolbar"))
                .ok()
                .and_then(|d| banished_formats::game::toolbar_description(&d).ok())
                .map(|t| ctx.ui.res.string(t.string_table.as_deref(), &t.name))
                .unwrap_or_default();
            let n = town.cells.size as i32;
            let cell = (p.cell.1 + (p.footprint.1 >> 1)) * n + p.cell.0 + (p.footprint.0 >> 1);
            let auto_slow = ctx.settings.game.auto_slow;
            self.events.add(
                kind,
                severity,
                cell,
                month,
                year,
                [name, String::new(), String::new()],
                auto_slow,
            );
        }
    }

    /// `FUN_14006cf10`: the camera's four far-frustum corner rays met with the ground plane
    /// (z = 0), ordered top-left, top-right, bottom-left, bottom-right.
    fn frustum(&self) -> Option<[[f32; 2]; 4]> {
        let view = self.view.as_ref()?;
        let SceneCamera::Map(camera) = &view.camera else {
            return None;
        };
        let inv = camera.view_proj(&view.terrain, self.aspect).inverse();
        let eye = camera.eye(&view.terrain);
        let corner = |x: f32, y: f32| {
            let dir = inv.project_point3(glam::Vec3::new(x, y, 1.0)) - eye;
            let t = -eye.z / dir.z;
            [eye.x + dir.x * t, eye.y + dir.y * t]
        };
        Some([
            corner(-1.0, 1.0),
            corner(1.0, 1.0),
            corner(-1.0, -1.0),
            corner(1.0, -1.0),
        ])
    }

    /// `FUN_140067cd0` / `FUN_140068f00`: centre the camera on a cell
    /// (`x = cell % w - (w/2 - 0.5)`, likewise `y`; the original also takes the ground height).
    fn go_to(&mut self, cell: i32) {
        let (Some(town), Some(view)) = (self.town.as_ref(), self.view.as_mut()) else {
            return;
        };
        let n = town.cells.size as i32;
        let half = (n >> 1) as f32 - 0.5;
        if let SceneCamera::Map(camera) = &mut view.camera {
            camera.target = glam::Vec2::new((cell % n) as f32 - half, (cell / n) as f32 - half);
        }
    }

    /// Application tooltip controllers: `BuildRequirementUI` on the toolbar's build tooltip
    /// (`Dialog/ToolTipToolBar.rsc`), filled from the button's template.
    fn fill_tooltip(&self, ctx: &mut Ctx) {
        let Some((source, root, controllers)) = ctx.ui.new_tooltip() else {
            return;
        };
        if !controllers.contains(&banished_ui::tooltip_controller::BUILD_REQUIREMENT) {
            return;
        }
        let Some((template, description)) =
            self.toolbar.as_ref().and_then(|t| t.build_entry(source))
        else {
            return;
        };
        let file = template.split(':').next().unwrap_or("").replace('\\', "/");
        let build = ctx
            .archive
            .read_ref(&format!("{file}:build"))
            .ok()
            .and_then(|d| banished_formats::game::build_description(&d).ok());
        crate::create_bar::fill_requirements(
            ctx.ui,
            ctx.archive,
            root,
            description,
            build.as_ref(),
        );
        ctx.ui.place_tooltip();
    }

    /// `FUN_14009b850`'s audio: the music player (`FUN_1400a95b0`), then
    /// `Scene::Update(dt, dt × speed × 6, random, camera pivot, camera distance)`.
    fn update_audio(&mut self, ctx: &mut Ctx, dt: f32) {
        let v = self.status_values();
        let people = v.adults + v.students + v.children;
        let mood = crate::music::mood(v.health, v.health_max, v.happiness, v.happiness_max, people);
        let month = self.month() as i32;
        let music = self
            .music
            .get_or_insert_with(|| crate::music::MusicPlayer::load(ctx.archive));
        music.update(ctx.audio, ctx.archive, month, mood);
        let temperature = self.environment.as_ref().map_or(50.0, |e| e.temperature);
        if let Some(town) = &self.town {
            match &mut self.ambience {
                None => {
                    self.ambience = Some(crate::ambience::Ambience::create(
                        ctx.audio,
                        ctx.archive,
                        &self.params.start_condition_ref,
                        town,
                        &self.river_paths,
                        temperature,
                    ));
                }
                Some(a) => {
                    if std::mem::take(&mut self.ambience_stale) {
                        a.reweigh(town);
                        a.sync_entities(ctx.audio, ctx.archive, town);
                    }
                    let particles = &self.particles;
                    a.follow_tornadoes(ctx.audio, town, |e| particles.alpha((e, 0)));
                    a.drive_entities(ctx.audio, town);
                    a.fade(ctx.audio, temperature);
                }
            }
        }
        // The precipitation emitters follow the environment's effects (`FUN_1400999b0`); with
        // the game option ShowWeather (`+0x16`) off the effects and their emitters are removed.
        // ShowWeather also gates the weather lighting blend (`FUN_140098f90`, in `update`).
        let show_weather = ctx.settings.game.show_weather;
        if let (Some(a), Some(env)) = (&mut self.ambience, &self.environment) {
            for (slot, (_, ambient)) in self.weather.iter().enumerate().take(2) {
                let intensity = env.effects[slot]
                    .filter(|_| show_weather)
                    .map(|e| e.intensity);
                a.weather(ctx.audio, ctx.archive, slot, ambient.as_deref(), intensity);
            }
        }
        for (at, effect) in std::mem::take(&mut self.pending_sounds) {
            ctx.audio.play_3d(
                ctx.archive,
                &effect.replace('\\', "/"),
                crate::sound::sound_type::EFFECT,
                at,
            );
        }
        let Some(SceneView {
            camera: SceneCamera::Map(camera),
            ..
        }) = &self.view
        else {
            return;
        };
        let listener = camera.target.extend(0.0);
        // The step reads the iteration count (`+0x34`) whether or not the game is paused.
        let speed = SPEED_LEVELS[self.speed as usize] as f32;
        ctx.audio
            .update(dt, dt * speed * 6.0, listener, camera.distance);
    }

    fn map_click(&self, ctx: &mut Ctx) -> Option<glam::Vec2> {
        let size = self.town.as_ref()?.cells.size as u32;
        self.utility.map_click(ctx.ui, self.cursor, size)
    }

    /// A toolbar button's action, from a click or its key (`FUN_1400be770`).
    fn run_action(&mut self, ctx: &mut Ctx, action: Action) {
        match action {
            // `FUN_1400be770` kinds 6-9 write the simulation controller's pause flag and speed.
            Action::Pause => self.paused = true,
            Action::Play => self.paused = false,
            Action::Slower => self.speed = self.speed.saturating_sub(1),
            Action::Faster => self.speed = (self.speed + 1).min((SPEED_LEVELS.len() - 1) as u8),
            Action::Utility(template) => {
                let map_size = self.town.as_ref().map_or(0, |t| t.cells.size as u32);
                let Some(screen) = self.screen.as_mut() else {
                    return;
                };
                self.utility.toggle(
                    ctx.ui,
                    screen,
                    ctx.archive,
                    &template,
                    Some(&self.events),
                    map_size,
                );
            }
            // Toolbar kind 1 → FUN_140108030: start (or end) the build tool.
            // `FUN_140108030`: under a RestrictPlayer setup the tool is its template's.
            Action::Build(Some(template)) => {
                let template = match &self.restrict {
                    Some(r) if !r.place.is_empty() => {
                        format!("{}:resource", r.place.split(':').next().unwrap_or(""))
                    }
                    _ => template,
                };
                self.start_tool(ctx, &template)
            }
            Action::Build(None) => self.end_placement(ctx),
            Action::Options => self.open_pause_menu(ctx),
            other => log::info!("toolbar action not implemented yet: {other:?}"),
        }
    }

    /// `FUN_1401071b0`: open the pause menu (pauses effects, ambience and music).
    fn open_pause_menu(&mut self, ctx: &mut Ctx) {
        if let (None, Some(screen)) = (&self.pause_menu, self.screen.as_mut()) {
            let flags = self.town.as_ref().map_or(0, |t| t.construction.game_flags);
            self.pause_menu = crate::pause_menu::PauseMenu::open(
                ctx.ui,
                screen,
                ctx.audio,
                self.params.scenario.is_some(),
                flags,
            );
        }
    }

    /// `CloseDialog` → `FUN_1400b9130`: resume effects, ambience and music.
    fn close_pause_menu(&mut self, ctx: &mut Ctx) {
        if let (Some(menu), Some(screen)) = (self.pause_menu.take(), self.screen.as_mut()) {
            menu.close(ctx.ui, screen, ctx.audio);
        }
    }

    fn return_to_menu() -> Next {
        Next::Switch(Box::new(DialogLoaderLoop::new(
            "Dialog/Loading.rsc:resource".into(),
            1.0,
            Box::new(MenuLoop::new()),
        )))
    }
}

impl GameLoop for GameplayLoop {
    fn name(&self) -> &'static str {
        "Game/Game"
    }

    fn load(&mut self, ctx: &mut Ctx) {
        let saved = self.saved.clone();
        let world = match &saved {
            Some(s) => World::from_save(ctx.archive, s),
            None => World::generate(ctx.archive, &self.params),
        };
        match world {
            Ok(world) => {
                let lighting = Lighting::from_month(&world.environment.months[0]);
                self.weather = world.environment.weather.clone();
                // `FUN_1400d9450` builds each slot's particles from its Weather description.
                self.precipitation = self
                    .weather
                    .iter()
                    .enumerate()
                    .map(|(i, (w, _))| {
                        let r = w.as_deref()?.replace('\\', "/");
                        let data = ctx.archive.read_ref(&r).ok()?;
                        let desc = banished_formats::weather::weather(&data).ok()?;
                        Some(crate::weather::Precipitation::new(desc, 0x7ea7 + i as u32))
                    })
                    .collect();
                self.river_paths = world.river_paths.clone();
                if saved.is_none() {
                    self.new_map = Some(new_map(&world));
                }
                self.environment_months = world.environment.months;
                self.environment_template = world.environment.template.clone();
                log::info!(
                    "{} terrain ready for {} ({}, {})",
                    world.profile.name,
                    self.params.town_name,
                    self.params.climate,
                    self.params.start_condition
                );
                // `FUN_1400a87d0`: start point, structures and natural resources, drawn from
                // the game stream before anything else consumes it.
                let grid = banished_game::terrain::HeightGrid {
                    width: world.terrain.size + 1,
                    heights: &world.terrain.heights,
                };
                let town = match &saved {
                    Some(s) => StartingTown::from_save(ctx.archive, s),
                    None => StartingTown::create(
                        ctx.archive,
                        &self.params.start_condition_ref,
                        world.terrain.size / 2,
                        &grid,
                        &mut self.rng,
                    ),
                };
                // Inferred: the camera opens over the start point (not yet traced).
                let mut camera = Camera::ground();
                if let Some(town) = &town {
                    let c = (world.terrain.size / 4) as f32 - 0.5;
                    camera.target =
                        glam::Vec2::new(town.start.0 as f32 - c, town.start.1 as f32 - c);
                }
                // A save restores its camera (`FUN_14009dd90`: target `+0x1c`, then yaw,
                // pitch and distance).
                if let Some(c) = saved.as_ref().and_then(|s| s.state.camera) {
                    camera.target = glam::Vec2::new(c.target[0], c.target[1]);
                    camera.yaw = c.floats[0];
                    camera.pitch = c.floats[1];
                    camera.distance = c.floats[2];
                }
                // The rivers and lakes with the profile's River and Lake materials.
                let w = &world.water;
                let water = crate::scene_assets::WaterData::from_mesh(
                    ctx.archive,
                    &world.profile.materials[1],
                    &world.profile.materials[2],
                    &w.vertices,
                    &w.indices,
                    w.rivers.clone(),
                    w.lakes.clone(),
                );
                let mut view = SceneView::new(world.terrain, SceneCamera::Map(camera), lighting);
                // `FUN_14009bfd0`: the sky (`Models/Sky/SkyMesh.rsc`); the roads' decals follow.
                view.extras = Some(crate::scene_assets::SceneExtras {
                    sky: crate::scene_assets::SkyData::load(ctx.archive, "Models/Sky/SkyMesh.rsc"),
                    water,
                    decals: Vec::new(),
                });
                self.road_materials = crate::road_decals::materials(ctx.archive);
                self.decal_descriptions.clear();
                if let Some(town) = &town {
                    view.models = Some(town.models(ctx.archive));
                }
                self.town = town;
                // A new game's ambience regions join the entity manager's bucket 2.
                if let (None, Some(town)) = (&saved, &mut self.town) {
                    let regions = crate::ambience::region_count(
                        ctx.archive,
                        &self.params.start_condition_ref,
                        town.cells.size,
                    );
                    town.register_ambience(regions);
                }
                if let Some(town) = &mut self.town {
                    town.construction.game_flags = match &saved {
                        Some(s) => s.state.flags,
                        None if self.params.disasters => 0,
                        None => 0xf0,
                    };
                    // The trader boats' way (MapComponent `+0x1e0`'s first river).
                    town.construction.river = self.river_paths.first().cloned().unwrap_or_default();
                }
                self.view = Some(view);
            }
            Err(e) => log::error!("world generation failed: {e}"),
        }
        // `FUN_14009bfd0` → `FUN_140099300`: bind the environment to the selected climate.
        let reference = self.params.climate_ref.replace('\\', "/");
        match ctx
            .archive
            .read_ref(&reference)
            .and_then(|d| climate_generator(&d))
        {
            Ok(c) => {
                let mut env = Environment::new(Climate::from(&c));
                // The weather list's descriptions (`Weather +0x5c`: fade time).
                for (slot, (weather, _)) in self.weather.iter().enumerate().take(2) {
                    env.fade_times[slot] = weather
                        .as_deref()
                        .and_then(|r| ctx.archive.read_ref(&r.replace('\\', "/")).ok())
                        .and_then(|d| banished_formats::weather::weather(&d).ok())
                        .map(|w| w.fade_time);
                }
                // Disasters off stop tornadoes (`Game +0xd0` bit 7).
                env.game_flags = if self.params.disasters { 0 } else { 0xf0 };
                if let Some(saved) = &self.saved {
                    env.game_flags = saved.state.flags;
                    saved.restore_environment(&mut env);
                }
                self.environment = Some(env);
            }
            Err(e) => log::error!("{reference}: {e}"),
        }
    }

    fn startup(&mut self, ctx: &mut Ctx) {
        self.build_hud(
            ctx.ui,
            ctx.archive,
            ctx.registry,
            &ctx.settings.keymap,
            ctx.settings.game.autosave_interval,
        );
        self.achievements = crate::achievements::load(ctx.archive);
        self.achievement_manager = crate::achievements::AchievementManager::new(
            &self.achievements,
            &ctx.settings.persistence,
        );
        if let Some(saved) = &self.saved {
            self.achievement_manager.restore(&self.achievements, saved);
        }
        self.start_scenario(ctx);
    }

    fn update(&mut self, ctx: &mut Ctx, dt: f32) -> Next {
        {
            self.finish_save(ctx);
            self.update_autosave(ctx, dt);
            if let Some(next) = self.update_save_dialog(ctx, dt) {
                return next;
            }
        }
        if ctx.ui.screen[0] > 0 && ctx.ui.screen[1] > 0 {
            self.aspect = ctx.ui.screen[0] as f32 / ctx.ui.screen[1] as f32;
        }
        self.fill_tooltip(ctx);
        if let Some(toolbar) = &mut self.toolbar {
            toolbar.set_keymap(ctx.ui, &ctx.settings.keymap);
            toolbar.relayout(ctx.ui);
        }
        // FUN_140107b70: while the application is paused (unfocused) the controller opens the
        // pause menu (game option `+0x18`, on by default) instead of running.
        if ctx.app_paused && ctx.settings.game.auto_pause && self.pause_menu.is_none() {
            self.open_pause_menu(ctx);
        }
        self.show_celsius = ctx.settings.game.show_celsius;
        if let Some(options) = &mut self.options {
            options.update();
        }
        // `UserInterface::PausesGame`: FUN_14009b850 skips the simulation and the controller.
        let pauses_game = self.pause_menu.is_some();
        // Controller FUN_140107b70 → FUN_140104390: the camera eases toward its zoom and pitch
        // targets every frame.
        if ctx.app_paused {
            self.held = [false; keys::COUNT];
        }
        if let Some(view) = self.view.as_mut().filter(|_| !pauses_game) {
            if let SceneCamera::Map(camera) = &mut view.camera {
                // `FUN_140104390`: held rotation keys, then movement from the screen edges
                // (game option EdgeScroll) overridden by the held movement keys.
                let held = |id: usize| self.held[id];
                let turn = held(keys::ROTATE_RIGHT) as i32 - held(keys::ROTATE_LEFT) as i32;
                camera.turn(turn as f32, dt);
                let mut d = glam::Vec2::ZERO;
                let [w, h] = ctx.ui.screen;
                if ctx.settings.game.edge_scroll && self.cursor_seen && w > 0 && h > 0 {
                    let (x, y) = self.cursor;
                    if x < EDGE {
                        d.x = -1.0;
                    }
                    if w - EDGE - 1 < x && x < w {
                        d.x = 1.0;
                    }
                    if y < EDGE {
                        d.y = -1.0;
                    }
                    if h - EDGE - 1 < y && y < h {
                        d.y = 1.0;
                    }
                }
                if held(keys::LEFT) {
                    d.x = -1.0;
                }
                if held(keys::RIGHT) {
                    d.x = 1.0;
                }
                if held(keys::FORWARD) {
                    d.y = -1.0;
                }
                if held(keys::BACK) {
                    d.y = 1.0;
                }
                camera.travel(d, dt, &view.terrain);
                camera.update(&view.terrain);
            }
        }
        // PlacementTool slot +0x38 runs every frame: ghost, validity and the CreateBar.
        if self.placement.is_some() && !pauses_game {
            self.refresh_placement(ctx);
        }
        let mut models_changed = false;
        let mut iterations = 0;
        if !self.paused && !pauses_game {
            for _ in 0..SPEED_LEVELS[self.speed as usize] {
                models_changed |= self.iterate(ctx.archive, dt);
                iterations += 1;
            }
        }
        if !pauses_game {
            self.update_goal(ctx, dt, ticks(dt * iterations as f32));
            self.update_achievements(ctx, ticks(dt * iterations as f32));
            self.force_restricted_tool(ctx);
        }
        self.refresh_tutorial_ui(ctx.ui);
        if let Some(i) = self.next_scenario.take() {
            if let Some((_, s)) = tutorial::scenarios(ctx.archive).into_iter().nth(i) {
                let game = tutorial::scenario_game(ctx.ui, ctx.archive, ctx.global.as_ref(), &s, i);
                return Next::Switch(Box::new(DialogLoaderLoop::new(
                    format!("Dialog/Loading.rsc:gameLoad{}", ctx.rng.rand_below(12)),
                    0.5,
                    Box::new(game),
                )));
            }
        }
        self.update_follow(ctx);
        self.refresh_decals(ctx.archive);
        self.refresh_particles(ctx.archive, dt * iterations as f32);
        self.refresh_weather(ctx, dt * iterations as f32);
        // FUN_14009b850 runs this phase once after the speed loop, even at speed zero.
        if let (Some(town), Some(view), false) = (&mut self.town, &mut self.view, pauses_game) {
            let grid = banished_game::terrain::HeightGrid {
                width: view.terrain.size + 1,
                heights: &view.terrain.heights,
            };
            models_changed |= town.smooth(&grid, &mut self.rng);
            self.ambience_stale |= models_changed;
            if models_changed {
                if let Some(models) = &mut view.models {
                    town.refresh_models(ctx.archive, models);
                }
            }
        }
        self.utility.update(ctx.ui, &self.status_values());
        if let (Some(notify), Some(screen)) = (self.notify.as_mut(), self.screen.as_mut()) {
            let width = self.toolbar.as_ref().map_or(0, |t| t.width(ctx.ui));
            if notify.update(ctx.ui, screen, &self.events, dt, width) {
                // `Scene::PlayEffect(WarningEffect, 1)`.
                ctx.audio.play(
                    ctx.archive,
                    crate::sound::global::WARNING,
                    crate::sound::sound_type::UI,
                );
            }
        }
        if let Some(town) = &self.town {
            let town_ctx = crate::town_hall_ui::TownContext {
                stock_counts: &self.stock_warnings.count,
                statistics: &self.statistics,
            };
            self.entities.refresh_with(
                ctx.ui,
                ctx.archive,
                town,
                &self.stock_warnings.limits,
                &self.params.town_name,
                Some(&town_ctx),
            );
        }
        // TradeUI's open (`FUN_140087f40`) and close (`FUN_140087ed0`): the post holds its
        // merchant while its window is open.
        let open = self.entities.trade_windows();
        if open != self.trade_held {
            if let Some(town) = &mut self.town {
                use banished_game::{professions::Command, trade::TradeSetting};
                for &e in self.trade_held.iter().filter(|e| !open.contains(e)) {
                    town.construction.commands.push(Command::Trade {
                        entity: e,
                        setting: TradeSetting::Held(false),
                    });
                }
                for &e in open.iter().filter(|e| !self.trade_held.contains(e)) {
                    town.construction.commands.push(Command::Trade {
                        entity: e,
                        setting: TradeSetting::Held(true),
                    });
                }
            }
            self.trade_held = open;
        }
        self.position_entity_window(ctx);
        if let (Some(town), Some(view)) = (self.town.as_ref(), self.view.as_mut()) {
            let grid = banished_game::terrain::HeightGrid {
                width: view.terrain.size + 1,
                heights: &view.terrain.heights,
            };
            view.highlight =
                self.highlight
                    .geometry(ctx.archive, town, &grid, self.selection.selected);
            view.status_icons = town.status_icons();
            // Repainted when entities change (MapComponent activation/removal).
            if models_changed || view.minimap_overlay.is_none() {
                let revision = view.minimap_overlay.as_ref().map_or(0, |o| o.0) + 1;
                view.minimap_overlay =
                    Some((revision, town.cells.size as u32, town.minimap_overlay()));
            }
            for r in town.icon_sheet_refs() {
                if !view.icon_sheets.contains_key(&r) {
                    if let Some(sheet) = crate::status_icons::load_sheet(ctx.archive, &r) {
                        view.icon_sheets.insert(r, sheet);
                    }
                }
            }
        }
        self.log_life_events(ctx);
        self.log_site_events(ctx);
        // AutoSlow (settings `+0x17`): a fire, a blight, a tornado or a shortage
        // (`FUN_14009a2d0`, `FUN_14009aa50`, `FUN_14009ac20` and the disasters themselves) sets
        // the game speed (`Game +0x120 → +0x34`) back to 1.
        if std::mem::take(&mut self.events.slow_request) && ctx.settings.game.auto_slow {
            self.speed = 0;
        }
        self.move_cooldown -= dt;
        self.update_audio(ctx, dt);
        let month = self.month();
        let into_month = (self.calendar.seconds() - month as f32 * MONTH_SECONDS) / MONTH_SECONDS;
        if let (Some(view), Some(from)) = (self.view.as_mut(), self.environment_months.get(month)) {
            // `FUN_140098f90`: blend this month's lighting towards the next.
            let to = &self.environment_months[(month + 1) % self.environment_months.len()];
            view.lighting = Lighting::between_months(from, to, into_month);
            // With ShowWeather, each running effect pulls the lighting towards its own.
            if let (true, Some(env)) = (ctx.settings.game.show_weather, &self.environment) {
                for (slot, p) in self.precipitation.iter().enumerate() {
                    let (Some(p), Some(Some(e))) = (p, env.effects.get(slot)) else {
                        continue;
                    };
                    view.lighting.toward_weather(&p.desc, e.intensity);
                }
            }
            if let Some(env) = &self.environment {
                let s = env.season;
                view.lighting.user = glam::Vec4::new(s.snow, s.autumn, s.leaf_loss, s.rain);
            }
        }
        Next::Stay
    }

    fn event(&mut self, ctx: &mut Ctx, event: Event) -> Next {
        if let (Some(d), Some(screen)) = (self.save_dialog.as_mut(), self.screen.as_mut()) {
            if let Event::Clicked(id) = event {
                let outcome = d.click(ctx.ui, screen, ctx.audio, ctx.archive, id);
                if let Some(next) = self.save_outcome(ctx, outcome) {
                    return next;
                }
            }
            return Next::Stay;
        }
        if let (Some(options), Some(screen)) = (&mut self.options, &mut self.screen) {
            if options.event(ctx, screen, event) == crate::options::Outcome::Closed {
                self.options = None;
            }
            return Next::Stay;
        }
        if let (Event::Clicked(id), Some(t)) = (event, self.tutorial.as_mut()) {
            t.pressed.push(id);
        }
        // TutorialUI's quit (`FUN_1400e8bb0`): back to the main menu.
        if let Event::Clicked(id) = event {
            if self.tutorial_quit(id) {
                return Self::return_to_menu();
            }
        }
        if self.utility.help_event(ctx.ui, event) {
            return Next::Stay;
        }
        // A combo in a building's window (event `0x14`).
        if let Event::Selected(id, index) = event {
            if self.entities.town_hall_select(ctx.ui, &self.statistics, id) {
                return Next::Stay;
            }
            let chosen = self
                .town
                .as_ref()
                .and_then(|town| self.entities.select_item(ctx.ui, town, id, index));
            if let (Some(WorkPlaceClick::Command(c)), Some(town)) = (chosen, &mut self.town) {
                town.construction.commands.push(c);
            }
            return Next::Stay;
        }
        let (Event::Clicked(id) | Event::Changed(id)) = event else {
            return Next::Stay;
        };
        // The Achievement and Goal dialogs over the pause menu (OK closes them).
        if let Some(d) = &self.menu_dialog {
            let root = match d {
                MenuDialog::Achievements(a) => a.closes(id).then_some(a.root),
                MenuDialog::Goal(g) => g.closes(id).then_some(g.root),
                MenuDialog::Map(m) => {
                    match m.click(ctx.ui, id, self.town_names.as_ref(), &mut self.rng) {
                        crate::map_params::MapParamsClick::Rename(name) => {
                            self.params.town_name = name;
                            Some(m.root)
                        }
                        crate::map_params::MapParamsClick::Close => Some(m.root),
                        crate::map_params::MapParamsClick::None => None,
                    }
                }
            };
            if let (Some(root), Some(screen)) = (root, self.screen.as_mut()) {
                screen.remove_root(ctx.ui, root);
                self.menu_dialog = None;
            }
            return Next::Stay;
        }
        if let Some(menu) = &self.pause_menu {
            // `FUN_1400b9020`.
            match menu.click(ctx.ui, id) {
                crate::pause_menu::PauseClick::Resume => self.close_pause_menu(ctx),
                crate::pause_menu::PauseClick::Menu => {
                    self.close_pause_menu(ctx);
                    return Self::return_to_menu();
                }
                crate::pause_menu::PauseClick::Quit => return Next::Quit,
                crate::pause_menu::PauseClick::Options => {
                    if let Some(screen) = &mut self.screen {
                        self.options = crate::options::OptionsDialog::open(ctx, screen);
                    }
                }
                crate::pause_menu::PauseClick::Achievement => {
                    if let Some(screen) = &mut self.screen {
                        let awarded = self.achievement_manager.awarded.clone();
                        if let Some(d) = crate::achievement_ui::AchievementDialog::open(
                            ctx.ui,
                            screen,
                            &self.achievements,
                        ) {
                            d.refresh(ctx.ui, &awarded);
                            self.menu_dialog = Some(MenuDialog::Achievements(d));
                        }
                    }
                }
                crate::pause_menu::PauseClick::Goal => {
                    if let Some(screen) = &mut self.screen {
                        let goal = self.goal.as_ref().map(|g| (&g.achievement, &g.instance));
                        self.menu_dialog =
                            crate::achievement_ui::GoalDialog::open(ctx.ui, screen, goal)
                                .map(MenuDialog::Goal);
                    }
                }
                crate::pause_menu::PauseClick::Map => {
                    if self.town_names.is_none() {
                        self.town_names = ctx
                            .archive
                            .read("Game/Names/Cities_resource.crs")
                            .and_then(|b| banished_formats::names::NameDatabase::read(&b))
                            .map_err(|e| log::error!("town names: {e}"))
                            .ok();
                    }
                    let flags = self.town.as_ref().map_or(0, |t| t.construction.game_flags);
                    if let Some(screen) = &mut self.screen {
                        self.menu_dialog = crate::map_params::MapParamsDialog::open(
                            ctx.ui,
                            screen,
                            &self.params,
                            flags,
                        )
                        .map(MenuDialog::Map);
                    }
                }
                crate::pause_menu::PauseClick::Load | crate::pause_menu::PauseClick::Save => {
                    if let Some(screen) = &mut self.screen {
                        let loading =
                            matches!(menu.click(ctx.ui, id), crate::pause_menu::PauseClick::Load);
                        self.save_dialog =
                            crate::save_dialog::SaveLoadDialog::open(ctx.ui, screen, loading, true);
                    }
                }
                crate::pause_menu::PauseClick::Other if menu.contains(ctx.ui, id) => {
                    log::info!(
                        "pause menu: {} is not implemented yet",
                        ctx.ui.node(id).name
                    );
                }
                crate::pause_menu::PauseClick::Other => {}
            }
            return Next::Stay;
        }
        // The Follow dialog's close button (`FUN_140069b90`, id 100).
        if self.follow.as_ref().is_some_and(|f| f.is_close(id)) {
            self.close_follow(ctx);
            return Next::Stay;
        }
        if let (Some(notify), Some(screen)) = (self.notify.as_mut(), self.screen.as_mut()) {
            if let Some(goto) = notify.click(ctx.ui, screen, &self.events, id) {
                if let Some(cell) = goto {
                    self.go_to(cell);
                }
                return Next::Stay;
            }
        }
        let values = self.status_values();
        let (Some(screen), Some(toolbar)) = (self.screen.as_mut(), self.toolbar.as_mut()) else {
            return Next::Stay;
        };
        let Some(action) = toolbar.click(ctx.ui, screen, id) else {
            match self.utility.click(ctx.ui, id, &self.events, &values) {
                Some(UtilityClick::ProfessionCount(profession, count)) => {
                    if let Some(town) = &mut self.town {
                        town.construction.commands.push(
                            banished_game::professions::Command::SetCount { profession, count },
                        );
                    }
                    return Next::Stay;
                }
                Some(UtilityClick::Notify(i, on)) => {
                    self.events.notify[i] = on;
                    return Next::Stay;
                }
                Some(UtilityClick::StockLimit(slot, limit)) => {
                    self.stock_warnings.limits[slot] = limit;
                    return Next::Stay;
                }
                Some(UtilityClick::GoTo(cell)) => {
                    self.go_to(cell);
                    return Next::Stay;
                }
                Some(UtilityClick::ShowCitizen(citizen)) => {
                    // `FUN_140108510`: centre the camera on the citizen and select it.
                    if let (Some(town), Some(view), Some(screen)) =
                        (self.town.as_ref(), self.view.as_mut(), self.screen.as_mut())
                    {
                        let entity = Entity::Citizen(citizen);
                        if let SceneCamera::Map(camera) = &mut view.camera {
                            camera.target = entity.position(town).truncate();
                        }
                        self.selection.selected = Some(entity);
                        self.entities
                            .select(ctx.ui, screen, ctx.archive, town, entity);
                    }
                    return Next::Stay;
                }
                Some(UtilityClick::Handled) => return Next::Stay,
                None => {}
            }
            if self.entities.toggle_page(ctx.ui, id) {
                return Next::Stay;
            }
            let clicked = self
                .town
                .as_ref()
                .and_then(|town| self.entities.click(ctx.ui, town, id));
            match clicked {
                Some(WorkPlaceClick::Command(command)) => {
                    if let Some(town) = &mut self.town {
                        town.construction.commands.push(command);
                    }
                    return Next::Stay;
                }
                // UpgradeUI (`FUN_14008c160`): the template is loaded for the rebuild.
                Some(WorkPlaceClick::Upgrade { entity, template }) => {
                    if let Some(town) = &mut self.town {
                        if let Some(t) = town.load_template(ctx.archive, &template) {
                            town.site_template(ctx.archive, t);
                            town.construction.commands.push(
                                banished_game::professions::Command::Upgrade {
                                    entity,
                                    template: t,
                                },
                            );
                        }
                    }
                    return Next::Stay;
                }
                // ResourceLimitUI (`FUN_14007ccb0`): the slot's limit.
                Some(WorkPlaceClick::StockLimit { slot, limit }) => {
                    self.stock_warnings.limits[slot] = limit;
                    return Next::Stay;
                }
                Some(WorkPlaceClick::ShowBuilding(building)) => {
                    self.entities.pin_containing(ctx.ui, id);
                    if let (Some(town), Some(view)) = (self.town.as_ref(), self.view.as_mut()) {
                        let entity = Entity::Placed(building);
                        if let SceneCamera::Map(camera) = &mut view.camera {
                            camera.target = entity.position(town).truncate();
                        }
                        self.selection.selected = Some(entity);
                        self.entities
                            .select(ctx.ui, screen, ctx.archive, town, entity);
                    }
                    return Next::Stay;
                }
                Some(WorkPlaceClick::ShowWorker(citizen)) => {
                    // `FUN_140108510`: pin the asking window, centre the camera on the entity,
                    // then select it (`FUN_140102880`).
                    self.entities.pin_containing(ctx.ui, id);
                    if let (Some(town), Some(view)) = (self.town.as_ref(), self.view.as_mut()) {
                        let entity = Entity::Citizen(citizen);
                        if let SceneCamera::Map(camera) = &mut view.camera {
                            camera.target = entity.position(town).truncate();
                        }
                        self.selection.selected = Some(entity);
                        self.entities
                            .select(ctx.ui, screen, ctx.archive, town, entity);
                    }
                    return Next::Stay;
                }
                None => {}
            }
            // The host dialog's close button (`Dialog/StandardDialog.rsc`).
            if ctx.ui.node(id).name == "buttonClose" {
                self.utility.close_containing(ctx.ui, screen, id);
                if self.entities.close_containing(ctx.ui, screen, id) == self.selection.selected {
                    self.selection.selected = None;
                }
            } else if ctx.ui.node(id).name == "checkPin" {
                self.entities.pin(ctx.ui, id);
            } else if matches!(
                ctx.ui.node(id).name.as_str(),
                "buttonProfession" | "buttonHome"
            ) {
                // CitizenUI event `0x12`: ids `0x227` (`FUN_140065330`: the school, else the
                // workplace) and `0x226` (`FUN_140065270`: the home) open that building through
                // `FUN_140108510`: the asking window pinned, the camera on it, selected.
                let profession = ctx.ui.node(id).name == "buttonProfession";
                let target = match (self.entities.entity_containing(ctx.ui, id), &self.town) {
                    (Some(Entity::Citizen(c)), Some(town)) => {
                        let s = &town.agents.states[c];
                        if profession {
                            s.school.or(s.workplace)
                        } else {
                            s.home
                        }
                    }
                    _ => None,
                };
                if let (Some(building), Some(town), Some(view)) =
                    (target, self.town.as_ref(), self.view.as_mut())
                {
                    self.entities.pin_containing(ctx.ui, id);
                    let entity = Entity::Placed(building);
                    if let SceneCamera::Map(camera) = &mut view.camera {
                        camera.target = entity.position(town).truncate();
                    }
                    self.selection.selected = Some(entity);
                    self.entities
                        .select(ctx.ui, screen, ctx.archive, town, entity);
                }
            } else if ctx.ui.node(id).name == "buttonFollow" {
                // CitizenUI event `0x12` id `0x228` (`FUN_140065080`).
                if let (Some(Entity::Citizen(c)), Some(owner), Some(town)) = (
                    self.entities.entity_containing(ctx.ui, id),
                    self.entities.root_containing(ctx.ui, id),
                    self.town.as_ref(),
                ) {
                    if let Some(old) = self.follow.take() {
                        old.close(ctx.ui, screen);
                    }
                    self.follow = crate::follow::Follow::open(ctx.ui, screen, town, c, owner);
                }
            } else if ctx.ui.node(id).name == "buttonCenter" {
                // EntityUI event `0x12` id `0x65` (`FUN_1400671d0`).
                if let (Some(entity), Some(town), Some(view)) = (
                    self.entities.entity_containing(ctx.ui, id),
                    self.town.as_ref(),
                    self.view.as_mut(),
                ) {
                    if let SceneCamera::Map(camera) = &mut view.camera {
                        camera.target = center_target(town, entity);
                    }
                }
            }
            return Next::Stay;
        };
        self.run_action(ctx, action);
        self.update_hud(ctx.ui);
        Next::Stay
    }

    fn raw(&mut self, ctx: &mut Ctx, input: RawInput) -> Next {
        match input {
            RawInput::Action(id) if id < keys::COUNT => self.held[id] = true,
            RawInput::ActionReleased(id) if id < keys::COUNT => self.held[id] = false,
            RawInput::Cursor(..) => self.cursor_seen = true,
            _ => {}
        }
        // The Save and Load dialog: Escape is buttonCancel's action 8.
        if let (Some(d), Some(screen)) = (self.save_dialog.as_mut(), self.screen.as_mut()) {
            if let RawInput::Escape = input {
                let outcome = d.escape(ctx.ui, screen);
                if let Some(next) = self.save_outcome(ctx, outcome) {
                    return next;
                }
            }
            return Next::Stay;
        }
        // While the pause menu is open the controller does not run; Escape is buttonResume's
        // action 8.
        if let (Some(options), Some(screen)) = (&mut self.options, &mut self.screen) {
            let outcome = match input {
                RawInput::Released(binding) => {
                    options.released(ctx, screen, binding);
                    crate::options::Outcome::Stay
                }
                RawInput::Escape => options.escape(ctx, screen),
                RawInput::Cursor(x, y) => {
                    self.cursor = (x, y);
                    crate::options::Outcome::Stay
                }
                _ => crate::options::Outcome::Stay,
            };
            if outcome == crate::options::Outcome::Closed {
                self.options = None;
            }
            return Next::Stay;
        }
        if self.pause_menu.is_some() {
            match input {
                RawInput::Escape => self.close_pause_menu(ctx),
                RawInput::Cursor(x, y) => self.cursor = (x, y),
                _ => {}
            }
            return Next::Stay;
        }
        if self.drag_window(ctx, &input) {
            return Next::Stay;
        }
        match input {
            RawInput::Cursor(x, y) => {
                self.cursor = (x, y);
                self.fill_tooltip(ctx);
                if self.placement.is_some() {
                    self.refresh_placement(ctx);
                }
                if self.removal.is_some() {
                    self.removal_input(ctx, DragInput::Move);
                }
                return Next::Stay;
            }
            RawInput::Release => {
                if self.removal.is_some() {
                    self.removal_input(ctx, DragInput::Release);
                }
                return Next::Stay;
            }
            // StructureLeft / StructureRight turn the RotationSetting through 0..3.
            // (FencedTool `FUN_1400e20b0` turns the same setting for its shelter.)
            RawInput::Action(keys::STRUCTURE_LEFT)
                if self.placement.is_some() || self.removal.is_some() =>
            {
                self.rotation = (self.rotation + 3) % 4;
                self.refresh_placement(ctx);
                return Next::Stay;
            }
            RawInput::Action(keys::STRUCTURE_RIGHT)
                if self.placement.is_some() || self.removal.is_some() =>
            {
                self.rotation = (self.rotation + 1) % 4;
                self.refresh_placement(ctx);
                return Next::Stay;
            }
            RawInput::Action(keys::STRUCTURE_TYPE) => {
                if let Some(tool) = &mut self.placement {
                    tool.next_variant();
                    self.refresh_placement(ctx);
                }
                return Next::Stay;
            }
            RawInput::Click if self.removal.is_some() => {
                self.removal_input(ctx, DragInput::Press);
                return Next::Stay;
            }
            RawInput::Click => {
                if let (Some(tool), Some(town)) = (self.placement.as_mut(), self.town.as_mut()) {
                    // `FUN_140102b20`: under a RestrictPlayer setup, only on its cells, turned.
                    let allowed =
                        Self::restrict_allows(self.restrict.as_ref(), tool, self.rotation);
                    // `FUN_1401055c0` / `FUN_140105610`: thunk on placement, error otherwise.
                    let placed = if allowed {
                        tool.place(ctx.archive, town, self.rotation)
                    } else {
                        None
                    };
                    let sound = match placed {
                        Some(id) => {
                            log::info!("placed {} as entity {id}", tool.reference);
                            crate::sound::global::PLACEMENT_THUNK
                        }
                        None => {
                            log::info!("{}: cannot build here", tool.reference);
                            crate::sound::global::PLACEMENT_ERROR
                        }
                    };
                    ctx.audio
                        .play(ctx.archive, sound, crate::sound::sound_type::UI);
                    self.refresh_placement(ctx);
                } else if let Some(target) = self.map_click(ctx) {
                    // MapUI: move the camera (`FUN_14006d1b0`).
                    if let Some(SceneView {
                        camera: SceneCamera::Map(c),
                        ..
                    }) = &mut self.view
                    {
                        c.target = target;
                    }
                } else if self.path_tool.is_some() {
                    self.path_click(ctx);
                } else {
                    self.select_at_cursor(ctx);
                    self.position_entity_window(ctx);
                }
                return Next::Stay;
            }
            // DragTool `+0x48` (`FUN_140101b10`): Escape first stops a drag in progress.
            RawInput::Escape if self.removal.as_mut().is_some_and(|t| t.cancel()) => {
                return Next::Stay;
            }
            // `FUN_140104bb0`: the tool stays while a RestrictPlayer setup holds.
            RawInput::Escape if self.restrict.as_ref().is_some_and(|r| !r.place.is_empty()) => {
                return Next::Stay;
            }
            // `FUN_140069bb0`: Escape closes the Follow dialog.
            RawInput::Escape if self.follow.is_some() => {
                self.close_follow(ctx);
                return Next::Stay;
            }
            RawInput::Escape
                if self.placement.is_some()
                    || self.removal.is_some()
                    || self.path_tool.is_some() =>
            {
                self.end_placement(ctx);
                if let Some(toolbar) = &self.toolbar {
                    toolbar.clear_build(ctx.ui);
                }
                return Next::Stay;
            }
            // `FUN_14009b850`: the screenshot actions.
            RawInput::Action(keys::SCREENSHOT_NO_UI) => {
                self.screenshot = Some(false);
                return Next::Stay;
            }
            RawInput::Action(keys::SCREENSHOT) => {
                self.screenshot = Some(true);
                return Next::Stay;
            }
            RawInput::Action(keys::TOGGLE_PAUSE) => {
                self.paused = !self.paused;
                self.update_hud(ctx.ui);
                return Next::Stay;
            }
            // Toolbar0..8, Tool0..9, Camera0..4: the shown button holding that key.
            RawInput::Action(a @ keys::TOOLBAR0..=keys::CAMERA4) => {
                let (Some(screen), Some(toolbar)) = (self.screen.as_mut(), self.toolbar.as_mut())
                else {
                    return Next::Stay;
                };
                if let Some(Some(action)) = toolbar.press_action(ctx.ui, screen, a as i32) {
                    self.run_action(ctx, action);
                }
                self.update_hud(ctx.ui);
                return Next::Stay;
            }
            _ => {}
        }
        let Some(SceneView {
            camera: SceneCamera::Map(camera),
            terrain,
            ..
        }) = &mut self.view
        else {
            return Next::Stay;
        };
        match input {
            RawInput::Drag(dx, dy) => camera.rotate(dx, dy),
            RawInput::PanDrag(dx, dy) => camera.pan_drag(dx, dy, terrain),
            RawInput::Wheel(steps) => camera.zoom(steps),
            RawInput::Action(keys::ZOOM_IN) => camera.zoom(1.0),
            RawInput::Action(keys::ZOOM_OUT) => camera.zoom(-1.0),
            RawInput::Action(keys::PITCH_UP) => camera.pitch(1.0),
            RawInput::Action(keys::PITCH_DOWN) => camera.pitch(-1.0),
            // Toolbar Options (kind 3) holds the fixed action 8, Escape.
            RawInput::Escape => self.open_pause_menu(ctx),
            _ => {}
        }
        Next::Stay
    }

    fn screen(&self) -> Option<&Screen> {
        self.screen.as_ref()
    }

    /// Capture scripts: `event KIND [CELL]` posts an event at severity 3; `open TEMPLATE`
    /// toggles a utility window.
    fn script(&mut self, ctx: &mut Ctx, command: &str) -> bool {
        let mut words = command.split_whitespace();
        match words.next() {
            Some("event") => {}
            // `save PATH`: write the game (one loaded from a save) to `PATH` once the next frame
            // has drawn its preview.
            #[cfg(not(target_arch = "wasm32"))]
            Some("save") => {
                let Some(path) = words.next() else {
                    return false;
                };
                self.pending_save = Some((SaveTarget::File(path.into()), None, None));
                return true;
            }
            // `pause`: open the pause menu (captures).
            Some("pause") => {
                self.open_pause_menu(ctx);
                return self.pause_menu.is_some();
            }
            Some("open") => {
                let (Some(template), Some(screen)) = (words.next(), self.screen.as_mut()) else {
                    return false;
                };
                let map_size = self.town.as_ref().map_or(0, |t| t.cells.size as u32);
                self.utility.toggle(
                    ctx.ui,
                    screen,
                    ctx.archive,
                    template,
                    Some(&self.events),
                    map_size,
                );
                return true;
            }
            // `place TEMPLATE`: place a building on the first clear spot near the start.
            Some("place") => {
                let (Some(template), Some(town), Some(view)) =
                    (words.next(), self.town.as_mut(), self.view.as_ref())
                else {
                    return false;
                };
                let grid = banished_game::terrain::HeightGrid {
                    width: view.terrain.size + 1,
                    heights: &view.terrain.heights,
                };
                let Some(mut tool) =
                    PlacementTool::start(ctx.archive, town, template, &mut self.rng)
                else {
                    return false;
                };
                let near = town.start;
                if tool.find_clear_spot(town, &grid, near).is_none() {
                    return false;
                }
                let placed = tool.place(ctx.archive, town, 0);
                let cell = placed.map(|e| (town.placed[e].cell, town.start));
                log::info!("script placed {template} as {placed:?} (cell, start: {cell:?})");
                if let Some(models) = self.view.as_mut().and_then(|v| v.models.as_mut()) {
                    if let Some(town) = &self.town {
                        town.refresh_models(ctx.archive, models);
                    }
                }
                return placed.is_some();
            }
            // `remove TEMPLATE RADIUS`: drag a removal tool over the square around the start.
            Some("remove") => {
                let (Some(template), Some(r), Some(town), Some(view)) = (
                    words.next(),
                    words.next().and_then(|w| w.parse::<i32>().ok()),
                    self.town.as_mut(),
                    self.view.as_ref(),
                ) else {
                    return false;
                };
                let Some(mut tool) = RemovalTool::start(ctx.archive, town, template) else {
                    return false;
                };
                let grid = banished_game::terrain::HeightGrid {
                    width: view.terrain.size + 1,
                    heights: &view.terrain.heights,
                };
                let n = town.cells.size as i32;
                let (x, y) = (town.start.0 as i32, town.start.1 as i32);
                let clamp = |v: i32| v.clamp(0, n - 1);
                let a = clamp(y - r) * n + clamp(x - r);
                let b = clamp(y + r) * n + clamp(x + r);
                tool.drag(ctx.archive, town, &grid, a, b);
                let roads: Vec<i32> = (0..town.cells.road_kind.len() as i32)
                    .filter(|&i| town.cells.road_kind[i as usize] != 0xff)
                    .collect();
                log::info!(
                    "script dragged {template} from {a} to {b} (start {:?}); {} road cells: {:?}",
                    town.start,
                    roads.len(),
                    &roads[..roads.len().min(8)]
                );
                return true;
            }
            // `tool TEMPLATE`: start a template's tool, as its toolbar button does.
            // `screenshot [ui]`: the screenshot actions (`0x21`, `0x22` with `ui`).
            Some("screenshot") => {
                self.screenshot = Some(words.next() == Some("ui"));
                return true;
            }
            // `rotate`: StructureRight (T), one step clockwise.
            Some("rotate") => {
                self.rotation = (self.rotation + 1) % 4;
                self.refresh_placement(ctx);
                return true;
            }
            Some("tool") => {
                let Some(template) = words.next() else {
                    return false;
                };
                self.start_tool(ctx, template);
                return self.placement.is_some()
                    || self.removal.is_some()
                    || self.path_tool.is_some();
            }
            // `select`: select the most recently placed entity (EditTool::Select).
            Some("select") => {
                let (Some(town), Some(screen)) = (self.town.as_ref(), self.screen.as_mut()) else {
                    return false;
                };
                let Some(p) = town.placed.iter().rposition(|p| p.alive) else {
                    return false;
                };
                let entity = Entity::Placed(p);
                self.selection.selected = Some(entity);
                self.entities
                    .select(ctx.ui, screen, ctx.archive, town, entity);
                return true;
            }
            // `profession INDEX COUNT`: the ProfessionUI spinner.
            Some("profession") => {
                let (Some(profession), Some(count), Some(town)) = (
                    words.next().and_then(|w| w.parse().ok()),
                    words.next().and_then(|w| w.parse().ok()),
                    self.town.as_mut(),
                ) else {
                    return false;
                };
                town.construction
                    .commands
                    .push(banished_game::professions::Command::SetCount { profession, count });
                return true;
            }
            _ => return false,
        }
        let Some(kind) = words.next().and_then(|k| k.parse().ok()) else {
            return false;
        };
        let cell = words.next().and_then(|c| c.parse().ok()).unwrap_or(-1);
        let (month, year) = (self.month(), self.calendar.year as i32);
        self.events
            .add(kind, 3, cell, month, year, Default::default(), false);
        true
    }

    /// `FUN_14009b850`: the UI's cursor (`UserInterface::Update` → `GetMouseCursorIndex`)
    /// while it holds the pointer (`+0xd9`) or pauses the game; otherwise the tool's (slot
    /// `+0x60` through `FUN_140107b70`): EditTool 1 (Hand, `FUN_14010ad30`), PlacementTool and
    /// every DragTool 5 (Place, `FUN_140103cd0`, `FUN_140101ea0`), PathTool 4 (Query,
    /// `FUN_1400e3de0`).
    fn cursor(&self, ui: &Ui, (x, y): (i32, i32)) -> usize {
        if ui.pointer_over_interactive() || self.pause_menu.is_some() {
            return self
                .screen
                .as_ref()
                .map_or(0, |s| s.cursor_at(ui, x, y).max(0) as usize);
        }
        if self.placement.is_some() || self.removal.is_some() {
            5
        } else if self.path_tool.is_some() {
            4
        } else {
            1
        }
    }

    fn take_screenshot(&mut self) -> Option<bool> {
        self.screenshot.take()
    }

    fn wants_save_preview(&self) -> bool {
        self.pending_save
            .as_ref()
            .is_some_and(|(_, _, i)| i.is_none())
    }

    fn save_preview(&mut self, image: Vec<u8>, minimap: Option<(u32, Vec<u32>)>) {
        if let Some((_, _, slot)) = &mut self.pending_save {
            *slot = Some(image);
            self.save_minimap = minimap;
        }
    }

    fn view(&self) -> View<'_> {
        self.view.as_ref().map_or(View::None, View::World)
    }
}

/// Where a save goes: the save store under a name (the Save dialog, autosave), or a file
/// (the `save PATH` script command).
#[derive(Debug)]
enum SaveTarget {
    Store(String),
    #[cfg(not(target_arch = "wasm32"))]
    File(std::path::PathBuf),
}

/// A generated world's terrain block (`FUN_1400b59a0`, version 6): its size, heights,
/// surface, the rivers' and lakes' mesh with the profile's River and Lake materials, and the
/// river lines; no decals yet.
fn new_map(world: &World) -> banished_formats::menu_scene::MapComponent {
    let t = &world.terrain;
    let size = t.size as i32;
    let w = &world.water;
    let material = |m: &str| Some(m.replace('/', "\\"));
    banished_formats::menu_scene::MapComponent {
        version: 6,
        dims: [size, size / 2, 32, 33, size + 1, 33 * 33],
        heights: t.heights.clone(),
        terrain_material: material(&world.profile.materials[0]),
        surface: t.surface.clone(),
        water_vertices: w.vertices.clone(),
        water_indices: w.indices.clone(),
        river_material: material(&world.profile.materials[1]),
        lake_material: material(&world.profile.materials[2]),
        river_ranges: w.rivers.clone(),
        lake_ranges: w.lakes.clone(),
        river_paths: world.river_paths.clone(),
        decals: Vec::new(),
        extra_decals: Vec::new(),
        ints: Vec::new(),
        decal_materials: Vec::new(),
    }
}

mod tutorial;
#[cfg(test)]
use tutorial::scenario_params;
pub use tutorial::{scenario_game, scenarios};

/// `FUN_1400671d0`: where the Center button puts the camera. An entity with a map rectangle:
/// the middle of its corners `(x, y)` and `(x + w, y + h)` (cell positions without the
/// half-cell offset); one without (a citizen, an animal): its model's position.
fn center_target(town: &StartingTown, entity: Entity) -> glam::Vec2 {
    match entity {
        Entity::Placed(i) if town.templates[town.placed[i].template].map.is_some() => {
            let p = &town.placed[i];
            let half = (town.cells.size >> 1) as f32;
            glam::Vec2::new(
                (p.cell.0 as f32 - half + (p.cell.0 + p.footprint.0) as f32 - half) * 0.5,
                (p.cell.1 as f32 - half + (p.cell.1 + p.footprint.1) as f32 - half) * 0.5,
            )
        }
        _ => entity.position(town).truncate(),
    }
}

/// An achievement's instance as `new_game` writes it: per goal its reference, its instance
/// class, its value (bit 0 met, bit 1 failed) and a TimeGoal's time.
fn achievement_save<'a>(
    a: &'a crate::achievements::Achievement,
    instance: Option<&crate::achievements::AchievementInstance>,
) -> crate::start::save::new_game::AchievementSave<'a> {
    use banished_formats::scenario::GoalKind;
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

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    fn generated() -> Option<(crate::Game, GameplayLoop)> {
        let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../game-data/original/WinData");
        if !dir.join("data0.pkg").exists() {
            eprintln!("game data missing; skipping gameplay growth integration");
            return None;
        }
        let packages = ["data1.pkg", "patch1.pkg", "patch0.pkg", "data0.pkg"]
            .into_iter()
            .filter_map(|name| {
                std::fs::read(dir.join(name))
                    .ok()
                    .map(|data| (name.to_owned(), data))
            })
            .collect();
        let mut game =
            crate::Game::with_settings(packages, crate::settings::Settings::default()).unwrap();
        let mut gameplay = GameplayLoop::new(WorldParams {
            start_condition_ref: r"Game\StartCondition\Medium.rsc:resource".into(),
            ..WorldParams::default()
        });
        game.with_ctx(|_, ctx| {
            gameplay.load(ctx);
            gameplay.startup(ctx);
        });
        Some((game, gameplay))
    }

    #[test]
    fn selection_host_and_highlight_leave_simulation_unchanged() {
        let Some((mut game, mut gameplay)) = generated() else {
            return;
        };
        let town = gameplay.town.as_ref().unwrap();
        let id = town
            .storage_sites
            .iter()
            .map(|s| s.entity)
            .find(|&i| {
                town.templates[town.placed[i].template]
                    .reference
                    .contains("StorageBarn")
            })
            .unwrap();
        let position = glam::Vec3::from(town.placed[id].position);
        let origin = position + glam::Vec3::new(0., -12., 10.);
        let direction = (position + glam::Vec3::Z - origin).normalize();
        let rng = gameplay.rng.clone();
        let time = gameplay.calendar.elapsed;
        let before = format!(
            "{:?}{:?}{:?}",
            town.citizens, town.agents.states, town.storages
        );
        game.with_ctx(|_, ctx| {
            let town = gameplay.town.as_ref().unwrap();
            assert_eq!(
                gameplay
                    .selection
                    .cast(ctx.archive, town, origin, direction, 400., None),
                Some(Entity::Placed(id))
            );
            for _ in 0..3 {
                gameplay.entities.select(
                    ctx.ui,
                    gameplay.screen.as_mut().unwrap(),
                    ctx.archive,
                    town,
                    Entity::Placed(id),
                );
                assert_eq!(gameplay.entities.open.len(), 1);
                let window = &gameplay.entities.open[0];
                assert!(!window.controllers.is_empty());
                let root = window.root;
                assert!(ctx.ui.find(root, "labelTitle").is_some());
                let pages = ctx.ui.find(root, "pages").unwrap();
                assert!(matches!(
                    ctx.ui.node(pages).kind,
                    banished_ui::Kind::Control(banished_ui::Control::Page { selected: 1, .. })
                ));
                gameplay
                    .screen
                    .as_ref()
                    .unwrap()
                    .layout(ctx.ui, (1280, 800));
                ctx.ui
                    .pointer_move(80, 80, gameplay.screen.as_ref().unwrap().root);
                let _ = ctx.ui.draw(gameplay.screen.as_ref().unwrap().root);
                let view = gameplay.view.as_ref().unwrap();
                let grid = banished_game::terrain::HeightGrid {
                    width: view.terrain.size + 1,
                    heights: &view.terrain.heights,
                };
                let geometry =
                    gameplay
                        .highlight
                        .geometry(ctx.archive, town, &grid, Some(Entity::Placed(id)));
                assert!(!geometry.is_empty());
                assert_eq!(
                    gameplay.entities.close_containing(
                        ctx.ui,
                        gameplay.screen.as_mut().unwrap(),
                        root
                    ),
                    Some(Entity::Placed(id))
                );
            }
        });
        let town = gameplay.town.as_ref().unwrap();
        assert_eq!(rng, gameplay.rng);
        assert_eq!(time, gameplay.calendar.elapsed);
        assert_eq!(
            before,
            format!(
                "{:?}{:?}{:?}",
                town.citizens, town.agents.states, town.storages
            )
        );
    }

    /// TownHallUI over the started town: the overview's counts, a row per stock slot and
    /// a profession row each, and the graphs drawn from the statistics.
    #[test]
    fn town_hall_window_reads_the_town() {
        let Some((mut game, mut gameplay)) = generated() else {
            return;
        };
        game.with_ctx(|_, ctx| {
            for _ in 0..3 {
                gameplay.iterate(ctx.archive, 1.0 / 30.0);
            }
            let root = ctx.ui.instantiate("Dialog\\TownHall.rsc:tabs").unwrap();
            let mut th = crate::town_hall_ui::TownHallUi::bind(ctx.ui, ctx.archive, root);
            let town = gameplay.town.as_ref().unwrap();
            let x = crate::town_hall_ui::TownContext {
                stock_counts: &gameplay.stock_warnings.count,
                statistics: &gameplay.statistics,
            };
            th.refresh(ctx.ui, ctx.archive, town, &x);
            let text = |name: &str| {
                let n = ctx.ui.find(root, name).unwrap();
                match &ctx.ui.node(n).kind {
                    banished_ui::Kind::Control(banished_ui::Control::Edit(e)) => {
                        e.text.text.clone()
                    }
                    banished_ui::Kind::Text(t) => t.text.clone(),
                    _ => String::new(),
                }
            };
            let alive = town.citizens.iter().filter(|c| c.alive).count();
            assert_eq!(text("numberCitizens"), alive.to_string());
            assert!(!gameplay.statistics.series[0].is_empty(), "no sample yet");
            assert_eq!(gameplay.statistics.series[0][0], alive as i32);
        });
    }

    /// TradeUI on a trading post with a merchant docked: the post's kept logs sell at the
    /// merchant's value, its goods buy; the trade button sends the exchange, which moves the
    /// goods, counts the value traded away and brings the livestock to the post.
    #[test]
    fn trade_window_exchanges_with_the_merchant() {
        let Some((mut game, mut gameplay)) = generated() else {
            return;
        };
        let view = gameplay.view.as_ref().unwrap();
        let heights = view.terrain.heights.clone();
        let terrain = banished_game::terrain::HeightGrid {
            width: view.terrain.size + 1,
            heights: &heights,
        };
        let id = game.with_ctx(|_, ctx| {
            let town = gameplay.town.as_mut().unwrap();
            let mut rng = banished_game::random::Random::new(7);
            let mut tool = crate::placement_tool::PlacementTool::start(
                ctx.archive,
                town,
                r"Template\TradingPost.rsc:resource",
                &mut rng,
            )
            .unwrap();
            let n = town.cells.size as i32;
            let half = (n >> 1) as f32;
            let river = gameplay.river_paths.first().cloned().unwrap_or_default();
            let mut spot = None;
            'search: for p in &river {
                let (x, y) = ((p[0] + half) as i32, (p[1] + half) as i32);
                for dy in -6..=6 {
                    for dx in -6..=6 {
                        for r in 0..4 {
                            tool.hover(town, &terrain, Some((x + dx, y + dy)), r);
                            if tool.valid() {
                                spot = Some(((x + dx, y + dy), r));
                                break 'search;
                            }
                        }
                    }
                }
            }
            let (at, r) = spot.expect("no trading post spot");
            tool.hover(town, &terrain, Some(at), r);
            tool.place(ctx.archive, town, r).unwrap()
        });
        let town = gameplay.town.as_mut().unwrap();
        // The next iteration activates the post (its TradeComponent).
        let mut rng = banished_game::random::Random::new(7);
        town.apply_commands(&terrain, &mut rng);
        town.iterate(&terrain, &mut rng, 1.0 / 30.0, 50.0);
        let material = |town: &crate::start::StartingTown, key: &str| {
            (0..town.materials.len())
                .find(|&m| town.materials[m].reference.contains(key))
                .unwrap()
        };
        let logs = material(town, "RawMaterialWood");
        let goods = &town.construction.trade_goods;
        let food = goods
            .iter()
            .position(|g| g.material.is_some() && g.material != Some(logs))
            .unwrap();
        let livestock = goods
            .iter()
            .position(|g| town.wildlife.livestock_kinds.contains_key(&g.template))
            .expect("no livestock good");
        // A merchant who takes logs.
        let st = town.construction.sites[&id].template;
        let flags = town.materials[logs].flags;
        let merchant = town.construction.templates[st]
            .trade
            .as_ref()
            .unwrap()
            .merchants
            .iter()
            .position(|m| (m.flags.0 | m.flags.1) & flags != 0)
            .expect("no merchant takes logs") as i32;
        let site = town.construction.sites.get_mut(&id).unwrap();
        site.machine.state = 0x13;
        let storage = site.storage;
        let tr = site.trade.as_mut().unwrap();
        tr.merchant = merchant;
        tr.timer = i64::MAX / 2;
        tr.keep.insert(logs, 30);
        tr.offers = vec![
            banished_game::trade::Offer {
                good: food,
                value: 2,
                amount: 10,
            },
            banished_game::trade::Offer {
                good: livestock,
                value: 1,
                amount: 2,
            },
        ];
        town.storages[storage].add(logs, 30);
        game.with_ctx(|_, ctx| {
            let town = gameplay.town.as_ref().unwrap();
            gameplay.entities.select(
                ctx.ui,
                gameplay.screen.as_mut().unwrap(),
                ctx.archive,
                town,
                Entity::Placed(id),
            );
            let root = gameplay.entities.open[0].root;
            assert!(gameplay.entities.trade_windows().contains(&id));
            // The sell row of the kept logs and the merchant's two goods.
            let spinners: Vec<_> = (0..ctx.ui.nodes.len())
                .filter(|&n| {
                    ctx.ui.descendant(root, n) && ctx.ui.node(n).name == "spinnerItemCount"
                })
                .collect();
            assert_eq!(spinners.len(), 3, "one sell row and two buy rows");
            let value = banished_game::trade::sell_value(
                &town.construction.trade_goods,
                &town.materials,
                town.construction.sites[&id].trade.as_ref().unwrap(),
                town.construction.templates[town.construction.sites[&id].template]
                    .trade
                    .as_ref(),
                logs,
            );
            // Sell enough logs to pay for 4 food and both animals.
            let sell = (4 * 2 + 2 + value - 1) / value;
            crate::building_ui::set_spinner(ctx.ui, spinners[0], sell);
            crate::building_ui::set_spinner(ctx.ui, spinners[1], 4);
            crate::building_ui::set_spinner(ctx.ui, spinners[2], 2);
            gameplay.entities.refresh(ctx.ui, ctx.archive, town);
            let trade = ctx.ui.find(root, "buttonTrade").unwrap();
            assert!(value > 0);
            assert!(ctx.ui.node(trade).enabled, "the trade is not allowed");
            let Some(WorkPlaceClick::Command(c)) = gameplay.entities.click(ctx.ui, town, trade)
            else {
                panic!("the trade button sends nothing");
            };
            assert_eq!(
                c,
                banished_game::professions::Command::Exchange {
                    entity: id,
                    sell: vec![(logs, sell)],
                    buy: vec![(food, 4), (livestock, 2)],
                }
            );
            let town = gameplay.town.as_mut().unwrap();
            let unassigned = town.wildlife.unassigned.len();
            town.construction.commands.push(c);
            let mut rng = banished_game::random::Random::new(7);
            town.apply_commands(&terrain, &mut rng);
            let s = &town.storages[storage];
            let food_m = town.construction.trade_goods[food].material.unwrap();
            assert_eq!(s.available(logs), 30 - sell);
            assert_eq!(s.available(food_m), 4);
            assert!(town.construction.known_materials.contains(&food_m));
            let tr = town.construction.sites[&id].trade.as_ref().unwrap();
            assert_eq!(tr.offers.len(), 1, "the livestock offer is used up");
            assert_eq!(tr.offers[0].amount, 6);
            assert_eq!(town.construction.trade_value, sell * value);
            let t = town.construction.trade_goods[livestock].template;
            assert!(town.construction.known.contains(&t));
            assert_eq!(town.wildlife.unassigned.len(), unassigned + 2);
            let a = *town.wildlife.unassigned.last().unwrap();
            assert_eq!(
                town.wildlife.animals[&a].flags & banished_game::wildlife::WILD,
                0
            );
            // The window's amounts go back to none.
            let town = gameplay.town.as_ref().unwrap();
            gameplay.entities.refresh(ctx.ui, ctx.archive, town);
            let left: Vec<_> = (0..ctx.ui.nodes.len())
                .filter(|&n| {
                    ctx.ui.descendant(root, n) && ctx.ui.node(n).name == "spinnerItemCount"
                })
                .map(|n| crate::building_ui::spinner_value(ctx.ui, n))
                .collect();
            assert_eq!(left, vec![Some(0), Some(0)]);
        });
    }

    /// CropUI on a placed field: its `selectResource` combo lists the known crops; choosing
    /// one sends the field's crop.
    #[test]
    fn crop_window_lists_the_known_crops() {
        let Some((mut game, mut gameplay)) = generated() else {
            return;
        };
        let view = gameplay.view.as_ref().unwrap();
        let heights = view.terrain.heights.clone();
        let terrain = banished_game::terrain::HeightGrid {
            width: view.terrain.size + 1,
            heights: &heights,
        };
        let id = game.with_ctx(|_, ctx| {
            let town = gameplay.town.as_mut().unwrap();
            let n = town.cells.size as i32;
            let empty = |town: &crate::start::StartingTown, x: i32, y: i32| {
                (y..y + 6).all(|cy| {
                    (x..x + 6).all(|cx| {
                        let i = (cy * n + cx) as usize;
                        cx < n
                            && cy < n
                            && town.cells.types[i] == banished_game::terrain::CellType::GROUND
                            && town.cells.objects[i].is_none()
                            && town.cells.occupants[i].is_none()
                            && town.cells.flags[i] == 0
                    })
                })
            };
            let (sx, sy) = town.start;
            let rect = (8..60)
                .flat_map(|r| (-r..=r).flat_map(move |d| [(d, -r), (d, r), (-r, d), (r, d)]))
                .map(|(dx, dy)| (sx + dx, sy + dy))
                .find(|&(x, y)| x > 0 && y > 0 && empty(town, x, y))
                .expect("no empty ground");
            town.place_area(
                ctx.archive,
                r"Template\CropField.rsc:resource",
                (rect.0, rect.1, 6, 6),
                &terrain,
            )
            .expect("the field was not placed")
        });
        let town = gameplay.town.as_mut().unwrap();
        let mut rng = banished_game::random::Random::new(7);
        town.apply_commands(&terrain, &mut rng);
        town.iterate(&terrain, &mut rng, 1.0 / 30.0, 50.0);
        let crops: Vec<usize> = town
            .construction
            .known
            .iter()
            .copied()
            .filter(|&t| {
                town.templates[t].growth.is_some() && !town.templates[t].reference.contains("Tree")
            })
            .collect();
        assert!(!crops.is_empty());
        game.with_ctx(|_, ctx| {
            let town = gameplay.town.as_ref().unwrap();
            gameplay.entities.select(
                ctx.ui,
                gameplay.screen.as_mut().unwrap(),
                ctx.archive,
                town,
                Entity::Placed(id),
            );
            let root = gameplay.entities.open[0].root;
            let combo = ctx.ui.find(root, "selectResource").unwrap();
            let list = match &ctx.ui.node(combo).kind {
                banished_ui::Kind::Control(banished_ui::Control::Combo { list, .. }) => *list,
                _ => panic!("selectResource is not a combo"),
            };
            let items = match &ctx.ui.node(list).kind {
                banished_ui::Kind::List(l) => l.items.clone(),
                _ => Vec::new(),
            };
            assert_eq!(items.len(), crops.len(), "{items:?}");
            assert!(ctx.ui.node(combo).enabled);
            let chosen = gameplay
                .entities
                .select_item(ctx.ui, town, combo, 0)
                .expect("the choice sends nothing");
            assert!(matches!(
                chosen,
                WorkPlaceClick::Command(banished_game::professions::Command::SetCrop {
                    entity,
                    crop: Some(c),
                }) if entity == id && crops.contains(&c)
            ));
        });
    }

    /// HelpUI: the topics by title, the chosen page's text, back and forward through the
    /// pages seen.
    #[test]
    fn windows_drag_by_their_title() {
        let Some((mut game, mut gameplay)) = generated() else {
            return;
        };
        game.with_ctx(|_, ctx| {
            let template = r"Template\UtilityHelp.rsc:resource";
            gameplay.utility.toggle(
                ctx.ui,
                gameplay.screen.as_mut().unwrap(),
                ctx.archive,
                template,
                None,
                0,
            );
            gameplay
                .screen
                .as_mut()
                .unwrap()
                .layout(ctx.ui, (1280, 800));
            let screen_root = gameplay.screen.as_ref().unwrap().root;
            let title = ctx.ui.find(screen_root, "borderTitle").expect("no title");
            let [tx, ty, ..] = ctx.ui.node(title).rect;
            let (_, root) = gameplay
                .utility
                .title_at(ctx.ui, tx + 4, ty + 4)
                .expect("no title to drag");
            let [x0, y0, ..] = ctx.ui.node(root).rect;
            let b = ctx.ui.find(root, "borderTitle").unwrap();
            let [bx, by, ..] = ctx.ui.node(b).rect;
            let press = (bx + 4, by + 4);
            gameplay.raw(ctx, RawInput::Cursor(press.0, press.1));
            gameplay.raw(ctx, RawInput::UiPress);
            gameplay.raw(ctx, RawInput::Cursor(press.0 + 100, press.1 + 50));
            gameplay.raw(ctx, RawInput::Release);
            gameplay.raw(ctx, RawInput::Cursor(press.0 + 300, press.1 + 300));
            let m = ctx.ui.node(root).margins;
            assert_eq!((m[0], m[2]), (x0 + 100, y0 + 50));
            // Reopened, it comes back where it was left.
            for _ in 0..2 {
                gameplay.utility.toggle(
                    ctx.ui,
                    gameplay.screen.as_mut().unwrap(),
                    ctx.archive,
                    template,
                    None,
                    0,
                );
            }
            gameplay
                .screen
                .as_mut()
                .unwrap()
                .layout(ctx.ui, (1280, 800));
            assert!(gameplay
                .utility
                .title_at(ctx.ui, press.0 + 100, press.1 + 50)
                .is_some());
            // An entity window pins itself once dragged more than 5 pixels.
            let town = gameplay.town.as_ref().unwrap();
            let barn = town.storage_sites[0].entity;
            gameplay.entities.select(
                ctx.ui,
                gameplay.screen.as_mut().unwrap(),
                ctx.archive,
                town,
                Entity::Placed(barn),
            );
            gameplay
                .screen
                .as_mut()
                .unwrap()
                .layout(ctx.ui, (1280, 800));
            let root = gameplay.entities.open[0].root;
            let b = ctx.ui.find(root, "borderTitle").expect("no entity title");
            let [bx, by, ..] = ctx.ui.node(b).rect;
            gameplay.raw(ctx, RawInput::Cursor(bx + 4, by + 4));
            gameplay.raw(ctx, RawInput::UiPress);
            gameplay.raw(ctx, RawInput::Cursor(bx + 7, by + 6));
            assert!(!gameplay.entities.open[0].pinned);
            gameplay.raw(ctx, RawInput::Cursor(bx + 24, by + 4));
            gameplay.raw(ctx, RawInput::Release);
            assert!(gameplay.entities.open[0].pinned);
            assert!(gameplay.entities.open[0].at.is_some());
            let pin = ctx.ui.find(root, "checkPin").unwrap();
            assert!(ctx.ui.checked(pin));
        });
    }

    #[test]
    fn help_window_browses_the_pages() {
        let Some((mut game, mut gameplay)) = generated() else {
            return;
        };
        game.with_ctx(|_, ctx| {
            gameplay.utility.toggle(
                ctx.ui,
                gameplay.screen.as_mut().unwrap(),
                ctx.archive,
                r"Template\UtilityHelp.rsc:resource",
                None,
                0,
            );
            let root = gameplay.screen.as_ref().unwrap().root;
            let combo = ctx.ui.find(root, "comboTopic").expect("no help window");
            let doc = ctx.ui.find(root, "documentHelp").unwrap();
            let list = match &ctx.ui.node(combo).kind {
                banished_ui::Kind::Control(banished_ui::Control::Combo { list, .. }) => *list,
                _ => panic!("comboTopic is not a combo"),
            };
            let items = match &ctx.ui.node(list).kind {
                banished_ui::Kind::List(l) => l.items.clone(),
                _ => Vec::new(),
            };
            assert_eq!(items.len(), 66);
            assert_eq!(items[0], "Contents");
            let text = |ui: &banished_ui::Ui| match &ui.node(doc).kind {
                banished_ui::Kind::Control(banished_ui::Control::Document(_)) => {
                    format!("{:?}", ui.node(doc).kind)
                }
                _ => String::new(),
            };
            assert!(text(ctx.ui).contains("Welcome to the world of Banished"));
            let quarry = items.iter().position(|t| t == "Quarries").unwrap();
            gameplay.event(ctx, Event::Selected(combo, quarry));
            assert!(text(ctx.ui).contains("cut stone"));
            gameplay.update_hud(ctx.ui);
            let back = ctx.ui.find(root, "buttonBack").unwrap();
            let forward = ctx.ui.find(root, "buttonForward").unwrap();
            assert!(ctx.ui.node(back).enabled && !ctx.ui.node(forward).enabled);
            gameplay.event(ctx, Event::Clicked(back));
            assert!(text(ctx.ui).contains("Welcome to the world of Banished"));
            gameplay.update_hud(ctx.ui);
            assert!(!ctx.ui.node(back).enabled && ctx.ui.node(forward).enabled);
            gameplay.event(ctx, Event::Clicked(forward));
            assert!(text(ctx.ui).contains("cut stone"));
        });
    }

    /// The achievement list's goals read the started town: it has a storage barn (Builder's
    /// goal is met), not 300 people, and is not in the mountains.
    #[test]
    fn achievements_read_the_town() {
        let Some((mut game, gameplay)) = generated() else {
            return;
        };
        game.with_ctx(|_, ctx| {
            let list = crate::achievements::load(ctx.archive);
            assert_eq!(list.len(), 36);
            let town = gameplay.town.as_ref().unwrap();
            let terrain = format!(r"Game\Terrain\{}.rsc", gameplay.params.terrain);
            let x = crate::achievements::GoalContext {
                town,
                statistics: &gameplay.statistics,
                year: gameplay.calendar.year,
                year_seconds: 0.0,
                terrain: &terrain,
                climate: &gameplay.params.climate_ref,
                size: 0,
            };
            let state = |title: &str, n: usize| {
                let a = list.iter().find(|a| a.desc.title == title).unwrap();
                crate::achievements::check(&a.goals[n], &Default::default(), &x, Some(ctx.archive))
            };
            use crate::achievements::GoalState;
            let builder = list.iter().find(|a| a.desc.title == "Builder").unwrap();
            let barn = builder
                .goals
                .iter()
                .position(|g| format!("{:?}", g.kind).contains("StorageBarn"))
                .unwrap();
            assert_eq!(state("Builder", barn), GoalState::Met);
            assert_eq!(state("Settlement", 0), GoalState::NotMet);
            let mountains = list.iter().find(|a| a.desc.title == "MountainMen").unwrap();
            let map = mountains
                .goals
                .iter()
                .position(|g| format!("{:?}", g.kind).contains("Mountains"))
                .unwrap();
            assert_eq!(state("MountainMen", map), GoalState::NotMet);
            // Isolationist fails once there is a trading post; there is none.
            let iso = list
                .iter()
                .find(|a| a.desc.title == "Isolationist")
                .unwrap();
            let post = iso
                .goals
                .iter()
                .position(|g| format!("{:?}", g.kind).contains("TradingPost"))
                .unwrap();
            assert_eq!(state("Isolationist", post), GoalState::Met);
        });
    }

    /// The first tutorial from its scenario: the window opens with step 0's text, only the
    /// tutorial's next button (and the options) usable; pressing next enters step 1.
    #[test]
    fn tutorial_starts_and_steps_on() {
        let Some((mut game, _)) = generated() else {
            return;
        };
        let mut gameplay = None;
        game.with_ctx(|_, ctx| {
            let list = scenarios(ctx.archive);
            assert_eq!(list.len(), 4);
            let first = list
                .iter()
                .position(|(r, _)| r.contains("GettingStarted"))
                .unwrap();
            let params = scenario_params(ctx.ui, ctx.global.as_ref(), &list[first].1, first);
            assert_eq!(params.seed, 120323451);
            // It starts from `Save\TutorialGettingStarted.sav`, as the scenario's.
            let mut g = scenario_game(
                ctx.ui,
                ctx.archive,
                ctx.global.as_ref(),
                &list[first].1,
                first,
            );
            assert!(g.saved.is_some(), "the tutorial's save did not load");
            // Every tutorial names a save, and each one reads.
            for (i, (r, s)) in list.iter().enumerate() {
                assert!(!s.save.is_empty(), "{r}");
                let g = scenario_game(ctx.ui, ctx.archive, ctx.global.as_ref(), s, i);
                assert!(g.saved.is_some(), "{r}: {}", s.save);
            }
            assert_eq!(g.params.scenario, Some(first));
            g.load(ctx);
            g.startup(ctx);
            gameplay = Some(g);
        });
        let mut gameplay = gameplay.unwrap();
        game.with_ctx(|_, ctx| {
            gameplay.update(ctx, 1.0 / 30.0);
            gameplay.update(ctx, 1.0 / 30.0);
            let t = gameplay.tutorial.as_ref().expect("no tutorial");
            assert_eq!(t.step, 0);
            assert!(t.def.steps.len() > 50);
            let ui = t.ui.clone().expect("no tutorial window");
            let pre = ui.pre.unwrap();
            let text = format!("{:?}", ctx.ui.node(pre).kind);
            assert!(!text.contains("Step_Keys_0_Pre"), "untranslated: {text}");
            let next = ui.next.unwrap();
            assert!(ctx.ui.node(next).usable(), "next is locked");
            let housing = gameplay
                .toolbar
                .as_ref()
                .unwrap()
                .button(ctx.ui, "housing")
                .unwrap();
            assert!(!ctx.ui.node(housing).usable(), "the toolbar is not locked");
            gameplay.event(ctx, Event::Clicked(next));
            gameplay.update(ctx, 1.0 / 30.0);
            assert_eq!(gameplay.tutorial.as_ref().unwrap().step, 1);
            // Through the keys to the first house: the housing button alone opens up.
            for step in 2..=9 {
                gameplay.event(ctx, Event::Clicked(next));
                gameplay.update(ctx, 1.0 / 30.0);
                assert_eq!(gameplay.tutorial.as_ref().unwrap().step, step);
            }
            let toolbar = gameplay.toolbar.as_ref().unwrap();
            let housing = toolbar.button(ctx.ui, "housing").unwrap();
            let food = toolbar.button(ctx.ui, "food").unwrap();
            assert!(ctx.ui.node(housing).usable());
            assert!(!ctx.ui.node(food).usable());
        });
    }

    /// The placement overlay (`FUN_140106850`): over the barn the footprint has red cells; on
    /// clear ground every cell is green and the porch's `.` cells show the entrance arrow
    /// (frame 2) turned with the building.
    #[test]
    fn the_placement_overlay_shows_validity_and_the_entrance() {
        let Some((mut game, mut gameplay)) = generated() else {
            return;
        };
        game.with_ctx(|_, ctx| {
            let town = gameplay.town.as_mut().unwrap();
            let view = gameplay.view.as_ref().unwrap();
            let grid = banished_game::terrain::HeightGrid {
                width: view.terrain.size + 1,
                heights: &view.terrain.heights,
            };
            let mut tool = PlacementTool::start(
                ctx.archive,
                town,
                "Template\\WoodHouse.rsc:resource",
                &mut gameplay.rng,
            )
            .unwrap();
            let barn = town
                .placed
                .iter()
                .find(|p| town.templates[p.template].reference.contains("StorageBarn"))
                .unwrap()
                .cell;
            tool.hover(town, &grid, Some((barn.0 + 2, barn.1 + 2)), 0);
            let over = tool.overlay(town, &grid, tool.rect().unwrap(), 0, false);
            assert!(over.iter().any(|c| c.argb == 0xffff_0000));
            let at = tool.find_clear_spot(town, &grid, town.start).unwrap();
            let mut offsets = Vec::new();
            for rotation in 0..4 {
                tool.hover(town, &grid, Some(at), rotation);
                let Some(rect) = tool.rect().filter(|_| tool.valid()) else {
                    continue;
                };
                let over = tool.overlay(town, &grid, rect, rotation * 90, false);
                assert!(over.iter().all(|c| c.argb == 0xff00_ff00));
                let arrows: Vec<_> = over.iter().filter(|c| c.frame == 2).collect();
                assert_eq!(arrows.len(), 4, "the porch");
                // Every arrow turns with the building by the same entrance offset.
                offsets.extend(arrows.iter().map(|c| c.rotation - rotation * 90));
            }
            assert!(offsets.len() >= 8, "{offsets:?}");
            assert!(
                offsets.iter().all(|&o| o == offsets[0] && o >= 0),
                "{offsets:?}"
            );
            let restricted = tool.overlay(town, &grid, tool.rect().unwrap(), 0, true);
            assert!(restricted.iter().all(|c| c.argb == 0xffff_ff00));
        });
    }

    /// A forester's work radius: its ghost shows the ring (`FUN_1401033b0` →
    /// `FUN_140055d90`), and so does its window while open (RadiusUI, `FUN_14007cb20`).
    #[test]
    fn work_radii_show_while_placing_and_selected() {
        let Some((mut game, mut gameplay)) = generated() else {
            return;
        };
        game.with_ctx(|_, ctx| {
            let count = |g: &mut GameplayLoop, ctx: &mut Ctx| {
                g.refresh_decals(ctx.archive);
                g.view
                    .as_ref()
                    .unwrap()
                    .extras
                    .as_ref()
                    .unwrap()
                    .decals
                    .len()
            };
            let base = count(&mut gameplay, ctx);
            assert!(gameplay.script(ctx, "tool Template/ForesterLodge.rsc"));
            let start = gameplay.town.as_ref().unwrap().start;
            {
                let town = gameplay.town.as_ref().unwrap();
                let view = gameplay.view.as_ref().unwrap();
                let grid = banished_game::terrain::HeightGrid {
                    width: view.terrain.size + 1,
                    heights: &view.terrain.heights,
                };
                let tool = gameplay.placement.as_mut().unwrap();
                tool.hover(town, &grid, Some(start), 0);
                assert!(tool.radius_decal().is_some());
            }
            assert_eq!(count(&mut gameplay, ctx), base + 1);
            gameplay.end_placement(ctx);
            assert_eq!(count(&mut gameplay, ctx), base);
            assert!(gameplay.script(ctx, "place Template/ForesterLodge.rsc"));
            let with_site = count(&mut gameplay, ctx);
            assert!(gameplay.script(ctx, "select"));
            assert_eq!(count(&mut gameplay, ctx), with_site + 1);
        });
    }

    /// A citizen window's Follow (`FUN_140065080`): the dialog opens with the citizen's name,
    /// the camera stays on the citizen, and Escape closes it. Center, Home and Profession act
    /// on the window's citizen.
    #[test]
    fn follow_keeps_the_camera_on_the_citizen() {
        let Some((mut game, mut gameplay)) = generated() else {
            return;
        };
        game.with_ctx(|_, ctx| {
            let entity = Entity::Citizen(0);
            {
                let (town, screen) = (
                    gameplay.town.as_ref().unwrap(),
                    gameplay.screen.as_mut().unwrap(),
                );
                gameplay
                    .entities
                    .select(ctx.ui, screen, ctx.archive, town, entity);
            }
            let root = gameplay.entities.open[0].root;
            let follow = ctx.ui.find(root, "buttonFollow").expect("buttonFollow");
            gameplay.event(ctx, Event::Clicked(follow));
            assert!(gameplay.follow.is_some());
            for _ in 0..30 {
                gameplay.update(ctx, 1.0 / 30.0);
            }
            let at = gameplay.town.as_ref().unwrap().citizens[0].position;
            let SceneCamera::Map(camera) = &gameplay.view.as_ref().unwrap().camera else {
                panic!("no map camera");
            };
            assert!((camera.target - glam::Vec2::new(at[0], at[1])).length() < 1e-3);
            let name = gameplay.town.as_ref().unwrap().citizens[0].name.clone();
            let screen = gameplay.screen.as_ref().unwrap();
            let shown = screen
                .dialogs
                .iter()
                .rev()
                .find_map(|d| ctx.ui.find(d.root, "textFollow"))
                .map(|n| crate::utility::text_of(ctx.ui, n))
                .unwrap_or_default();
            assert!(shown.contains(&name), "{shown:?} {name:?}");
            gameplay.raw(ctx, RawInput::Escape);
            assert!(gameplay.follow.is_none());
            // Center: the camera on the citizen's model.
            let center = ctx.ui.find(root, "buttonCenter").expect("buttonCenter");
            gameplay.event(ctx, Event::Clicked(center));
        });
    }

    /// The Path tool (`FUN_1400e2c40`): a worked building's workers' commutes, home to
    /// workplace, drawn as cells and steps; Escape ends it.
    #[test]
    fn the_path_tool_shows_commutes() {
        let Some((mut game, _)) = generated() else {
            return;
        };
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let Ok(bytes) = std::fs::read(root.join("save-game/Save83.sav")) else {
            return;
        };
        let mut gameplay = GameplayLoop::from_save(crate::saved::SavedGame::parse(&bytes).unwrap());
        game.with_ctx(|_, ctx| {
            gameplay.load(ctx);
            gameplay.startup(ctx);
            gameplay.start_tool(ctx, "Template\\UtilityPath.rsc:resource");
            assert!(gameplay.path_tool.is_some());
            let town = gameplay.town.as_mut().unwrap();
            let c = town
                .agents
                .states
                .iter()
                .position(|s| s.home.is_some() && s.workplace.is_some())
                .expect("a commuter in Save83");
            let workplace = town.agents.states[c].workplace.unwrap();
            let tool = gameplay.path_tool.as_mut().unwrap();
            tool.pick(town, &mut gameplay.rng, Entity::Citizen(c));
            let (lines, cells) = tool.overlay(town.cells.size as i32);
            assert!(cells.len() >= 2, "{}", cells.len());
            assert!(!lines.is_empty());
            assert!(cells.iter().all(|c| c.frame == 4 && c.argb == 0xfffc_cc4c));
            // The workplace: every worker's commute, at least as many cells.
            tool.pick(town, &mut gameplay.rng, Entity::Placed(workplace));
            let (_, all) = tool.overlay(town.cells.size as i32);
            assert!(all.len() >= cells.len());
            gameplay.raw(ctx, RawInput::Escape);
            assert!(gameplay.path_tool.is_none());
        });
    }

    /// A crop field's window lists the known crops with their icons (`FUN_14007dd00`), and the
    /// combo's label shows the chosen one's.
    #[test]
    fn crop_choices_have_icons() {
        let Some((mut game, _)) = generated() else {
            return;
        };
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let Ok(bytes) = std::fs::read(root.join("save-game/Save83.sav")) else {
            return;
        };
        let mut gameplay = GameplayLoop::from_save(crate::saved::SavedGame::parse(&bytes).unwrap());
        game.with_ctx(|_, ctx| {
            gameplay.load(ctx);
            gameplay.startup(ctx);
            let field = {
                let town = gameplay.town.as_ref().unwrap();
                town.placed
                    .iter()
                    .position(|p| {
                        p.alive && town.templates[p.template].reference.contains("CropField")
                    })
                    .expect("a crop field")
            };
            {
                let (town, screen) = (
                    gameplay.town.as_ref().unwrap(),
                    gameplay.screen.as_mut().unwrap(),
                );
                gameplay
                    .entities
                    .select(ctx.ui, screen, ctx.archive, town, Entity::Placed(field));
            }
            gameplay.update(ctx, 1.0 / 30.0);
            let w = gameplay.entities.open.last().unwrap().root;
            let combo = ctx.ui.find(w, "selectResource").expect("selectResource");
            let list = crate::menu::combo_list(ctx.ui, combo).unwrap();
            let banished_ui::Kind::List(data) = &ctx.ui.node(list).kind else {
                panic!("no list");
            };
            assert!(!data.items.is_empty());
            assert_eq!(data.icons.len(), data.items.len());
            assert!(data.icons.iter().all(Option::is_some), "{:?}", data.items);
        });
    }

    /// The Limits window (`UtilityLimits`): each ResourceLimitUI row shows its slot's limit,
    /// and a changed spinner sets it (`FUN_14007ccb0`).
    #[test]
    fn the_limits_window_sets_the_stock_limits() {
        let Some((mut game, mut gameplay)) = generated() else {
            return;
        };
        game.with_ctx(|_, ctx| {
            assert!(gameplay.script(ctx, "open Template/UtilityLimits.rsc"));
            gameplay.update(ctx, 1.0 / 30.0);
            let root = gameplay.utility.controllers()[0].0;
            let mut spinners = Vec::new();
            let mut stack = vec![root];
            while let Some(id) = stack.pop() {
                if ctx.ui.node(id).name == "editLimit" {
                    spinners.push(id);
                }
                stack.extend(ctx.ui.children(id));
            }
            assert_eq!(spinners.len(), 11);
            let shown: Vec<_> = spinners
                .iter()
                .map(|&s| crate::building_ui::spinner_value(ctx.ui, s).unwrap())
                .collect();
            for v in &shown {
                assert!(gameplay.stock_warnings.limits.contains(v), "{shown:?}");
            }
            crate::building_ui::set_spinner(ctx.ui, spinners[0], 345);
            gameplay.event(ctx, Event::Changed(spinners[0]));
            assert!(gameplay.stock_warnings.limits.contains(&345));
        });
    }

    /// The start structures stamp the map's object (`+8`, `FUN_1400a30f0`) on their
    /// footprint, so picking finds the storage yard through its cells.
    #[test]
    fn start_structures_stamp_their_cells() {
        let Some((_, gameplay)) = generated() else {
            return;
        };
        let town = gameplay.town.as_ref().unwrap();
        let n = town.cells.size;
        let mut checked = 0;
        for (e, p) in town.placed.iter().enumerate() {
            if !p.alive || town.templates[p.template].storage.is_none() {
                continue;
            }
            let cells: Vec<_> = (p.cell.1..p.cell.1 + p.footprint.1)
                .flat_map(|y| (p.cell.0..p.cell.0 + p.footprint.0).map(move |x| (x, y)))
                .collect();
            let stamped = cells
                .iter()
                .filter(|&&(x, y)| town.cells.objects[y as usize * n + x as usize] == Some(e))
                .count();
            eprintln!(
                "{} {stamped}/{}",
                town.templates[p.template].reference,
                cells.len()
            );
            assert!(stamped > 0, "{}", town.templates[p.template].reference);
            checked += 1;
        }
        assert!(checked > 0);
    }

    /// Planned road cells get the roads' tiled decals (`FUN_1400b4840`): one per tile and
    /// kind, four vertices a cell, the alpha the built byte.
    #[test]
    fn roads_get_decals() {
        let Some((mut game, mut gameplay)) = generated() else {
            return;
        };
        game.with_ctx(|_, ctx| {
            assert!(
                gameplay.road_materials.iter().all(|m| m.is_some()),
                "{:?}",
                gameplay.road_materials
            );
            gameplay.update(ctx, 1.0 / 30.0);
            let count = |g: &GameplayLoop| -> usize {
                let e = g.view.as_ref().unwrap().extras.as_ref().unwrap();
                e.decals.iter().map(|d| d.vertices.len() / 20).sum()
            };
            let before = count(&gameplay);
            let town = gameplay.town.as_mut().unwrap();
            let n = town.cells.size as i32;
            let (x, y) = (town.start.0 as i32, town.start.1 as i32 + 12);
            for dx in 0..6 {
                town.construction
                    .commands
                    .push(banished_game::professions::Command::PlanRoad {
                        cell: y * n + x + dx,
                        kind: 1,
                        ty: 3,
                        material: None,
                    });
            }
            gameplay.update(ctx, 1.0 / 30.0);
            let town = gameplay.town.as_ref().unwrap();
            let planned = (0..6)
                .filter(|dx| town.cells.road_kind[(y * n + x + dx) as usize] == 1)
                .count();
            assert!(0 < planned, "no road planned");
            assert_eq!(count(&gameplay), before + planned * 4);
        });
    }

    /// A burning barn's FireDescription slots start its fire particles and its fire sound;
    /// put out, they fade and stop.
    #[test]
    fn burning_buildings_get_particles() {
        let Some((mut game, mut gameplay)) = generated() else {
            return;
        };
        game.with_ctx(|_, ctx| {
            gameplay.update(ctx, 1.0 / 30.0);
            let town = gameplay.town.as_mut().unwrap();
            let barn = town
                .storage_sites
                .iter()
                .map(|s| s.entity)
                .find(|&i| {
                    town.templates[town.placed[i].template]
                        .reference
                        .contains("StorageBarn")
                })
                .unwrap();
            let fire = |g: &GameplayLoop| {
                g.view
                    .as_ref()
                    .unwrap()
                    .particles
                    .iter()
                    .any(|b| b.material.contains("Fire"))
            };
            assert!(!fire(&gameplay));
            let town = gameplay.town.as_mut().unwrap();
            town.construction
                .sites
                .get_mut(&barn)
                .unwrap()
                .fire
                .as_mut()
                .unwrap()
                .burn = 1_000_000_000;
            for _ in 0..30 {
                gameplay.update(ctx, 1.0 / 30.0);
            }
            assert!(fire(&gameplay));
            // Its FireDescription's emitter slot plays at the building.
            let (_, playing) = ctx.audio.scene.active();
            assert!(0 < playing);
            let town = gameplay.town.as_mut().unwrap();
            town.construction
                .sites
                .get_mut(&barn)
                .unwrap()
                .fire
                .as_mut()
                .unwrap()
                .burn = 0;
            for _ in 0..120 {
                gameplay.update(ctx, 1.0 / 30.0);
            }
            assert!(!fire(&gameplay));
            assert!(ctx.audio.scene.active().1 < playing);
        });
    }

    /// The starting buildings' footprints are drawn opaque (their ClearArea reached state 2),
    /// a cell for each of their footprint's cells.
    #[test]
    fn starting_buildings_get_decals() {
        let Some((mut game, mut gameplay)) = generated() else {
            return;
        };
        game.with_ctx(|_, ctx| {
            gameplay.update(ctx, 1.0 / 30.0);
            let town = gameplay.town.as_ref().unwrap();
            let with: Vec<usize> = (0..town.placed.len())
                .filter(|&i| {
                    town.placed[i].alive
                        && gameplay.decal_descriptions[town.placed[i].template].is_some()
                })
                .collect();
            assert!(!with.is_empty());
            for &i in &with {
                let d = town
                    .construction
                    .decals
                    .get(&i)
                    .copied()
                    .unwrap_or_default();
                assert!(
                    d.opaque,
                    "{}",
                    town.templates[town.placed[i].template].reference
                );
            }
            let cells: usize = with
                .iter()
                .map(|&i| (town.placed[i].footprint.0 * town.placed[i].footprint.1) as usize)
                .sum();
            let e = gameplay.view.as_ref().unwrap().extras.as_ref().unwrap();
            let vertices: usize = e.decals.iter().map(|d| d.vertices.len() / 20).sum();
            assert!(cells * 4 <= vertices, "{cells} cells, {vertices} vertices");
        });
    }

    /// DestroyUI on a storage barn: its buttons ask for the demolition to be called off,
    /// rebuilt or cleared.
    #[test]
    fn destroy_window_buttons_send_commands() {
        let Some((mut game, mut gameplay)) = generated() else {
            return;
        };
        let town = gameplay.town.as_ref().unwrap();
        let id = town
            .storage_sites
            .iter()
            .map(|s| s.entity)
            .find(|&i| {
                town.templates[town.placed[i].template]
                    .reference
                    .contains("StorageBarn")
            })
            .unwrap();
        game.with_ctx(|_, ctx| {
            let town = gameplay.town.as_ref().unwrap();
            gameplay.entities.select(
                ctx.ui,
                gameplay.screen.as_mut().unwrap(),
                ctx.archive,
                town,
                Entity::Placed(id),
            );
            let window = &gameplay.entities.open[0];
            assert!(window
                .bound
                .iter()
                .any(|c| matches!(c, crate::building_ui::Controller::Destroy(_))));
            let root = window.root;
            let (reclaim, rebuild, destroy) = (
                ctx.ui.find(root, "buttonReclaim").unwrap(),
                ctx.ui.find(root, "buttonRebuild").unwrap(),
                ctx.ui.find(root, "buttonDestroy").unwrap(),
            );
            use banished_game::professions::Command;
            let click = |gameplay: &mut GameplayLoop, ui: &banished_ui::Ui, n| match gameplay
                .entities
                .click(ui, gameplay.town.as_ref().unwrap(), n)
            {
                Some(WorkPlaceClick::Command(c)) => Some(c),
                _ => None,
            };
            assert_eq!(
                click(&mut gameplay, ctx.ui, reclaim),
                Some(Command::CancelDemolition { entity: id })
            );
            assert_eq!(
                click(&mut gameplay, ctx.ui, rebuild),
                Some(Command::StateMessage {
                    entity: id,
                    message: 0
                })
            );
            assert_eq!(
                click(&mut gameplay, ctx.ui, destroy),
                Some(Command::StateMessage {
                    entity: id,
                    message: 1
                })
            );
            // A building standing: no damage text, the reclaim button shows.
            gameplay
                .entities
                .refresh(ctx.ui, ctx.archive, gameplay.town.as_ref().unwrap());
            assert!(ctx.ui.node(reclaim).visible());
            assert!(!ctx.ui.node(rebuild).visible());
        });
    }

    #[test]
    fn status_bar_layout_open_close_and_draw_leave_simulation_unchanged() {
        let Some((mut game, mut gameplay)) = generated() else {
            return;
        };
        let town = gameplay.town.as_mut().unwrap();
        let rules = town.agents.rules();
        for s in &mut town.agents.states {
            s.health = rules.health_max * 0.5;
            s.happiness = rules.happiness * 0.5;
            s.status = 1 | 16;
        }
        let people = town.citizens.len() as u32;
        let state = format!("{:?}", town.agents.states);
        let rng = gameplay.rng.clone();
        let time = gameplay.calendar.elapsed;
        let values = gameplay.status_values();
        assert_eq!(values.status, [people, 0, 0, 0, people, 0]);
        assert_eq!(values.stock[4], 40);
        assert_eq!(values.stock[5], 1800);
        assert_eq!(values.health, rules.health_max * 0.5);
        game.with_ctx(|_, ctx| {
            for _ in 0..4 {
                gameplay.utility.toggle(
                    ctx.ui,
                    gameplay.screen.as_mut().unwrap(),
                    ctx.archive,
                    "Template/StatusBar.rsc",
                    None,
                    0,
                );
                gameplay.update_hud(ctx.ui);
                let screen = gameplay.screen.as_ref().unwrap();
                screen.layout(ctx.ui, (1280, 800));
                ctx.ui.pointer_move(80, 80, screen.root);
                let _ = ctx.ui.draw(screen.root);
                if let Some(group) = ctx.ui.find(screen.root, "groupHearts") {
                    let icons = ctx.ui.node(group).children.clone();
                    assert_eq!(icons.len(), 5);
                    let half = ctx.ui.find(icons[2], "half").unwrap();
                    let whole = ctx.ui.find(icons[2], "whole").unwrap();
                    assert_eq!(ctx.ui.node(half).flags & 1, 0);
                    assert_eq!(ctx.ui.node(whole).flags & 1, 1);
                }
            }
        });
        assert_eq!(rng, gameplay.rng);
        assert_eq!(time, gameplay.calendar.elapsed);
        assert_eq!(
            state,
            format!("{:?}", gameplay.town.as_ref().unwrap().agents.states)
        );
    }

    #[test]
    fn audio_plays_music_ambience_and_keeps_the_simulation_stream() {
        let Some((mut game, mut gameplay)) = generated() else {
            return;
        };
        game.with_ctx(|_, ctx| {
            ctx.audio.set_master_volume(0.0);
            for _ in 0..3 {
                gameplay.update(ctx, 1.0 / 30.0);
            }
            let (effects, effects3d) = ctx.audio.scene.active();
            eprintln!("2D effects {effects}, 3D effects {effects3d}");
            // Environment classes (warm and cold) and the wind share 3D effects.
            assert!(effects3d >= 2, "{effects3d}");
            // The music track and the audible ambience.
            assert!(effects >= 1, "{effects}");
            let ambience = gameplay.ambience.as_ref().unwrap();
            assert!(ambience.region_count() > 0);
            // Every generated map has a river (`FUN_1400d2560`): emitters every 8 units.
            let length: f32 = gameplay
                .river_paths
                .iter()
                .flat_map(|p| p.windows(2))
                .map(|w| (glam::Vec2::from(w[1]) - glam::Vec2::from(w[0])).length())
                .sum();
            eprintln!("river length {length}, emitters {}", ambience.river_count());
            assert!(ambience.river_count() > 0);
            assert!(ambience.river_count() as f32 <= length / 8.0);
        });
    }

    #[test]
    fn pause_menu_pauses_sounds_and_the_simulation() {
        use crate::sound::sound_type;
        let Some((mut game, mut gameplay)) = generated() else {
            return;
        };
        game.with_ctx(|_, ctx| {
            ctx.audio.set_master_volume(0.0);
            gameplay.update(ctx, 1.0 / 30.0);
            gameplay.raw(ctx, RawInput::Escape);
            assert!(gameplay.pause_menu.is_some());
            for kind in [sound_type::MUSIC, sound_type::AMBIENT, sound_type::EFFECT] {
                assert!(ctx.audio.scene.is_paused(kind));
            }
            assert!(!ctx.audio.scene.is_paused(sound_type::UI));
            let (rng, seconds) = (gameplay.rng.clone(), gameplay.calendar.seconds());
            for _ in 0..3 {
                gameplay.update(ctx, 1.0 / 30.0);
            }
            assert_eq!(gameplay.calendar.seconds(), seconds);
            assert_eq!(format!("{:?}", gameplay.rng), format!("{rng:?}"));
            // Escape is buttonResume's action.
            gameplay.raw(ctx, RawInput::Escape);
            assert!(gameplay.pause_menu.is_none());
            assert!(!ctx.audio.scene.is_paused(sound_type::MUSIC));
            gameplay.update(ctx, 1.0 / 30.0);
            assert!(gameplay.calendar.seconds() > seconds);
        });
    }

    #[test]
    fn rain_effect_drives_the_rain_emitter() {
        let Some((mut game, mut gameplay)) = generated() else {
            return;
        };
        assert_eq!(
            gameplay.environment.as_ref().unwrap().fade_times[0],
            Some(20.0)
        );
        game.with_ctx(|_, ctx| {
            ctx.audio.set_master_volume(0.0);
            gameplay.update(ctx, 1.0 / 30.0);
            let (_, before) = ctx.audio.scene.active();
            gameplay.environment.as_mut().unwrap().effects[0] =
                Some(banished_game::environment::WeatherEffect {
                    intensity: 0.5,
                    direction: 1.0,
                });
            gameplay.update(ctx, 1.0 / 30.0);
            let ambience = gameplay.ambience.as_ref().unwrap();
            assert!(ambience.weather_active(0));
            // The rain emitter's effect plays (other 3D one-shots come and go with the game
            // stream, so the scene's total is not compared).
            let handles = ambience.weather_handles(0);
            assert!(handles
                .iter()
                .any(|&h| ctx.audio.scene.emitter_count(h) > 0));
            let _ = before;
            // A save made now has the rain emitter's entity, its first slot at the intensity.
            let w = ambience.saved().weather;
            assert_eq!(w.len(), 1);
            assert!(w[0].0.contains("RainEffect"), "{}", w[0].0);
            assert!((w[0].2[0] - 0.5).abs() < 0.01, "{:?}", w[0].2);
            let g = gameplay.save_state(ctx.archive, ctx.ui).unwrap();
            let rain = g.entities.iter().flatten().filter(|&&e| {
                let o = &g.objects[&e];
                matches!(o.fields.at("entity.template"),
                    Some(banished_formats::savegame::Value::Ext(r))
                        if g.references[r].contains("RainEffect"))
            });
            assert_eq!(rain.count(), 1);
            // Rain stops and cannot start again in this step (whatever the stream draws).
            let env = gameplay.environment.as_mut().unwrap();
            env.effects[0] = None;
            env.fade_times[0] = None;
            gameplay.update(ctx, 1.0 / 30.0);
            assert!(!gameplay.ambience.as_ref().unwrap().weather_active(0));
            assert!(handles
                .iter()
                .all(|&h| ctx.audio.scene.emitter_count(h) == 0));
        });
    }

    #[test]
    fn building_emitters_register_without_playing() {
        let Some((mut game, mut gameplay)) = generated() else {
            return;
        };
        game.with_ctx(|_, ctx| {
            ctx.audio.set_master_volume(0.0);
            gameplay.update(ctx, 1.0 / 30.0);
            let town = gameplay.town.as_ref().unwrap();
            let building = town
                .placed
                .iter()
                .position(|p| {
                    p.alive
                        && ctx
                            .archive
                            .read_ref(&format!(
                                "{}:ambientemitter",
                                town.templates[p.template]
                                    .reference
                                    .replace('\\', "/")
                                    .split(':')
                                    .next()
                                    .unwrap()
                            ))
                            .is_ok()
                })
                .expect("a building with an ambient emitter");
            let at = town.placed[building].position;
            let ambience = gameplay.ambience.as_mut().unwrap();
            assert!(ambience.entity_emitters() > 0);
            // Autostart is off: nothing plays until building state starts a slot (fire).
            let (_, before) = ctx.audio.scene.active();
            ambience.start_entity_slot(ctx.audio, building, 0, glam::Vec3::from(at), None);
            assert_eq!(ctx.audio.scene.active().1, before + 1);
            ambience.stop_entity_slot(ctx.audio, building, 0);
            assert_eq!(ctx.audio.scene.active().1, before);
        });
    }

    #[test]
    fn event_ribbon_popup_and_goto() {
        let Some((mut game, mut gameplay)) = generated() else {
            return;
        };
        let rng = gameplay.rng.clone();
        game.with_ctx(|_, ctx| {
            let frame = |g: &mut GameplayLoop, ctx: &mut crate::loops::Ctx| {
                let screen = g.screen.as_mut().unwrap();
                screen.layout(ctx.ui, (1280, 800));
                let width = g.toolbar.as_ref().unwrap().width(ctx.ui);
                g.notify
                    .as_mut()
                    .unwrap()
                    .update(ctx.ui, screen, &g.events, 0.1, width)
            };
            frame(&mut gameplay, ctx);
            // Low food (no cell), a fire at a cell, and a type-26 event (popup, icon 12).
            gameplay
                .events
                .add(22, 3, -1, 0, 1, Default::default(), false);
            let n = gameplay.town.as_ref().unwrap().cells.size as i32;
            gameplay
                .events
                .add(31, 3, 70 * n + 30, 0, 1, Default::default(), false);
            gameplay
                .events
                .add(0x1a, 1, -1, 0, 1, Default::default(), false);
            assert!(frame(&mut gameplay, ctx));
            for _ in 0..40 {
                frame(&mut gameplay, ctx);
            }
            let screen = gameplay.screen.as_ref().unwrap();
            let root = screen
                .dialogs
                .iter()
                .find(|d| ctx.ui.find(d.root, "ribbonNotify").is_some())
                .unwrap()
                .root;
            let content = ctx.ui.find(root, "notifyContent").unwrap();
            let groups = ctx.ui.node(content).children.clone();
            assert_eq!(groups.len(), 6);
            let visible = |ui: &banished_ui::Ui, n| ui.node(n).flags & 1 == 0;
            let button: Vec<_> = groups
                .iter()
                .map(|&g| ctx.ui.find(g, "notifyButton").unwrap())
                .collect();
            let image: Vec<_> = groups
                .iter()
                .map(|&g| ctx.ui.find(g, "notifyImage").unwrap())
                .collect();
            assert!(visible(ctx.ui, image[0]) && !visible(ctx.ui, button[0]));
            assert!(visible(ctx.ui, button[1]) && !visible(ctx.ui, image[1]));
            assert_eq!(
                ctx.ui.node(groups[0]).tooltip,
                "The reserve of food is low!"
            );
            eprintln!(
                "ribbon root {:?} margins {:?}",
                ctx.ui.node(root).rect,
                ctx.ui.node(root).margins
            );
            // The popup and its OK button.
            let popup = screen
                .dialogs
                .iter()
                .find(|d| ctx.ui.find(d.root, "imageIcon").is_some())
                .map(|d| d.root)
                .unwrap();
            let ok = ctx.ui.find(popup, "buttonOk").unwrap();
            let dialogs = screen.dialogs.len();
            gameplay.event(ctx, Event::Clicked(ok));
            assert_eq!(gameplay.screen.as_ref().unwrap().dialogs.len(), dialogs - 1);
            // The fire's button moves the camera to its cell.
            gameplay.event(ctx, Event::Clicked(button[1]));
            let half = (n >> 1) as f32 - 0.5;
            let Some(SceneView {
                camera: SceneCamera::Map(c),
                ..
            }) = &gameplay.view
            else {
                panic!("no map camera");
            };
            assert_eq!(c.target, glam::Vec2::new(30.0 - half, 70.0 - half));
        });
        assert_eq!(rng, gameplay.rng);
    }

    #[test]
    fn speed_loop_smooths_once_and_pause_still_updates_gpu_instances() {
        let Some((mut game, mut gameplay)) = generated() else {
            return;
        };
        let town = gameplay.town.as_mut().expect("generated starting town");
        // Use the first scheduled tree so the quota visits it during this update. Deliberately
        // lag its visible scale to distinguish one smoothing pass from ten speed iterations.
        let id = town
            .placed
            .iter()
            .position(|p| {
                p.alive
                    && town.templates[p.template]
                        .growth
                        .as_ref()
                        .is_some_and(|d| d.f28 > 0.0)
            })
            .expect("growing tree");
        town.placed[id].growth = 0.2;
        let state = town.placed[id].growth_state.as_mut().unwrap();
        state.smoothed = 0.0;
        state.smoothing = true;
        gameplay.speed = 3; // 10 iterations
        game.with_ctx(|_, ctx| {
            gameplay.update(ctx, 0.5);
        });
        assert_eq!(gameplay.calendar.elapsed, 5_000_000);
        let town = gameplay.town.as_ref().unwrap();
        let value = town.placed[id].growth;
        let d = town.templates[town.placed[id].template]
            .growth
            .as_ref()
            .unwrap();
        let target = value.min(1.0).max(d.f60);
        let smoothed = town.placed[id].growth_state.as_ref().unwrap().smoothed;
        assert_eq!(smoothed.to_bits(), (target * 0.009_999_99f32).to_bits());
        let revision = gameplay
            .view
            .as_ref()
            .unwrap()
            .models
            .as_ref()
            .unwrap()
            .revision;
        assert!(revision > 0);

        let mut renderer = pollster::block_on(crate::render::Renderer::headless(320, 200));
        renderer.set_scene(gameplay.view.as_ref().unwrap());
        gameplay.paused = true;
        game.with_ctx(|_, ctx| {
            gameplay.update(ctx, 0.5);
        });
        assert_eq!(gameplay.calendar.elapsed, 5_000_000);
        let town = gameplay.town.as_ref().unwrap();
        assert_eq!(town.placed[id].growth.to_bits(), value.to_bits());
        let after_pause = town.placed[id].growth_state.as_ref().unwrap().smoothed;
        assert_eq!(
            after_pause.to_bits(),
            (smoothed * 0.99 + target * 0.009_999_99f32).to_bits()
        );
        let view = gameplay.view.as_ref().unwrap();
        assert!(view.models.as_ref().unwrap().revision > revision);
        renderer.set_scene(view); // exercises COPY_DST updates on an already uploaded scene
        let rgba = renderer.capture(&crate::render::Frame {
            terrain: Some(view.frame(renderer.aspect(), 5.0)),
            clear: crate::CLEAR,
            ui: None,
        });
        assert_eq!(rgba.len(), 320 * 200 * 4);
        assert!(rgba.chunks_exact(4).any(|pixel| pixel[..3] != [0, 0, 0]));
        eprintln!(
            "generated town: speed/pause growth phases and live GPU instance upload verified"
        );
    }

    #[test]
    fn tree_expiry_plays_packaged_clip_and_removes_after_completion() {
        let Some((mut game, mut gameplay)) = generated() else {
            return;
        };
        let town = gameplay.town.as_mut().unwrap();
        let id = town
            .placed
            .iter()
            .position(|p| {
                p.alive
                    && town.templates[p.template]
                        .natural
                        .as_ref()
                        .is_some_and(|n| n.flag)
            })
            .expect("tree resource");
        let p = &mut town.placed[id];
        p.growth_state.as_mut().unwrap().maximum = 0.01;
        p.growth = 1.0;
        p.scale = [1.0; 4];
        let position = p.position;
        let cell = p.cell;
        let model = town.templates[p.template].model.as_ref().unwrap();
        let mesh = model.variants[p.variant].meshes[p.mesh]
            .as_ref()
            .unwrap()
            .clone();
        let group = town.templates[p.template].animations[p.variant]
            .as_ref()
            .unwrap();
        assert_eq!(group.clips[2].name, "Remove");
        assert_eq!(group.clips[2].duration_ticks, 5_066_667);
        if let SceneCamera::Map(camera) = &mut gameplay.view.as_mut().unwrap().camera {
            camera.target = glam::Vec2::new(position[0], position[1]);
            camera.distance = 18.0;
        }
        // The chosen tree still uses the real quota scheduler. Wait for its terminal growth tick.
        for _ in 0..2000 {
            game.with_ctx(|_, ctx| {
                gameplay.update(ctx, 1.0 / 30.0);
            });
            if gameplay.town.as_ref().unwrap().placed[id]
                .animation
                .is_some()
            {
                break;
            }
        }
        let p = &gameplay.town.as_ref().unwrap().placed[id];
        let playback = p
            .animation
            .as_ref()
            .expect("growth transition started animation 2");
        assert_eq!(playback.clip, 2);
        assert_eq!(playback.elapsed, 0); // created after this iteration's animation phase
        assert!(p.alive);
        assert!(p.removal_requested);
        let standing = gameplay
            .view
            .as_ref()
            .unwrap()
            .models
            .as_ref()
            .unwrap()
            .groups
            .iter()
            .find(|g| g.name == mesh)
            .unwrap()
            .instances[0];

        let mut renderer = pollster::block_on(crate::render::Renderer::headless(640, 400));
        fn capture(
            gameplay: &GameplayLoop,
            renderer: &mut crate::render::Renderer,
            name: &str,
        ) -> Vec<u8> {
            let view = gameplay.view.as_ref().unwrap();
            renderer.set_scene(view);
            let rgba = renderer.capture(&crate::render::Frame {
                terrain: Some(view.frame(renderer.aspect(), gameplay.calendar.seconds())),
                clear: crate::CLEAR,
                ui: None,
            });
            if let Ok(directory) = std::env::var("BANISHED_CAPTURE_TREE_EXPIRY") {
                let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("../..")
                    .join(directory)
                    .join(format!("tree-{name}.png"));
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                let file = std::fs::File::create(path).unwrap();
                let mut encoder = png::Encoder::new(file, 640, 400);
                encoder.set_color(png::ColorType::Rgba);
                encoder.set_depth(png::BitDepth::Eight);
                encoder
                    .write_header()
                    .unwrap()
                    .write_image_data(&rgba)
                    .unwrap();
            }
            rgba
        }
        let before = capture(&gameplay, &mut renderer, "standing");
        for _ in 0..9 {
            game.with_ctx(|_, ctx| {
                gameplay.update(ctx, 0.25);
            });
        }
        let p = &gameplay.town.as_ref().unwrap().placed[id];
        assert_eq!(p.animation.as_ref().unwrap().elapsed, 2_250_000);
        assert_eq!(p.animation.as_ref().unwrap().sample_time, 2_000_000);
        assert!(p.alive);
        let falling = gameplay
            .view
            .as_ref()
            .unwrap()
            .models
            .as_ref()
            .unwrap()
            .groups
            .iter()
            .find(|g| g.name == mesh)
            .unwrap()
            .instances[0];
        assert_ne!(
            standing, falling,
            "each animated tree must receive its own node transform"
        );
        let during = capture(&gameplay, &mut renderer, "falling");
        assert_ne!(before, during);

        gameplay.paused = true;
        game.with_ctx(|_, ctx| {
            gameplay.update(ctx, 1.0);
        });
        assert_eq!(
            gameplay.town.as_ref().unwrap().placed[id]
                .animation
                .as_ref()
                .unwrap()
                .elapsed,
            2_250_000
        );
        gameplay.paused = false;
        // Advance to two ticks before completion: still registered and drawn. Requested f32
        // seconds undergo the original Time constructor's truncation to microsecond ticks.
        game.with_ctx(|_, ctx| {
            gameplay.update(ctx, 2.816666);
        });
        let p = &gameplay.town.as_ref().unwrap().placed[id];
        assert_eq!(p.animation.as_ref().unwrap().elapsed, 5_066_665);
        assert!(p.alive);
        game.with_ctx(|_, ctx| {
            gameplay.update(ctx, 0.000_001_1);
        });
        assert_eq!(
            gameplay.town.as_ref().unwrap().placed[id]
                .animation
                .as_ref()
                .unwrap()
                .elapsed,
            5_066_666
        );
        assert!(gameplay.town.as_ref().unwrap().placed[id].alive);
        game.with_ctx(|_, ctx| {
            gameplay.update(ctx, 0.000_001_1);
        });
        let town = gameplay.town.as_ref().unwrap();
        assert_eq!(
            town.placed[id].animation.as_ref().unwrap().elapsed,
            5_066_667
        );
        assert!(!town.placed[id].alive);
        let index = cell.1 as usize * town.cells.size + cell.0 as usize;
        assert_eq!(town.cells.objects[index], None);
        let terrain = &gameplay.view.as_ref().unwrap().terrain;
        let grid = banished_game::terrain::HeightGrid {
            width: terrain.size + 1,
            heights: &terrain.heights,
        };
        assert_eq!(
            town.cells.types[index],
            grid.natural_cell_type(town.cells.size, index)
        );
        let after = capture(&gameplay, &mut renderer, "removed");
        assert_ne!(during, after);
        eprintln!(
            "tree Remove clip, per-instance pose, pause and exact-duration cleanup verified on GPU"
        );
    }
}

/// What the removal tool hears from the mouse.
#[derive(Debug, Clone, Copy)]
enum DragInput {
    Press,
    Move,
    Release,
}
