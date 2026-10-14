//! First playable `Game/Game` loop: generated world, Banished-styled HUD, calendar and time controls.
//!
//! The original startup installs several specialized dialogs and initializes simulation managers.
//! This port keeps the new-game selections alive and supplies a smaller UI until those managers
//! are implemented.

use banished_formats::game::EnvironmentMonth;
use banished_formats::ui::flags;
use banished_ui::{Event, Kind, Node, NodeId, Ui};

use crate::loops::{Ctx, DialogLoaderLoop, GameLoop, Next, RawInput, Screen, View};
use crate::menu::MenuLoop;
use crate::render::Lighting;
use crate::scene::{Camera, SceneCamera, SceneView};
use crate::toolbar::{Action, Toolbar};
use crate::world::{World, WorldParams};

const YEAR_SECONDS: f32 = 3_600.0;
const MONTHS_PER_YEAR: usize = 12;
const MONTH_SECONDS: f32 = YEAR_SECONDS / MONTHS_PER_YEAR as f32;
const SPEED_LEVELS: [f32; 4] = [1.0, 2.0, 5.0, 10.0];
const FONT_SMALL: &str = r"Font\FontSmall.rsc:resource";
const FONT_MEDIUM: &str = r"Font\FontMedium.rsc:resource";

pub struct GameplayLoop {
    params: WorldParams,
    view: Option<SceneView>,
    environment_months: Vec<EnvironmentMonth>,
    screen: Option<Screen>,
    year: u32,
    month: usize,
    month_elapsed: f32,
    /// Index into the original 1x, 2x, 5x, 10x simulation iteration values.
    speed: u8,
    paused: bool,
    date_label: Option<NodeId>,
    toolbar: Option<Toolbar>,
}

impl GameplayLoop {
    pub fn new(params: WorldParams) -> Self {
        Self {
            params,
            view: None,
            environment_months: Vec::new(),
            screen: None,
            year: 1,
            month: 0,
            month_elapsed: 0.0,
            speed: 0,
            paused: false,
            date_label: None,
            toolbar: None,
        }
    }

    fn add_panel_border(ui: &mut Ui, node: NodeId, border: &str) {
        let border = ui.instantiate(border);
        ui.node_mut(node).border = border;
    }

    fn build_hud(&mut self, ui: &mut Ui, archive: &banished_formats::pkg::Archive) {
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

        // `FUN_14009d0d0` installs the toolbar with the other in-game dialogs.
        let mut screen = screen;
        let mut toolbar = Toolbar::load(archive);
        toolbar.install(ui, &mut screen);
        self.toolbar = Some(toolbar);
        self.screen = Some(screen);
        self.update_hud(ui);
    }

    fn update_hud(&self, ui: &mut Ui) {
        if let Some(date) = self.date_label {
            let key = format!("Month{}", self.month);
            let season = ui.res.string(Some(r"Dialog\StringTable.rsc:seasons"), &key);
            let speed = if self.paused {
                "Paused".to_string()
            } else {
                format!("{}x", SPEED_LEVELS[self.speed as usize] as u32)
            };
            ui.set_text(
                date,
                &format!("Year {}  |  {}  |  {}", self.year, season, speed),
            );
        }
        if let Some(toolbar) = &self.toolbar {
            toolbar.refresh(ui, SPEED_LEVELS[self.speed as usize] as u32, self.paused);
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
                let lighting = Lighting::from_month(&world.environment.months[0]);
                self.environment_months = world.environment.months;
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
        self.build_hud(ctx.ui, ctx.archive);
    }

    fn update(&mut self, ctx: &mut Ctx, dt: f32) -> Next {
        if let (Some(screen), Some(toolbar)) = (&self.screen, &self.toolbar) {
            toolbar.relayout(ctx.ui, screen);
        }
        if !self.paused {
            self.month_elapsed += dt * SPEED_LEVELS[self.speed as usize];
            while self.month_elapsed >= MONTH_SECONDS {
                self.month_elapsed -= MONTH_SECONDS;
                self.month += 1;
                if self.month == MONTHS_PER_YEAR {
                    self.month = 0;
                    self.year += 1;
                }
                self.update_hud(ctx.ui);
            }
        }
        if let (Some(view), Some(month)) =
            (self.view.as_mut(), self.environment_months.get(self.month))
        {
            let next = &self.environment_months[(self.month + 1) % self.environment_months.len()];
            view.lighting =
                Lighting::between_months(month, next, self.month_elapsed / MONTH_SECONDS);
            view.lighting
                .set_seasonal_foliage(self.month as f32 + self.month_elapsed / MONTH_SECONDS);
        }
        Next::Stay
    }

    fn event(&mut self, ctx: &mut Ctx, event: Event) -> Next {
        let (Event::Clicked(id) | Event::Changed(id)) = event else {
            return Next::Stay;
        };
        let (Some(screen), Some(toolbar)) = (self.screen.as_mut(), self.toolbar.as_mut()) else {
            return Next::Stay;
        };
        let Some(action) = toolbar.click(ctx.ui, screen, id) else {
            return Next::Stay;
        };
        match action {
            // `FUN_1400be770` kinds 6-9 write the simulation controller's pause flag and speed.
            Action::Pause => self.paused = true,
            Action::Play => self.paused = false,
            Action::Slower => self.speed = self.speed.saturating_sub(1),
            Action::Faster => self.speed = (self.speed + 1).min((SPEED_LEVELS.len() - 1) as u8),
            other => log::info!("toolbar action not implemented yet: {other:?}"),
        }
        self.update_hud(ctx.ui);
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
