//! First playable `Game/Game` loop: generated world, Banished-styled HUD, calendar and time controls.
//!
//! The original startup installs several specialized dialogs and initializes simulation managers.
//! This port keeps the new-game selections alive and supplies a smaller UI until those managers
//! are implemented.

use banished_formats::game::EnvironmentMonth;
use banished_ui::{Event, Ui};

use crate::loops::{Ctx, DialogLoaderLoop, GameLoop, Next, RawInput, Screen, View};
use crate::menu::MenuLoop;
use crate::render::Lighting;
use crate::scene::{Camera, SceneCamera, SceneView};
use crate::toolbar::{Action, Toolbar};
use crate::utility::{StatusValues, UtilityWindows};
use crate::world::{World, WorldParams};

const YEAR_SECONDS: f32 = 3_600.0;
const MONTHS_PER_YEAR: usize = 12;
const MONTH_SECONDS: f32 = YEAR_SECONDS / MONTHS_PER_YEAR as f32;
const SPEED_LEVELS: [f32; 4] = [1.0, 2.0, 5.0, 10.0];

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
    toolbar: Option<Toolbar>,
    utility: UtilityWindows,
    /// Environment controller temperature (°F, `+0x18`) and weather state (`+0x10`).
    temperature: f32,
    weather: i32,
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
            toolbar: None,
            utility: UtilityWindows::default(),
            temperature: 50.0,
            weather: 3,
        }
    }

    /// `FUN_14009d0d0` installs the toolbar with the other in-game dialogs.
    fn build_hud(&mut self, ui: &mut Ui, archive: &banished_formats::pkg::Archive) {
        let mut screen = Screen::new(ui);
        let mut toolbar = Toolbar::load(archive);
        toolbar.install(ui, &mut screen);
        self.toolbar = Some(toolbar);
        self.screen = Some(screen);
        self.update_hud(ui);
    }

    /// The values the Status Bar's `FUN_14007eda0` reads from the game managers. Managers that
    /// are not ported yet (citizens, stock, weather) report their empty-town values.
    fn status_values(&self) -> StatusValues {
        StatusValues {
            town_name: self.params.town_name.clone(),
            year: self.year,
            month: self.month,
            temperature: self.temperature,
            weather: self.weather,
            ..StatusValues::default()
        }
    }

    fn update_hud(&self, ui: &mut Ui) {
        if let Some(toolbar) = &self.toolbar {
            toolbar.refresh(ui, SPEED_LEVELS[self.speed as usize] as u32, self.paused);
        }
        self.utility.update(ui, &self.status_values());
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
        if let Some(toolbar) = &self.toolbar {
            toolbar.relayout(ctx.ui);
        }
        self.utility.update(ctx.ui, &self.status_values());
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
            // The host dialog's close button (`Dialog/StandardDialog.rsc`).
            if ctx.ui.node(id).name == "buttonClose" {
                self.utility.close_containing(ctx.ui, screen, id);
            }
            return Next::Stay;
        };
        match action {
            // `FUN_1400be770` kinds 6-9 write the simulation controller's pause flag and speed.
            Action::Pause => self.paused = true,
            Action::Play => self.paused = false,
            Action::Slower => self.speed = self.speed.saturating_sub(1),
            Action::Faster => self.speed = (self.speed + 1).min((SPEED_LEVELS.len() - 1) as u8),
            Action::Utility(template) => {
                self.utility.toggle(ctx.ui, screen, ctx.archive, &template);
            }
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
