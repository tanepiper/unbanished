//! First playable `Game/Game` loop: generated world, Banished-styled HUD, calendar and time controls.
//!
//! The original startup installs several specialized dialogs and initializes simulation managers.
//! This port keeps the new-game selections alive and supplies a smaller UI until those managers
//! are implemented.

use banished_formats::game::climate_generator;
use banished_formats::game::EnvironmentMonth;
use banished_game::environment::{Climate, Environment, YEAR_SECONDS};
use banished_game::random::Random;
use banished_ui::{Event, Ui};

use crate::loops::{Ctx, DialogLoaderLoop, GameLoop, Next, RawInput, Screen, View};
use crate::menu::MenuLoop;
use crate::render::Lighting;
use crate::scene::{Camera, SceneCamera, SceneView};
use crate::start::StartingTown;
use crate::toolbar::{Action, Toolbar};
use crate::utility::{StatusValues, UtilityWindows};
use crate::world::{World, WorldParams};

const MONTHS_PER_YEAR: usize = 12;
const MONTH_SECONDS: f32 = YEAR_SECONDS / MONTHS_PER_YEAR as f32;
/// Simulation iterations per frame (`Game/Game +0x120 → +0x34`).
const SPEED_LEVELS: [u32; 4] = [1, 2, 5, 10];

pub struct GameplayLoop {
    params: WorldParams,
    view: Option<SceneView>,
    environment_months: Vec<EnvironmentMonth>,
    screen: Option<Screen>,
    /// Calendar (`Game/Game +0x128`): year count and time into the year.
    year: u32,
    elapsed: f32,
    /// The game's shared random stream (`Game/Game +0xd8`, `Random(seed)` in `FUN_14009bfd0`).
    rng: Random,
    environment: Option<Environment>,
    town: Option<StartingTown>,
    /// Index into the original 1x, 2x, 5x, 10x simulation iteration values.
    speed: u8,
    paused: bool,
    toolbar: Option<Toolbar>,
    utility: UtilityWindows,
}

impl GameplayLoop {
    pub fn new(params: WorldParams) -> Self {
        Self {
            rng: Random::new(params.seed),
            params,
            view: None,
            environment_months: Vec::new(),
            screen: None,
            year: 1,
            elapsed: 0.0,
            environment: None,
            town: None,
            speed: 0,
            paused: false,
            toolbar: None,
            utility: UtilityWindows::default(),
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
            month: self.month(),
            temperature: self.environment.as_ref().map_or(0.0, |e| e.temperature),
            weather: self.environment.as_ref().map_or(3, |e| e.state),
            ..StatusValues::default()
        }
    }

    fn update_hud(&self, ui: &mut Ui) {
        if let Some(toolbar) = &self.toolbar {
            toolbar.refresh(ui, SPEED_LEVELS[self.speed as usize], self.paused);
        }
        self.utility.update(ui, &self.status_values());
    }

    /// `floor(elapsed / 3600 × 12)`.
    fn month(&self) -> usize {
        ((self.elapsed / YEAR_SECONDS * MONTHS_PER_YEAR as f32) as usize).min(MONTHS_PER_YEAR - 1)
    }

    /// One simulation iteration of `FUN_14009b850`: the calendar, then the environment
    /// (`FUN_1400999b0`). Entities, animation and the stock/citizen managers are not ported yet.
    fn iterate(&mut self, dt: f32) {
        self.elapsed += dt;
        if self.elapsed >= YEAR_SECONDS {
            self.year += 1;
            self.elapsed -= YEAR_SECONDS;
        }
        if let Some(env) = &mut self.environment {
            env.update(self.elapsed, &mut self.rng);
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
                // `FUN_1400a87d0`: start point, structures and natural resources, drawn from
                // the game stream before anything else consumes it.
                let grid = banished_game::terrain::HeightGrid {
                    width: world.terrain.size + 1,
                    heights: &world.terrain.heights,
                };
                let town = StartingTown::create(
                    ctx.archive,
                    &self.params.start_condition_ref,
                    world.terrain.size / 2,
                    &grid,
                    &mut self.rng,
                );
                // Inferred: the camera opens over the start point (not yet traced).
                let mut camera = Camera::ground();
                if let Some(town) = &town {
                    let c = (world.terrain.size / 4) as f32 - 0.5;
                    camera.target =
                        glam::Vec2::new(town.start.0 as f32 - c, town.start.1 as f32 - c);
                }
                let mut view = SceneView::new(world.terrain, SceneCamera::Map(camera), lighting);
                if let Some(town) = &town {
                    view.models = Some(town.models(ctx.archive));
                }
                self.town = town;
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
            Ok(c) => self.environment = Some(Environment::new(Climate::from(&c))),
            Err(e) => log::error!("{reference}: {e}"),
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
            for _ in 0..SPEED_LEVELS[self.speed as usize] {
                self.iterate(dt);
            }
        }
        let month = self.month();
        let into_month = (self.elapsed - month as f32 * MONTH_SECONDS) / MONTH_SECONDS;
        if let (Some(view), Some(from)) = (self.view.as_mut(), self.environment_months.get(month)) {
            // `FUN_140098f90`: blend this month's lighting towards the next.
            let to = &self.environment_months[(month + 1) % self.environment_months.len()];
            view.lighting = Lighting::between_months(from, to, into_month);
            if let Some(env) = &self.environment {
                let s = env.season;
                view.lighting.user = glam::Vec4::new(s.snow, s.autumn, s.leaf_loss, s.rain);
            }
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
