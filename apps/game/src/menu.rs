//! The main menu loop (vtable `0x140165120`, resource group "Game/Menu") and the New Game dialog.
//!
//! Load (`FUN_14009fe00`) picks one of ten menu scenes with the clock-seeded random generator and
//! finds `Audio/Music/Menu.rsc`; Startup (`FUN_1400a0e00`) plays the music; the `MainMenuDialog`
//! (`FUN_1400b91a0`) maps buttons to actions and adds `Dialog/BuildId.rsc`.

mod controls;
mod game_loop;
mod new_game;
mod scene;
use controls::{combo_index, edit_text, set_parameters};
pub(crate) use controls::{combo_list, select_combo};
use scene::load_menu_scene;

use unbanished_ui::{Control, Event, Kind, NodeId, Ui};

use crate::gfx::render::Lighting;
use crate::loops::{Ctx, DialogLoaderLoop, GameLoop, Next, RawInput, Screen, View};
use crate::view::scene::{MenuCamera, SceneCamera, SceneView};
use crate::view::world::{Terrain, WorldParams};

const MENU_DIALOG: &str = "Dialog/MainMenu.rsc:resource";
const BUILD_ID_DIALOG: &str = "Dialog/BuildId.rsc:resource";
const NEW_GAME_DIALOG: &str = "Dialog/NewGame.rsc:resource";
/// `System::Configuration::CurrentVersion` / `CurrentBuildId` of the supported build
/// (1.0.7, compiled Sep 10 2017 → `yymmdd`).
const VERSION: &str = "1.0.7";
const BUILD_ID: &str = "170910";
const MENU_MUSIC: &str = "Audio/Music/Menu.rsc:resource";
/// `Game/MenuScene/Scene%i.rsc`, chosen with `Rand(10)`.
pub const MENU_SCENES: u32 = 10;

pub struct MenuLoop {
    pub scene_index: u32,
    /// The 3D background (`Game/MenuScene/Scene%i.rsc`); `None` if it failed to load, in which
    /// case only the dialog's backdrop shows, as in the original.
    scene: Option<SceneView>,
    screen: Option<Screen>,
    /// Index of the New Game dialog in `screen.dialogs` while it is open.
    new_game: Option<usize>,
    /// For each selector, the global-list index of every combo item (`ComboElement` item data).
    climate_items: Vec<usize>,
    condition_items: Vec<usize>,
    terrain_items: Vec<usize>,
    /// `Game/Names/Cities.ndb`.
    town_names: Option<unbanished_formats::names::NameDatabase>,
    /// `+0xb62`: `Application::IsPaused` of the last frame.
    app_paused: bool,
    /// The Options dialog while it is open (`MainMenuDialog` element id 4).
    options: Option<crate::ui::options::OptionsDialog>,
    /// The Achievements dialog (id 8) and the list it shows.
    achievements: Option<crate::ui::achievement_ui::AchievementDialog>,
    /// The Scenario (id 9) or Tutorial (id 12) dialog.
    scenarios: Option<crate::ui::scenario_ui::ScenarioDialog>,
    /// A save to start once this frame's update runs (the Load dialog, the `load` script
    /// command).
    load: Option<crate::saves::saved::SavedGame>,
    /// The Load dialog (`MainMenuDialog` id 2, `FUN_1400b8d80` with mode 1).
    save_dialog: Option<crate::ui::save_dialog::SaveLoadDialog>,
    /// The Mods dialog (`MainMenuDialog` id 7, `FUN_1400b8370`).
    mods_dialog: Option<crate::ui::mod_ui::ModDialog>,
    /// The Multiplayer window (not in the original).
    multiplayer: Option<crate::ui::multiplayer::MultiplayerDialog>,
}

/// Shared settings defaults (`FUN_140042850`) before `Game/Last*` persistence is applied:
/// terrain, map size, climate, start condition, disasters.
const DEFAULT_SELECTION: [(&str, usize); 5] = [
    ("comboTerrain", 0),
    ("comboTerrainSize", 0),
    ("comboClimate", 1),
    ("comboCondition", 1),
    ("comboDisaster", 0),
];

impl MenuLoop {
    pub fn new() -> MenuLoop {
        MenuLoop {
            scene_index: 0,
            scene: None,
            screen: None,
            new_game: None,
            climate_items: Vec::new(),
            condition_items: Vec::new(),
            terrain_items: Vec::new(),
            town_names: None,
            app_paused: false,
            options: None,
            achievements: None,
            scenarios: None,
            load: None,
            save_dialog: None,
            mods_dialog: None,
            multiplayer: None,
        }
    }

    fn is(&self, ui: &Ui, dialog: usize, name: &str, id: NodeId) -> bool {
        self.screen
            .as_ref()
            .and_then(|s| s.find(ui, dialog, name))
            .is_some_and(|n| ui.descendant(n, id))
    }
}
