//! First playable `Game/Game` loop: generated world, Banished-styled HUD, calendar and time controls.
//!
//! The original startup installs several specialized dialogs and initializes simulation managers.
//! This port keeps the new-game selections alive and supplies a smaller UI until those managers
//! are implemented.

use banished_formats::ui::flags;
use banished_ui::{Event, Kind, Node, NodeId, Ui};

use crate::loops::{Ctx, DialogLoaderLoop, GameLoop, Next, RawInput, Screen, View};
use crate::menu::MenuLoop;
use crate::render::Lighting;
use crate::scene::{Camera, SceneCamera, SceneView};
use crate::world::{World, WorldParams};

const MONTH_SECONDS: f32 = 12.0;
const FONT_SMALL: &str = r"Font\FontSmall.rsc:resource";
const FONT_MEDIUM: &str = r"Font\FontMedium.rsc:resource";

pub struct GameplayLoop {
    params: WorldParams,
    view: Option<SceneView>,
    month_lighting: Vec<Lighting>,
    screen: Option<Screen>,
    year: u32,
    month: usize,
    month_elapsed: f32,
    /// 0 paused, 1 normal, 2 fast.
    speed: u8,
    date_label: Option<NodeId>,
    pause_button: Option<NodeId>,
}

impl GameplayLoop {
    pub fn new(params: WorldParams) -> Self {
        Self {
            params,
            view: None,
            month_lighting: Vec::new(),
            screen: None,
            year: 1,
            month: 0,
            month_elapsed: 0.0,
            speed: 1,
            date_label: None,
            pause_button: None,
        }
    }

    fn add_panel_border(ui: &mut Ui, node: NodeId, border: &str) {
        let border = ui.instantiate(border);
        ui.node_mut(node).border = border;
    }

    fn add_button(ui: &mut Ui, row: NodeId, name: &str, label: &str) -> NodeId {
        let states = [
            "buttonEnabledBorder",
            "buttonHighlightBorder",
            "buttonPressedBorder",
            "buttonDisabledBorder",
        ]
        .map(|part| ui.instantiate(&format!(r"Dialog\SharedElements.rsc:{part}")));
        let text = ui.text(FONT_MEDIUM, label);
        if let Kind::Text(data) = &mut ui.node_mut(text).kind {
            data.justify = 1;
        }
        ui.node_mut(text).alignment = 4;

        let mut button = Node::new(Kind::Button {
            states,
            content: Some(text),
            colors: [0; 4],
            state: 0,
        });
        button.name = name.to_string();
        button.min_size = [78, 40];
        button.margins = [3, 3, 3, 3];
        let id = ui.add(button);
        ui.add_child(row, id);
        id
    }

    fn build_hud(&mut self, ui: &mut Ui) {
        let screen = Screen::new(ui);
        let mut overlay = Node::new(Kind::Group);
        overlay.flags = flags::FILL_WIDTH | flags::FILL_HEIGHT;
        let overlay = ui.add(overlay);
        ui.add_child(screen.root, overlay);

        let title = ui.text(FONT_MEDIUM, &self.params.town_name);
        ui.node_mut(title).margins = [10, 10, 5, 2];
        let detail_text = format!(
            "{}  |  {}  |  {}  |  Seed {}",
            self.params.terrain, self.params.climate, self.params.start_condition, self.params.seed
        );
        let detail = ui.text(FONT_SMALL, &detail_text);
        ui.node_mut(detail).margins = [10, 10, 2, 5];
        let mut identity = Node::new(Kind::Ribbon {
            vertical: true,
            uniform: false,
            spacing: 0,
            cells: Vec::new(),
        });
        identity.name = "gameIdentity".to_string();
        identity.margins = [12, 0, 12, 0];
        identity.children = vec![title, detail];
        let identity = ui.add(identity);
        Self::add_panel_border(ui, identity, r"Dialog\SharedElements.rsc:dialogBorder");
        ui.add_child(overlay, identity);

        let date = ui.text(FONT_MEDIUM, "Year 1  |  Early Spring");
        ui.node_mut(date).alignment = 2;
        ui.node_mut(date).margins = [0, 12, 12, 0];
        ui.node_mut(date).min_size = [240, 40];
        Self::add_panel_border(ui, date, r"Dialog\SharedElements.rsc:dialogBorder");
        ui.add_child(overlay, date);
        self.date_label = Some(date);

        let mut controls = Node::new(Kind::Ribbon {
            vertical: false,
            uniform: false,
            spacing: 2,
            cells: Vec::new(),
        });
        controls.name = "gameControls".to_string();
        controls.alignment = 8;
        controls.margins = [12, 12, 0, 12];
        let controls = ui.add(controls);
        ui.add_child(overlay, controls);
        Self::add_button(ui, controls, "buttonSlower", "Slower");
        let pause = Self::add_button(ui, controls, "buttonPause", "Pause");
        Self::add_button(ui, controls, "buttonFaster", "Faster");
        Self::add_button(ui, controls, "buttonMenu", "Menu");
        self.pause_button = Some(pause);

        self.screen = Some(screen);
        self.update_hud(ui);
    }

    fn update_hud(&self, ui: &mut Ui) {
        if let Some(date) = self.date_label {
            let key = format!("Month{}", self.month);
            let season = ui.res.string(Some(r"Dialog\StringTable.rsc:seasons"), &key);
            let speed = match self.speed {
                0 => "Paused",
                2 => "4x",
                _ => "1x",
            };
            ui.set_text(
                date,
                &format!("Year {}  |  {}  |  {}", self.year, season, speed),
            );
        }
        if let Some(button) = self.pause_button {
            ui.set_text(button, if self.speed == 0 { "Resume" } else { "Pause" });
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
        match World::generate(ctx.archive, &self.params) {
            Ok(world) => {
                self.month_lighting = world
                    .environment
                    .months
                    .iter()
                    .map(Lighting::from_month)
                    .collect();
                let lighting = self
                    .month_lighting
                    .first()
                    .copied()
                    .unwrap_or_else(|| Lighting::from_month(&world.environment.months[0]));
                log::info!(
                    "{} terrain ready for {} ({}, {})",
                    world.profile.name,
                    self.params.town_name,
                    self.params.climate,
                    self.params.start_condition
                );
                self.view = Some(SceneView::new(
                    world.terrain,
                    SceneCamera::Map(Camera::ground()),
                    lighting,
                ));
            }
            Err(e) => log::error!("world generation failed: {e}"),
        }
    }

    fn startup(&mut self, ctx: &mut Ctx) {
        self.build_hud(ctx.ui);
    }

    fn update(&mut self, ctx: &mut Ctx, dt: f32) -> Next {
        if self.speed != 0 {
            let multiplier = if self.speed == 2 { 4.0 } else { 1.0 };
            self.month_elapsed += dt * multiplier;
            while self.month_elapsed >= MONTH_SECONDS {
                self.month_elapsed -= MONTH_SECONDS;
                self.month += 1;
                if self.month == 12 {
                    self.month = 0;
                    self.year += 1;
                }
                if let (Some(view), Some(&lighting)) =
                    (self.view.as_mut(), self.month_lighting.get(self.month))
                {
                    view.lighting = lighting;
                }
                self.update_hud(ctx.ui);
            }
        }
        Next::Stay
    }

    fn event(&mut self, ctx: &mut Ctx, event: Event) -> Next {
        let Event::Clicked(id) = event else {
            return Next::Stay;
        };
        let Some(screen) = &self.screen else {
            return Next::Stay;
        };
        if ctx.ui.find(screen.root, "buttonPause") == Some(id) {
            self.speed = if self.speed == 0 { 1 } else { 0 };
            self.update_hud(ctx.ui);
        } else if ctx.ui.find(screen.root, "buttonSlower") == Some(id) {
            self.speed = self.speed.saturating_sub(1);
            self.update_hud(ctx.ui);
        } else if ctx.ui.find(screen.root, "buttonFaster") == Some(id) {
            self.speed = (self.speed + 1).min(2);
            self.update_hud(ctx.ui);
        } else if ctx.ui.find(screen.root, "buttonMenu") == Some(id) {
            return Self::return_to_menu();
        }
        Next::Stay
    }

    fn raw(&mut self, _ctx: &mut Ctx, input: RawInput) -> Next {
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
            RawInput::Pan(forward, right) => camera.pan(forward, right, terrain),
            RawInput::Escape => return Self::return_to_menu(),
        }
        Next::Stay
    }

    fn screen(&self) -> Option<&Screen> {
        self.screen.as_ref()
    }

    fn view(&self) -> View<'_> {
        self.view.as_ref().map_or(View::None, View::World)
    }
}
