//! Game/Game: the loop that runs a town. This file holds the loop's state, starting and loading
//! a game, the pause menu and the `GameLoop` implementation; the rest is split by topic:
//!
//! - [`simulation`]: one iteration of the town and what follows it;
//! - [`input`]: picking, clicks, window dragging, key actions and UI events;
//! - [`tools`]: the active tool (placement, removal, following, a scenario's restrictions);
//! - [`hud`]: the toolbar, status values and tooltip;
//! - [`audio`]: music and the scene's sound;
//! - [`saving`]: saving the game and autosave;
//! - `tutorial`: tutorials and scenarios.

mod audio;
pub(crate) mod commands;
pub mod digest;
mod hud;
mod input;
pub mod recording;
mod saving;
mod simulation;
mod tools;

use commands::{Outcome, PlayerCommand, Tool};
use input::DragInput;
use saving::SaveTarget;

use banished_formats::game::climate_generator;
use banished_formats::game::EnvironmentMonth;
use banished_game::calendar::Calendar;
use banished_game::nature::environment::{Climate, Environment, YEAR_SECONDS};
use banished_game::random::Random;
use banished_game::scheduler::ticks;
use banished_ui::{Event, Ui};

use crate::gfx::render::Lighting;
use crate::keymap::id as keys;
use crate::loops::{Ctx, DialogLoaderLoop, GameLoop, Next, RawInput, Screen, View};
use crate::menu::MenuLoop;
use crate::start::StartingTown;
use crate::tools::placement_tool::PlacementTool;
use crate::tools::removal_tool::{DragSound, DragStep, RemovalTool};
use crate::tools::selection::{Entity, Selection};
use crate::ui::building_ui::WorkPlaceClick;
use crate::ui::create_bar::CreateBar;
use crate::ui::entity_ui::EntityWindows;
use crate::ui::toolbar::{Action, Toolbar};
use crate::ui::utility::{StatusValues, UtilityClick, UtilityWindows};
use crate::view::scene::{Camera, SceneCamera, SceneView};
use crate::view::world::{World, WorldParams};

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
    Achievements(crate::ui::achievement_ui::AchievementDialog),
    Goal(crate::ui::achievement_ui::GoalDialog),
    Map(crate::view::map_params::MapParamsDialog),
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
    particles: crate::view::particles::Particles,
    /// Each weather slot's rain or snow (`WeatherGeometry`).
    precipitation: Vec<Option<crate::view::weather::Precipitation>>,
    decal_cache: crate::view::scene_assets::DecalCache,
    selection: Selection,
    entities: EntityWindows,
    /// The Follow dialog (`Dialog/Follow.rsc`), opened from a citizen window.
    follow: Option<crate::tools::follow::Follow>,
    /// The Path tool (toolbar Path, `PathTool`).
    path_tool: Option<crate::tools::path_tool::PathTool>,
    /// `+0xc0` / `+0xc1`: a screenshot asked for (`Some(with_ui)`).
    screenshot: Option<bool>,
    highlight: crate::gfx::highlight::Highlight,
    /// The event log (`Game +0x138`) and the stock manager's warning state.
    pub events: banished_game::events::EventLog,
    stock_warnings: banished_game::events::StockWarnings,
    /// The Town Info graphs' history (StockManager `+0x150`, sampled every 75 s).
    statistics: banished_game::work::tracker::Statistics,
    /// The event ribbon (`Dialog/EventNotify.rsc`) and its popups.
    notify: Option<crate::ui::event_notify::EventNotify>,
    /// Screen aspect of the last frame (MapUI's frustum outline).
    aspect: f32,
    /// `Game +0x80`: the music player.
    music: Option<crate::audio::music::MusicPlayer>,
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
    autosave: Option<crate::saves::autosave::AutoSave>,
    /// Updates begun since the game loaded (recordings and replays count by it).
    updates: u64,
    /// The pause menu holds the game (in a replay: the recording says so).
    menu_paused: bool,
    /// A replay ([`recording::replay`]): the recording, not the interface, gives the commands.
    replaying: bool,
    /// The recording being written (`--record`).
    recorder: Option<recording::Recorder>,
    /// The drag tools started so far, by template (`PlayerCommand::Drag` applies them).
    drag_tools: std::collections::BTreeMap<String, RemovalTool>,
    /// The Save and Load dialog over the pause menu.
    save_dialog: Option<crate::ui::save_dialog::SaveLoadDialog>,
    /// The Mods dialog over the pause menu (id 7).
    mods_dialog: Option<crate::ui::mod_ui::ModDialog>,
    /// Map ambience (`FUN_1400a57c0`): environment regions and wind.
    ambience: Option<crate::audio::ambience::Ambience>,
    /// The map changed since the ambience last weighed it.
    ambience_stale: bool,
    /// The pause menu (`Dialog/Pause.rsc`); while open `UserInterface::PausesGame` holds.
    pause_menu: Option<crate::ui::pause_menu::PauseMenu>,
    /// The Options dialog opened from the pause menu (id 4).
    options: Option<crate::ui::options::OptionsDialog>,
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
    saved: Option<std::sync::Arc<crate::saves::saved::SavedGame>>,
    /// The loop was given its scenario (`+0x15c` not −1, `FUN_14009d530`): a scenario's start,
    /// with a new goal even from a save.
    scenario_start: bool,
}

impl GameplayLoop {
    /// A game restored from a save: its settings, the game's Random and the calendar now;
    /// the world and town when the loop loads.
    pub fn from_save(saved: crate::saves::saved::SavedGame) -> Self {
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

    /// `FUN_140108950` for every entity that went this update (a citizen died, a building or
    /// resource was removed): its window closes and, if it was the selection, it goes.
    fn close_removed_windows(&mut self, ui: &mut banished_ui::Ui) {
        let (Some(town), Some(screen)) = (self.town.as_ref(), self.screen.as_mut()) else {
            return;
        };
        let gone: Vec<Entity> = self
            .entities
            .open
            .iter()
            .map(|w| w.entity)
            .filter(|&e| match e {
                Entity::Placed(p) => !town.placed.get(p).is_some_and(|p| p.alive),
                Entity::Citizen(c) => !town.citizens.get(c).is_some_and(|c| c.alive),
            })
            .collect();
        for e in gone {
            self.entities.close_entity(ui, screen, e);
            if self.selection.selected == Some(e) {
                self.selection.selected = None;
            }
        }
    }

    /// The windows' remembered settings to their entities' UIComponents: those of windows
    /// closed since (`OnClosed`), and with `all` every open window's as well
    /// (`UserInterface::SaveAllSettings`, before a game is saved).
    fn store_window_settings(&mut self, ui: &banished_ui::Ui, all: bool) {
        let settings = if all {
            self.entities.take_settings(ui)
        } else {
            std::mem::take(&mut self.entities.saved)
        };
        let Some(town) = self.town.as_mut() else {
            return;
        };
        for (e, bytes) in settings {
            if let Some(site) = town.construction.sites.get_mut(&e) {
                site.ui_settings = bytes;
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
            updates: 0,
            menu_paused: false,
            replaying: false,
            recorder: None,
            drag_tools: Default::default(),
            save_dialog: None,
            mods_dialog: None,
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

    /// `FUN_1401071b0`: open the pause menu (pauses effects, ambience and music).
    fn open_pause_menu(&mut self, ctx: &mut Ctx) {
        if let (None, Some(screen)) = (&self.pause_menu, self.screen.as_mut()) {
            let flags = self.town.as_ref().map_or(0, |t| t.construction.game_flags);
            self.pause_menu = crate::ui::pause_menu::PauseMenu::open(
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
                        Some(crate::view::weather::Precipitation::new(
                            desc,
                            0x7ea7 + i as u32,
                        ))
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
                let water = crate::view::scene_assets::WaterData::from_mesh(
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
                view.extras = Some(crate::view::scene_assets::SceneExtras {
                    sky: crate::view::scene_assets::SkyData::load(
                        ctx.archive,
                        "Models/Sky/SkyMesh.rsc",
                    ),
                    water,
                    decals: Vec::new(),
                });
                self.road_materials = crate::gfx::road_decals::materials(ctx.archive);
                self.decal_descriptions.clear();
                if let Some(town) = &town {
                    view.models = Some(town.models(ctx.archive));
                }
                self.town = town;
                // A new game's ambience regions join the entity manager's bucket 2.
                if let (None, Some(town)) = (&saved, &mut self.town) {
                    let regions = crate::audio::ambience::region_count(
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
                // The ambience's sounds decode now, behind the loading screen, together with
                // the first music track; the emitters and the music still start on the first
                // gameplay frame (`update_audio`).
                let mut effects = self
                    .town
                    .as_ref()
                    .map(|town| crate::audio::ambience::preload_effects(ctx.archive, town))
                    .unwrap_or_default();
                let (month, mood) = self.music_month_mood();
                effects.extend(
                    self.music
                        .get_or_insert_with(|| crate::audio::music::MusicPlayer::load(ctx.archive))
                        .first_track(ctx.audio, month, mood),
                );
                self.view = Some(view);
                // The decals are built now too, so the renderer uploads them with the world;
                // the first frame rebuilds them only if the town changed meanwhile. The sounds
                // decode on other threads meanwhile (native).
                let archive: &banished_formats::pkg::Archive = ctx.archive;
                let effects: Vec<&str> = effects.iter().map(String::as_str).collect();
                let effects = ctx.audio.missing_effects(&effects);
                let (sounds, ()) = banished_formats::par::join(
                    || banished_audio::EffectCache::prepare(archive, &effects),
                    || self.refresh_decals(archive),
                );
                ctx.audio.add_prepared(sounds);
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
        // `--record`: a game loaded from a save is recorded from its first update.
        if self.saved.is_some() && !self.replaying {
            self.recorder = recording::take_request();
        }
    }

    fn update(&mut self, ctx: &mut Ctx, dt: f32) -> Next {
        // A recording's digest of the state this update begins with, every few updates.
        if self
            .recorder
            .as_ref()
            .is_some_and(|r| r.every > 0 && self.updates % r.every == 0)
        {
            match self.digest(ctx, self.updates as u32) {
                Ok(d) => self.recorder.as_mut().unwrap().digest(d),
                Err(e) => log::warn!("recording: {e}"),
            }
        }
        self.updates += 1;
        if let Some(d) = self.mods_dialog.as_mut() {
            d.poll(ctx.ui, ctx.settings, ctx.mods);
        }
        if !self.entities.saved.is_empty() {
            self.store_window_settings(ctx.ui, false);
        }
        self.close_removed_windows(ctx.ui);
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
        // FUN_1400be4a0 runs every update: the build limits follow the town.
        self.refresh_toolbar(ctx.ui);
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
        let pauses_game = match self.replaying {
            true => self.menu_paused,
            false => self.pause_menu.is_some(),
        };
        if !self.replaying && pauses_game != self.menu_paused {
            self.menu_paused = pauses_game;
            if let Some(r) = &mut self.recorder {
                r.menu_pause(self.updates - 1, pauses_game);
            }
        }
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
                let _p = crate::platform::perf::scope("sim.iterate");
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
        {
            let _p = crate::platform::perf::scope("refresh_decals");
            self.refresh_decals(ctx.archive);
        }
        {
            let _p = crate::platform::perf::scope("refresh_particles");
            self.refresh_particles(ctx.archive, dt * iterations as f32);
        }
        {
            let _p = crate::platform::perf::scope("refresh_weather");
            self.refresh_weather(ctx, dt * iterations as f32);
        }
        // FUN_14009b850 runs this phase once after the speed loop, even at speed zero.
        if let (Some(town), Some(view), false) = (&mut self.town, &mut self.view, pauses_game) {
            let grid = banished_game::terrain::HeightGrid {
                width: view.terrain.size + 1,
                heights: &view.terrain.heights,
            };
            {
                let _p = crate::platform::perf::scope("smooth");
                models_changed |= town.smooth(&grid, &mut self.rng);
            }
            self.ambience_stale |= models_changed;
            if models_changed {
                if let Some(models) = &mut view.models {
                    let _p = crate::platform::perf::scope("refresh_models");
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
                    crate::audio::sound::global::WARNING,
                    crate::audio::sound::sound_type::UI,
                );
            }
        }
        if let Some(town) = &self.town {
            let town_ctx = crate::ui::town_hall_ui::TownContext {
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
            if self.town.is_some() {
                use banished_game::{commerce::trade::TradeSetting, work::professions::Command};
                let closed = self.trade_held.iter().filter(|e| !open.contains(e));
                let opened = open.iter().filter(|e| !self.trade_held.contains(e));
                let held: Vec<Command> = closed
                    .map(|&entity| (entity, false))
                    .chain(opened.map(|&entity| (entity, true)))
                    .map(|(entity, on)| Command::Trade {
                        entity,
                        setting: TradeSetting::Held(on),
                    })
                    .collect();
                for command in held {
                    self.execute(ctx.archive, PlayerCommand::Town(command));
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
                    if let Some(sheet) = crate::gfx::status_icons::load_sheet(ctx.archive, &r) {
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
        {
            let _p = crate::platform::perf::scope("audio");
            self.update_audio(ctx, dt);
        }
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

    /// The original's window handlers act inside the UI event, paused or not: WorkPlaceUI's
    /// (`FUN_14008ccb0`) calls `FUN_1400925d0`/`FUN_1400918a0`, `FUN_140096650` and
    /// `FUN_14008cae0` → `FUN_140134fd0` there and then, as BuildUI's pause (`FUN_1400b7370`)
    /// and ProfessionUI's spinner (`FUN_14007b890`) do. The port queues the commands; applying
    /// them after the event is the order the next iteration would apply them in.
    fn event(&mut self, ctx: &mut Ctx, event: Event) -> Next {
        let next = self.window_event(ctx, event);
        if self
            .town
            .as_ref()
            .is_some_and(|t| !t.construction.commands.is_empty())
        {
            let flush = PlayerCommand::ApplyQueued {
                with_calendar: true,
            };
            self.execute(ctx.archive, flush);
        }
        next
    }

    fn raw(&mut self, ctx: &mut Ctx, input: RawInput) -> Next {
        match input {
            RawInput::Action(id) if id < keys::COUNT => self.held[id] = true,
            RawInput::ActionReleased(id) if id < keys::COUNT => self.held[id] = false,
            RawInput::Cursor(..) => self.cursor_seen = true,
            _ => {}
        }
        // The Mods dialog: Escape is buttonCancel's action 8.
        if let (Some(d), Some(screen)) = (self.mods_dialog.as_mut(), self.screen.as_mut()) {
            if let RawInput::Escape = input {
                d.cancel(ctx.ui, screen, ctx.mods, &ctx.settings.persistence);
                self.mods_dialog = None;
            }
            return Next::Stay;
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
                    crate::ui::options::Outcome::Stay
                }
                RawInput::Escape => options.escape(ctx, screen),
                RawInput::Cursor(x, y) => {
                    self.cursor = (x, y);
                    crate::ui::options::Outcome::Stay
                }
                _ => crate::ui::options::Outcome::Stay,
            };
            if outcome == crate::ui::options::Outcome::Closed {
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
                if let (Some(tool), Some(_)) = (self.placement.as_ref(), self.town.as_ref()) {
                    // `FUN_140102b20`: under a RestrictPlayer setup, only on its cells, turned.
                    let allowed =
                        Self::restrict_allows(self.restrict.as_ref(), tool, self.rotation);
                    // `FUN_1401055c0` / `FUN_140105610`: thunk on placement, error otherwise.
                    let placement = tool.placement(self.rotation).filter(|_| allowed);
                    let placed = match placement {
                        Some(p) => match self.execute(ctx.archive, PlayerCommand::Place(p)) {
                            Outcome::Placed(id) => Some(id),
                            _ => None,
                        },
                        None => None,
                    };
                    let Some(tool) = self.placement.as_mut() else {
                        return Next::Stay;
                    };
                    if placed.is_some() {
                        tool.placed();
                    }
                    let sound = match placed {
                        Some(id) => {
                            log::info!("placed {} as entity {id}", tool.reference);
                            crate::audio::sound::global::PLACEMENT_THUNK
                        }
                        None => {
                            log::info!("{}: cannot build here", tool.reference);
                            crate::audio::sound::global::PLACEMENT_ERROR
                        }
                    };
                    ctx.audio
                        .play(ctx.archive, sound, crate::audio::sound::sound_type::UI);
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
            // `FUN_140104bb0`: MouseRight (action 6) first lets the tool cancel what it is doing
            // (DragTool `+0x48`, a drag in progress), else ends the tool; not while a
            // RestrictPlayer setup holds it.
            RawInput::Action(keys::MOUSE_RIGHT) => {
                let restricted = self.restrict.as_ref().is_some_and(|r| !r.place.is_empty());
                let tool =
                    self.placement.is_some() || self.removal.is_some() || self.path_tool.is_some();
                if tool && !restricted && !self.removal.as_mut().is_some_and(|t| t.cancel()) {
                    self.end_placement(ctx);
                    if let Some(toolbar) = &self.toolbar {
                        toolbar.clear_build(ctx.ui);
                    }
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
                self.execute(ctx.archive, PlayerCommand::SetPaused(!self.paused));
                self.update_hud(ctx.ui);
                return Next::Stay;
            }
            // Toolbar0..8, Tool0..9, Camera0..4: the shown button holding that key.
            RawInput::Action(a @ keys::TOOLBAR0..=keys::CAMERA4) => {
                let (Some(screen), Some(toolbar)) = (self.screen.as_mut(), self.toolbar.as_mut())
                else {
                    return Next::Stay;
                };
                if let Some((node, action)) = toolbar.press_action(ctx.ui, screen, a as i32) {
                    // A key press counts as pressing its button for the tutorial's goals.
                    if let (Some(t), true) = (self.tutorial.as_mut(), ctx.ui.node(node).usable()) {
                        t.pressed.push(node);
                    }
                    if let Some(action) = action {
                        self.run_action(ctx, action);
                    }
                }
                self.update_hud(ctx.ui);
                return Next::Stay;
            }
            _ => {}
        }
        let Some(SceneView {
            camera: SceneCamera::Map(camera),
            ..
        }) = &mut self.view
        else {
            return Next::Stay;
        };
        match input {
            RawInput::Drag(dx, dy) => camera.rotate(dx, dy),
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
            // `toggle_pause`: the TogglePause key (captures).
            Some("toggle_pause") => {
                self.execute(ctx.archive, PlayerCommand::SetPaused(!self.paused));
                return true;
            }
            // `speed N`: the game speed, one of the toolbar's levels (1, 2, 5, 10; captures and
            // profiling).
            Some("speed") => {
                let level = words.next().and_then(|w| w.parse::<u32>().ok());
                let Some(i) = SPEED_LEVELS.iter().position(|&l| Some(l) == level) else {
                    return false;
                };
                self.execute(ctx.archive, PlayerCommand::SetSpeed(i as u8));
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
                let (Some(template), true, true) =
                    (words.next(), self.town.is_some(), self.view.is_some())
                else {
                    return false;
                };
                let start = PlayerCommand::StartTool {
                    template: template.to_string(),
                };
                let Outcome::Tool(Some(Tool::Placement(mut tool))) =
                    self.execute(ctx.archive, start)
                else {
                    return false;
                };
                let (Some(town), Some(view)) = (self.town.as_ref(), self.view.as_ref()) else {
                    return false;
                };
                let grid = banished_game::terrain::HeightGrid {
                    width: view.terrain.size + 1,
                    heights: &view.terrain.heights,
                };
                let near = town.start;
                if tool.find_clear_spot(town, &grid, near).is_none() {
                    return false;
                }
                let placed = match tool.placement(0) {
                    Some(p) => match self.execute(ctx.archive, PlayerCommand::Place(p)) {
                        Outcome::Placed(id) => Some(id),
                        _ => None,
                    },
                    None => None,
                };
                let Some(town) = self.town.as_ref() else {
                    return false;
                };
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
                let (Some(template), Some(r), true, true) = (
                    words.next(),
                    words.next().and_then(|w| w.parse::<i32>().ok()),
                    self.town.is_some(),
                    self.view.is_some(),
                ) else {
                    return false;
                };
                let start = PlayerCommand::StartTool {
                    template: template.to_string(),
                };
                let Outcome::Tool(Some(Tool::Removal(tool))) = self.execute(ctx.archive, start)
                else {
                    return false;
                };
                let Some(town) = self.town.as_ref() else {
                    return false;
                };
                let n = town.cells.size as i32;
                let (x, y) = (town.start.0 as i32, town.start.1 as i32);
                let clamp = |v: i32| v.clamp(0, n - 1);
                let a = clamp(y - r) * n + clamp(x - r);
                let b = clamp(y + r) * n + clamp(x + r);
                let drag = crate::gameplay::commands::Drag {
                    start: a,
                    end: b,
                    ..tool.drag_command()
                };
                self.execute(ctx.archive, PlayerCommand::Drag(drag));
                let Some(town) = self.town.as_ref() else {
                    return false;
                };
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
                // `select citizen N`: the N-th living citizen; `select profession NAME N`: the
                // N-th living citizen whose profession's name contains NAME (`fish`, `hunt`),
                // for captures that follow one. Plain `select`: the last placed entity.
                let kind = words.next();
                let entity = if matches!(kind, Some("citizen" | "profession")) {
                    let wanted: Option<Vec<usize>> = (kind == Some("profession")).then(|| {
                        let name = words.next().unwrap_or("").to_lowercase();
                        let list = &town.construction.professions.list;
                        (0..list.len())
                            .filter(|&p| list[p].name.to_lowercase().contains(&name))
                            .collect()
                    });
                    let n = words.next().and_then(|w| w.parse().ok()).unwrap_or(0);
                    let Some(c) = (0..town.citizens.len())
                        .filter(|&i| town.citizens[i].alive)
                        .filter(|&i| {
                            wanted.as_ref().is_none_or(|w| {
                                town.agents
                                    .states
                                    .get(i)
                                    .is_some_and(|s| w.contains(&s.profession))
                            })
                        })
                        .nth(n)
                    else {
                        return false;
                    };
                    Entity::Citizen(c)
                } else {
                    let Some(p) = town.placed.iter().rposition(|p| p.alive) else {
                        return false;
                    };
                    Entity::Placed(p)
                };
                self.selection.selected = Some(entity);
                self.entities
                    .select(ctx.ui, screen, ctx.archive, town, entity);
                return true;
            }
            // `profession INDEX COUNT`: the ProfessionUI spinner.
            Some("profession") => {
                let (Some(profession), Some(count), Some(_)) = (
                    words.next().and_then(|w| w.parse().ok()),
                    words.next().and_then(|w| w.parse().ok()),
                    self.town.as_ref(),
                ) else {
                    return false;
                };
                let set = banished_game::work::professions::Command::SetCount { profession, count };
                self.execute(ctx.archive, PlayerCommand::Town(set));
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

    #[cfg(not(target_arch = "wasm32"))]
    fn town(&self) -> Option<&StartingTown> {
        self.town.as_ref()
    }
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

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests;
