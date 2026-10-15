//! The main menu loop (vtable `0x140165120`, resource group "Game/Menu") and the New Game dialog.
//!
//! Load (`FUN_14009fe00`) picks one of ten menu scenes with the clock-seeded random generator and
//! finds `Audio/Music/Menu.rsc`; Startup (`FUN_1400a0e00`) plays the music; the `MainMenuDialog`
//! (`FUN_1400b91a0`) maps buttons to actions and adds `Dialog/BuildId.rsc`.

use banished_audio::Pcm;
use banished_ui::{Control, Event, Kind, NodeId, Ui};

use crate::loops::{Ctx, DialogLoaderLoop, GameLoop, Next, Screen, View};
use crate::render::Lighting;
use crate::scene::{MenuCamera, SceneCamera, SceneView};
use crate::world::{Terrain, WorldParams};

const MENU_DIALOG: &str = "Dialog/MainMenu.rsc:resource";
const BUILD_ID_DIALOG: &str = "Dialog/BuildId.rsc:resource";
const NEW_GAME_DIALOG: &str = "Dialog/NewGame.rsc:resource";
/// `System::Configuration::CurrentVersion` / `CurrentBuildId` of the supported build
/// (1.0.7, compiled Sep 10 2017 → `yymmdd`).
const VERSION: &str = "1.0.7";
const BUILD_ID: &str = "170910";
const MENU_MUSIC: &str = "Audio\\Music\\Menu.rsc:music";
/// `Game/MenuScene/Scene%i.rsc`, chosen with `Rand(10)`.
pub const MENU_SCENES: u32 = 10;

pub struct MenuLoop {
    music: Option<(Pcm, bool)>,
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
    town_names: Option<banished_formats::names::NameDatabase>,
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
            music: None,
            scene_index: 0,
            scene: None,
            screen: None,
            new_game: None,
            climate_items: Vec::new(),
            condition_items: Vec::new(),
            terrain_items: Vec::new(),
            town_names: None,
        }
    }

    fn is(&self, ui: &Ui, dialog: usize, name: &str, id: NodeId) -> bool {
        self.screen
            .as_ref()
            .and_then(|s| s.find(ui, dialog, name))
            .is_some_and(|n| ui.descendant(n, id))
    }

    /// `FUN_140075ba0`: fill the selectors from the global lists (listed entries only, labelled
    /// from their string tables), apply the default selections and show the condition text.
    fn open_new_game(&mut self, ctx: &mut Ctx) {
        let Some(screen) = &mut self.screen else {
            return;
        };
        let Some(index) = screen.add_modal(ctx.ui, NEW_GAME_DIALOG) else {
            return;
        };
        self.new_game = Some(index);
        if let Some(global) = ctx.global.as_ref() {
            fn fill<T>(
                ui: &mut Ui,
                combo: Option<NodeId>,
                choices: &[crate::global::Choice<T>],
            ) -> Vec<usize> {
                let mut map = Vec::new();
                let mut labels = Vec::new();
                for (i, c) in choices.iter().enumerate() {
                    if c.listed {
                        map.push(i);
                        labels.push(ui.res.string(c.label.0.as_deref(), &c.label.1));
                    }
                }
                if let Some(combo) = combo {
                    ui.set_choices(combo, labels);
                }
                map
            }
            let find = |ui: &Ui, name| screen.find(ui, index, name);
            let combo = find(ctx.ui, "comboClimate");
            self.climate_items = fill(ctx.ui, combo, &global.climates);
            let combo = find(ctx.ui, "comboCondition");
            self.condition_items = fill(ctx.ui, combo, &global.start_conditions);
            let combo = find(ctx.ui, "comboTerrain");
            self.terrain_items = fill(ctx.ui, combo, &global.terrains);
        }
        for (name, default) in DEFAULT_SELECTION {
            if let Some(combo) = screen.find(ctx.ui, index, name) {
                select_combo(ctx.ui, combo, default);
            }
        }
        self.new_seed(ctx);
        self.new_town_name(ctx);
        self.refresh_description(ctx);
    }

    fn new_town_name(&mut self, ctx: &mut Ctx) {
        let (Some(screen), Some(d)) = (&self.screen, self.new_game) else {
            return;
        };
        let Some(edit) = screen.find(ctx.ui, d, "editName") else {
            return;
        };
        let Some(db) = &self.town_names else {
            log::warn!("town names unavailable: Game/Names/Cities.ndb did not load");
            return;
        };
        // `FUN_140075ba0`: `FUN_14009ef90(rng, (editName max length / 3) · 2)`. The dialog's
        // own clock-seeded stream (`+0x118`) is the engine default `Random()` here.
        let max_length = match &ctx.ui.node(edit).kind {
            banished_ui::Kind::Control(banished_ui::Control::Edit(e)) => e.max_length as i32,
            _ => 30,
        };
        let name = banished_game::names::town_name(db, ctx.rng, max_length / 3 * 2);
        ctx.ui.set_text(edit, &name);
    }

    /// Event `0x14` on `comboCondition`: show the selected condition's description.
    fn refresh_description(&mut self, ctx: &mut Ctx) {
        let (Some(screen), Some(d), Some(global)) =
            (&self.screen, self.new_game, ctx.global.as_ref())
        else {
            return;
        };
        let selected = screen
            .find(ctx.ui, d, "comboCondition")
            .map_or(0, |c| combo_index(ctx.ui, c));
        let Some(condition) = self
            .condition_items
            .get(selected)
            .and_then(|&i| global.start_conditions.get(i))
        else {
            return;
        };
        let text = ctx
            .ui
            .res
            .string(condition.data.list.as_deref(), &condition.data.description);
        if let Some(desc) = screen.find(ctx.ui, d, "textDescription") {
            ctx.ui.set_text(desc, &text);
        }
    }

    fn new_seed(&mut self, ctx: &mut Ctx) {
        let (Some(screen), Some(d)) = (&self.screen, self.new_game) else {
            return;
        };
        if let Some(edit) = screen.find(ctx.ui, d, "editSeed") {
            // The original draws within the edit box's numeric range; that range is not yet decoded.
            let seed = ctx.rng.rand() & 0x7fff_ffff;
            ctx.ui.set_text(edit, &seed.to_string());
        }
    }

    fn close_new_game(&mut self, ctx: &mut Ctx) {
        if let (Some(screen), Some(d)) = (&mut self.screen, self.new_game.take()) {
            screen.remove_dialog(ctx.ui, d);
        }
    }

    /// `FUN_140075750`: read the selections and start loading the game.
    fn start_game(&mut self, ctx: &mut Ctx) -> Next {
        let (Some(screen), Some(d)) = (&self.screen, self.new_game) else {
            return Next::Stay;
        };
        let terrain_index = screen
            .find(ctx.ui, d, "comboTerrain")
            .map_or(0, |c| combo_index(ctx.ui, c));
        let size_index = screen
            .find(ctx.ui, d, "comboTerrainSize")
            .map_or(0, |c| combo_index(ctx.ui, c));
        let climate_index = screen
            .find(ctx.ui, d, "comboClimate")
            .map_or(0, |c| combo_index(ctx.ui, c));
        let condition_index = screen
            .find(ctx.ui, d, "comboCondition")
            .map_or(0, |c| combo_index(ctx.ui, c));
        let terrain = self
            .terrain_items
            .get(terrain_index)
            .and_then(|&i| ctx.global.as_ref()?.terrains.get(i));
        if let Some(t) = terrain {
            log::info!("terrain profile {}", t.reference);
        }
        let terrain = terrain.map_or_else(|| "Valleys".to_string(), |t| t.name.clone());
        let town_name = screen
            .find(ctx.ui, d, "editName")
            .and_then(|e| edit_text(ctx.ui, e))
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| "New Settlement".to_string());
        let climate_choice = self
            .climate_items
            .get(climate_index)
            .and_then(|&i| ctx.global.as_ref()?.climates.get(i));
        let climate_ref = climate_choice
            .map(|choice| choice.reference.clone())
            .unwrap_or_else(|| WorldParams::default().climate_ref);
        let climate = climate_choice
            .map(|choice| {
                ctx.ui
                    .res
                    .string(choice.label.0.as_deref(), &choice.label.1)
            })
            .unwrap_or_else(|| "Fair".to_string());
        let condition_choice = self
            .condition_items
            .get(condition_index)
            .and_then(|&i| ctx.global.as_ref()?.start_conditions.get(i));
        let start_condition_ref = condition_choice
            .map(|choice| choice.reference.clone())
            .unwrap_or_default();
        let start_condition = condition_choice
            .map(|choice| {
                ctx.ui
                    .res
                    .string(choice.data.list.as_deref(), &choice.data.name)
            })
            .unwrap_or_else(|| "Medium".to_string());
        let seed = screen
            .find(ctx.ui, d, "editSeed")
            .and_then(|e| edit_text(ctx.ui, e))
            .and_then(|t| t.trim().parse::<i64>().ok())
            .unwrap_or(0) as u32;
        // Scripted runs: `BANISHED_SEED` fixes the world seed.
        let seed = std::env::var("BANISHED_SEED")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(seed);
        let params = WorldParams {
            town_name,
            terrain,
            climate,
            climate_ref,
            start_condition,
            start_condition_ref,
            size: size_index,
            seed,
        };
        log::info!(
            "new game: {} / {} / {} terrain {} size {} seed {}",
            params.town_name,
            params.climate,
            params.start_condition,
            params.terrain,
            params.size,
            params.seed
        );
        // `FUN_14009e600`: one of twelve loading screens at random.
        let screen_ref = format!("Dialog/Loading.rsc:gameLoad{}", ctx.rng.rand_below(12));
        Next::Switch(Box::new(DialogLoaderLoop::new(
            screen_ref,
            0.5,
            Box::new(crate::gameplay::GameplayLoop::new(params)),
        )))
    }
}

/// `FUN_14009fe00`: load `Game/MenuScene/Scene{index}.rsc` and set up its camera and lighting.
fn load_menu_scene(ctx: &mut Ctx, index: u32) -> banished_formats::Result<SceneView> {
    let reference = format!("Game/MenuScene/Scene{index}.rsc:resource");
    let scene = banished_formats::menu_scene::menu_scene(&ctx.archive.read_ref(&reference)?)?;
    let models = crate::models::SceneModels::from_menu_scene(ctx.archive, &scene);
    let extras =
        crate::scene_assets::SceneExtras::from_map(ctx.archive, &scene.map, scene.sky.as_deref());
    let map = scene.map;
    let material = map
        .terrain_material
        .clone()
        .unwrap_or_else(|| "Terrain\\Terrain.rsc:resource".into());
    let terrain = Terrain::new(
        ctx.archive,
        &material,
        map.dims[0] as usize,
        map.heights,
        map.surface,
    )?;
    let c = scene.camera;
    let camera = MenuCamera {
        target: c.target.into(),
        base_yaw: c.floats[0],
        pitch: c.floats[1],
        distance: c.floats[2],
        angle: 0.0,
    };
    // Same layout as an EnvironmentMonth: ambient, sun, fog colours; fog start, end, maximum;
    // sun yaw and pitch.
    let mut lighting = Lighting::from_month(&banished_formats::game::EnvironmentMonth {
        ambient_color: scene.colors[0],
        sun_color: scene.colors[1],
        fog_color: scene.colors[2],
        fog_start: scene.floats[0],
        fog_end: scene.floats[1],
        fog_max: scene.floats[2],
        sun_yaw: scene.floats[3],
        sun_pitch: scene.floats[4],
    });
    // `pc_userPosition` from the scene (`FUN_1400a0910`): snow, autumn, leaf loss, rain.
    lighting.user = glam::Vec4::from(scene.vector);
    log::info!(
        "menu scene {index}: {} placed meshes in {} groups",
        scene.placements.len(),
        models.groups.len()
    );
    let mut view = SceneView::new(terrain, SceneCamera::Menu(camera), lighting);
    view.models = Some(models);
    view.extras = Some(extras);
    Ok(view)
}

/// `UI::TextElement::SetParameter`: replace `@0`, `@1`, … in a text element.
fn set_parameters(ui: &mut Ui, id: NodeId, values: &[&str]) {
    let text = match &ui.node(id).kind {
        Kind::Text(t) | Kind::Label { text: t, .. } => t.text.clone(),
        _ => return,
    };
    let mut out = text;
    for (i, v) in values.iter().enumerate() {
        out = out.replace(&format!("@{i}"), v);
    }
    ui.set_text(id, &out);
}

pub(crate) fn combo_list(ui: &Ui, combo: NodeId) -> Option<NodeId> {
    match ui.node(combo).kind {
        Kind::Control(Control::Combo { list, .. }) => Some(list),
        _ => None,
    }
}

fn combo_index(ui: &Ui, combo: NodeId) -> usize {
    combo_list(ui, combo)
        .and_then(|l| match &ui.node(l).kind {
            Kind::List(data) => data.selected,
            _ => None,
        })
        .unwrap_or(0)
}

/// `ComboElement::SetCurrentItem`: select an item and show its text on the button.
pub(crate) fn select_combo(ui: &mut Ui, combo: NodeId, index: usize) {
    let Some(list) = combo_list(ui, combo) else {
        return;
    };
    let text = match &ui.node(list).kind {
        Kind::List(data) => data.items.get(index).cloned(),
        _ => None,
    };
    let Some(text) = text else { return };
    ui.list_select(list, Some(index));
    ui.set_text(combo, &text);
}

fn edit_text(ui: &Ui, edit: NodeId) -> Option<String> {
    match &ui.node(edit).kind {
        Kind::Control(Control::Edit(e)) => Some(e.text.text.clone()),
        _ => None,
    }
}

impl GameLoop for MenuLoop {
    fn name(&self) -> &'static str {
        "Game/Menu"
    }

    fn load(&mut self, ctx: &mut Ctx) {
        match ctx
            .archive
            .read("Game/Names/Cities_resource.crs")
            .and_then(|bytes| banished_formats::names::NameDatabase::read(&bytes))
        {
            Ok(database) => self.town_names = Some(database),
            Err(e) => log::error!("town names: {e}"),
        }
        self.scene_index = ctx.rng.rand_below(MENU_SCENES);
        if let Some(forced) = std::env::var("BANISHED_MENU_SCENE")
            .ok()
            .and_then(|v| v.parse().ok())
        {
            self.scene_index = forced;
        }
        match load_menu_scene(ctx, self.scene_index) {
            Ok(view) => self.scene = Some(view),
            Err(e) => log::error!("menu scene {}: {e}", self.scene_index),
        }
        match banished_audio::load_sound(ctx.archive, MENU_MUSIC) {
            Ok(music) => self.music = Some(music),
            Err(e) => log::error!("menu music: {e}"),
        }
    }

    fn startup(&mut self, ctx: &mut Ctx) {
        let mut screen = Screen::new(ctx.ui);
        screen.add_dialog(ctx.ui, MENU_DIALOG);
        if let Some(d) = screen.add_dialog(ctx.ui, BUILD_ID_DIALOG) {
            if let Some(notice) = screen.find(ctx.ui, d, "textBuildNotice") {
                set_parameters(ctx.ui, notice, &[VERSION, BUILD_ID]);
            }
        }
        self.screen = Some(screen);
        if let Some((pcm, looping)) = self.music.clone() {
            if let Err(e) = ctx.audio.play(pcm, looping) {
                log::warn!("menu music: {e}");
            }
        }
    }

    /// `FUN_1400a0750`: `Audio::Scene::StopAll`.
    fn shutdown(&mut self, ctx: &mut Ctx) {
        ctx.audio.stop();
    }

    fn update(&mut self, _ctx: &mut Ctx, dt: f32) -> Next {
        if let Some(SceneView {
            camera: SceneCamera::Menu(camera),
            ..
        }) = &mut self.scene
        {
            camera.update(dt);
        }
        Next::Stay
    }

    fn view(&self) -> View<'_> {
        self.scene.as_ref().map_or(View::None, View::World)
    }

    fn event(&mut self, ctx: &mut Ctx, event: Event) -> Next {
        if let (Event::Selected(..), Some(_)) = (event, self.new_game) {
            self.refresh_description(ctx);
            return Next::Stay;
        }
        let Event::Clicked(id) = event else {
            return Next::Stay;
        };
        if let Some(d) = self.new_game {
            // NewGame IDs (FUN_140075ba0): buttonNewName 0, buttonNewSeed 1, buttonOk 2 (starts
            // the game), buttonCancel 3 (closes). See docs/NEW-GAME-FLOW.md.
            if self.is(ctx.ui, d, "buttonNewSeed", id) {
                self.new_seed(ctx);
            } else if self.is(ctx.ui, d, "buttonOk", id) {
                return self.start_game(ctx);
            } else if self.is(ctx.ui, d, "buttonCancel", id) {
                self.close_new_game(ctx);
            } else if self.is(ctx.ui, d, "buttonNewName", id) {
                self.new_town_name(ctx);
            }
            return Next::Stay;
        }
        if self.is(ctx.ui, 0, "buttonNew", id) {
            self.open_new_game(ctx);
        } else if self.is(ctx.ui, 0, "buttonQuit", id) {
            return Next::Quit;
        } else {
            log::info!("menu: {} is not implemented yet", ctx.ui.node(id).name);
        }
        Next::Stay
    }

    fn screen(&self) -> Option<&Screen> {
        self.screen.as_ref()
    }
}
