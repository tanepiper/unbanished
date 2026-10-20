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
mod frame;
mod hud;
mod input;
mod loading;
pub mod lockstep;
pub mod network;
mod probe;
mod raw_input;
pub mod recording;
mod saving;
mod script;
mod simulation;
mod tools;

use commands::{Outcome, PlayerCommand, Tool};
use input::DragInput;
use saving::SaveTarget;

use unbanished_formats::game::climate_generator;
use unbanished_game::nature::environment::{Climate, Environment, YEAR_SECONDS};
use unbanished_game::scheduler::ticks;
use unbanished_ui::{Event, Ui};

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
use unbanished_sim::simulation::Simulation;

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
    root: unbanished_ui::NodeId,
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
    /// The deterministic state: the town, heights, calendar, random stream, environment,
    /// stock warnings, statistics, event log and update count.
    pub sim: Simulation,
    params: WorldParams,
    view: Option<SceneView>,
    /// What drawing the town needs beyond the town (its models' and icons' data).
    town_view: Option<crate::view::town::TownView>,
    screen: Option<Screen>,
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
    /// The road kinds' decal materials, the templates' DecalDescriptions, and what the decals
    /// were built for (road revision, terrain revision, the entities' signature).
    road_materials: Vec<Option<String>>,
    decal_descriptions: Vec<Option<std::sync::Arc<unbanished_formats::game::DecalDescription>>>,
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
    /// The pause menu holds the game (in a replay: the recording says so).
    menu_paused: bool,
    /// A replay ([`recording::replay`]): the recording, not the interface, gives the commands.
    replaying: bool,
    /// The recording being written (`--record`).
    recorder: Option<recording::Recorder>,
    /// A multiplayer game ([`lockstep`]): the turns, and the turns sealed to send.
    lockstep: Option<lockstep::Lockstep>,
    outbox: Vec<lockstep::Message>,
    /// The slot carried out last (lockstep).
    slot_done: Option<u64>,
    /// A multiplayer game's session ([`network`]), and the update boundary it last handled.
    net: Option<network::NetState>,
    boundary_done: Option<u64>,
    /// Commands of a lockstep slot are being carried out (others go to the lockstep).
    carrying_out: bool,
    /// Replaced by a resync's save: it waits for the switch.
    retired: bool,
    /// The other players' Path tools (a Path tool's pick depends on its last pick).
    remote_paths: std::collections::BTreeMap<u8, crate::tools::path_tool::PathTool>,
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
    new_map: Option<unbanished_formats::menu_scene::MapComponent>,
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
        g.sim.restore(&saved);
        g.saved = Some(std::sync::Arc::new(saved));
        g
    }

    /// `FUN_140108950` for every entity that went this update (a citizen died, a building or
    /// resource was removed): its window closes and, if it was the selection, it goes.
    fn close_removed_windows(&mut self, ui: &mut unbanished_ui::Ui) {
        let (Some(town), Some(screen)) = (self.sim.town.as_ref(), self.screen.as_mut()) else {
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
    fn store_window_settings(&mut self, ui: &unbanished_ui::Ui, all: bool) {
        let settings = if all {
            self.entities.take_settings(ui)
        } else {
            std::mem::take(&mut self.entities.saved)
        };
        let Some(town) = self.sim.town.as_mut() else {
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
            sim: Simulation::new(params.seed),
            scenario_start: params.scenario.is_some(),
            params,
            view: None,
            town_view: None,
            screen: None,
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
            notify: None,
            aspect: 1.6,
            music: None,
            placement_cell: None,
            move_cooldown: 0.0,
            pending_sounds: Vec::new(),
            pending_save: None,
            autosave: None,
            menu_paused: false,
            replaying: false,
            recorder: None,
            lockstep: None,
            outbox: Vec::new(),
            slot_done: None,
            remote_paths: Default::default(),
            net: None,
            boundary_done: None,
            carrying_out: false,
            retired: false,
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
        let (Some(town), Some(view)) = (self.sim.town.as_ref(), self.view.as_mut()) else {
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
            let flags = self
                .sim
                .town
                .as_ref()
                .map_or(0, |t| t.construction.game_flags);
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
        self.load_world(ctx)
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
        if self.net.is_some() {
            self.open_players_line(ctx.ui);
        }
        // `--record`: a game loaded from a save is recorded from its first update.
        if self.saved.is_some() && !self.replaying {
            self.recorder = recording::take_request();
        }
    }

    fn update(&mut self, ctx: &mut Ctx, dt: f32) -> Next {
        self.update_frame(ctx, dt)
    }

    /// The original's window handlers act inside the UI event, paused or not: WorkPlaceUI's
    /// (`FUN_14008ccb0`) calls `FUN_1400925d0`/`FUN_1400918a0`, `FUN_140096650` and
    /// `FUN_14008cae0` → `FUN_140134fd0` there and then, as BuildUI's pause (`FUN_1400b7370`)
    /// and ProfessionUI's spinner (`FUN_14007b890`) do. The port queues the commands; applying
    /// them after the event is the order the next iteration would apply them in.
    fn event(&mut self, ctx: &mut Ctx, event: Event) -> Next {
        let next = self.window_event(ctx, event);
        if self
            .sim
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
        self.raw_input(ctx, input)
    }

    fn screen(&self) -> Option<&Screen> {
        self.screen.as_ref()
    }

    /// Capture scripts: `event KIND [CELL]` posts an event at severity 3; `open TEMPLATE`
    /// toggles a utility window.
    fn script(&mut self, ctx: &mut Ctx, command: &str) -> bool {
        self.run_script(ctx, command)
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

    fn probe_world(
        &self,
        what: &str,
        filter: Option<&str>,
    ) -> Option<Result<serde_json::Value, String>> {
        self.world_query(what, filter)
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn town(&self) -> Option<&StartingTown> {
        self.sim.town.as_ref()
    }
}

/// A generated world's terrain block (`FUN_1400b59a0`, version 6): its size, heights,
/// surface, the rivers' and lakes' mesh with the profile's River and Lake materials, and the
/// river lines; no decals yet.
fn new_map(world: &World) -> unbanished_formats::menu_scene::MapComponent {
    let t = &world.terrain;
    let size = t.size as i32;
    let w = &world.water;
    let material = |m: &str| Some(m.replace('/', "\\"));
    unbanished_formats::menu_scene::MapComponent {
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
