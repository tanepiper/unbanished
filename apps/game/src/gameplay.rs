//! Game/Game: calendar, climate, scheduled natural-resource growth and packaged gameplay UI.
//! Citizen AI, production and the remaining component/manager callbacks still await ports.

use banished_formats::game::climate_generator;
use banished_formats::game::EnvironmentMonth;
use banished_game::calendar::Calendar;
use banished_game::environment::{Climate, Environment, YEAR_SECONDS};
use banished_game::random::Random;
use banished_game::scheduler::ticks;
use banished_ui::{Event, Ui};

use crate::create_bar::CreateBar;
use crate::entity_ui::EntityWindows;
use crate::loops::{Ctx, DialogLoaderLoop, GameLoop, Next, RawInput, Screen, View};
use crate::menu::MenuLoop;
use crate::placement_tool::PlacementTool;
use crate::render::Lighting;
use crate::scene::{Camera, SceneCamera, SceneView};
use crate::selection::{Entity, Selection};
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
    create_bar: Option<CreateBar>,
    rotation: i32,
    /// Last cursor position in window pixels.
    cursor: (i32, i32),
    selection: Selection,
    entities: EntityWindows,
    highlight: crate::highlight::Highlight,
    /// The event log (`Game +0x138`) and the stock manager's warning state.
    pub events: banished_game::events::EventLog,
    stock_warnings: banished_game::events::StockWarnings,
}

impl GameplayLoop {
    pub fn new(params: WorldParams) -> Self {
        Self {
            rng: Random::new(params.seed),
            params,
            view: None,
            environment_months: Vec::new(),
            screen: None,
            calendar: Calendar::default(),
            environment: None,
            town: None,
            speed: 0,
            paused: false,
            toolbar: None,
            utility: UtilityWindows::default(),
            placement: None,
            create_bar: None,
            rotation: 0,
            cursor: (0, 0),
            selection: Selection::default(),
            entities: EntityWindows::default(),
            highlight: Default::default(),
            events: Default::default(),
            stock_warnings: Default::default(),
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

    /// The values the Status Bar's FUN_14007eda0 reads from the starting town and environment.
    fn status_values(&self) -> StatusValues {
        let summary = self.town.as_ref().map_or_else(Default::default, |t| {
            banished_game::status::aggregate(t.citizens.iter().zip(&t.agents.states).map(
                |(c, s)| banished_game::status::CitizenValue {
                    age: c.age,
                    adult_age: t.rules.adult_age,
                    school: c.school.is_some(),
                    health: s.health,
                    happiness: s.happiness,
                    status: s.status,
                },
            ))
        });
        StatusValues {
            events: Some(self.events.clone()),
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
            // Every adult starts as a laborer (`FUN_14011c550` with the manager's `+0x30`).
            professions: vec![self.population().0],
            // `FUN_14008f3a0`, recomputed from the town storages.
            stock: self.town.as_ref().map_or([0; 21], |t| {
                banished_game::stock::stock_table(&t.storages, &t.materials)
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
        let Some((origin, direction)) = self.cursor_ray(ctx.ui.screen) else {
            return;
        };
        let Some(town) = self.town.as_ref() else {
            return;
        };
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
        // Deviation: start structures do not stamp Map +8 yet. Derive their UI-only terrain
        // lookup from the same rectangles until registration supplies the original cell pointer.
        let fallback = fallback.or_else(|| {
            cell.and_then(|(x, y)| {
                town.placed
                    .iter()
                    .position(|p| {
                        p.alive
                            && town.templates[p.template].storage.is_some()
                            && x >= p.cell.0
                            && y >= p.cell.1
                            && x < p.cell.0 + p.footprint.0
                            && y < p.cell.1 + p.footprint.1
                    })
                    .map(Entity::Placed)
            })
        });
        let picked = self.selection.cast(
            ctx.archive,
            town,
            origin,
            direction,
            ground.map_or(400., |g| g.0),
            fallback,
        );
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
        let mut r = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
        for x in [lo.x, hi.x] {
            for y in [lo.y, hi.y] {
                for z in [lo.z, hi.z] {
                    let p = vp * glam::Vec4::new(x, y, z, 1.);
                    if p.w <= 0. {
                        return;
                    }
                    let x = (p.x / p.w * 0.5 + 0.5) * screen[0] as f32;
                    let y = (0.5 - p.y / p.w * 0.5) * screen[1] as f32;
                    r = [r[0].min(x), r[1].min(y), r[2].max(x), r[3].max(y)];
                }
            }
        }
        self.entities.position(ctx.ui, entity, r);
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
        if let Some(bar) = &mut self.create_bar {
            bar.update(ctx.ui, tool.size(), tool.valid(), rect);
        }
    }

    fn end_placement(&mut self, ctx: &mut Ctx) {
        self.placement = None;
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
    fn iterate(&mut self, dt: f32) -> bool {
        let mut models_changed = false;
        if let (Some(town), Some(view)) = (&mut self.town, &self.view) {
            let grid = banished_game::terrain::HeightGrid {
                width: view.terrain.size + 1,
                heights: &view.terrain.heights,
            };
            let temperature = self.environment.as_ref().map_or(0.0, |e| e.temperature);
            let changed = town.iterate(&grid, &mut self.rng, dt, temperature);
            // AnimationManager::Update runs inside the same iteration (FUN_14009b850).
            let changed = town.update_citizens(&grid, &mut self.rng, dt) | changed;
            // Animation::ApplyEvents now emits the original event resources at their traced
            // intervals. The spatial AnimAudioEvent/SoundEffect controller is still unported.
            for (entity, reference) in town.animation_events.drain(..) {
                log::debug!(
                    "animation event for entity {entity}: {reference} (spatial audio pending)"
                );
            }
            models_changed = changed;
        }
        self.calendar.advance(ticks(dt));
        if let Some(env) = &mut self.environment {
            env.update(self.calendar.seconds(), &mut self.rng);
        }
        // `FUN_140091780`: recompute stock (`FUN_14008f3a0`), then its warnings
        // (`FUN_1400915d0`, posted through `FUN_14009aa50` at severity 3).
        if let Some(town) = &self.town {
            self.stock_warnings
                .recompute(&town.storages, &town.materials);
            let (month, year) = (self.month(), self.calendar.year as i32);
            for kind in self.stock_warnings.check(ticks(dt)) {
                // Not ported: the AutoSlow option (default on) slows the game; nothing reads
                // `slow_request` yet.
                self.events
                    .add(kind, 3, -1, month, year, Default::default(), true);
            }
        }
        models_changed
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
        // PlacementTool slot +0x38 runs every frame: ghost, validity and the CreateBar.
        if self.placement.is_some() {
            self.refresh_placement(ctx);
        }
        let mut models_changed = false;
        if !self.paused {
            for _ in 0..SPEED_LEVELS[self.speed as usize] {
                models_changed |= self.iterate(dt);
            }
        }
        // FUN_14009b850 runs this phase once after the speed loop, even at speed zero.
        if let (Some(town), Some(view)) = (&mut self.town, &mut self.view) {
            let grid = banished_game::terrain::HeightGrid {
                width: view.terrain.size + 1,
                heights: &view.terrain.heights,
            };
            models_changed |= town.smooth(&grid, &mut self.rng);
            if models_changed {
                if let Some(models) = &mut view.models {
                    town.refresh_models(ctx.archive, models);
                }
            }
        }
        self.utility.update(ctx.ui, &self.status_values());
        if let Some(town) = &self.town {
            self.entities.refresh(ctx.ui, ctx.archive, town);
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
            for r in town.icon_sheet_refs() {
                if !view.icon_sheets.contains_key(&r) {
                    if let Some(sheet) = crate::status_icons::load_sheet(ctx.archive, &r) {
                        view.icon_sheets.insert(r, sheet);
                    }
                }
            }
        }
        let month = self.month();
        let into_month = (self.calendar.seconds() - month as f32 * MONTH_SECONDS) / MONTH_SECONDS;
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
            if self.utility.click(id) {
                return Next::Stay;
            }
            // The host dialog's close button (`Dialog/StandardDialog.rsc`).
            if ctx.ui.node(id).name == "buttonClose" {
                self.utility.close_containing(ctx.ui, screen, id);
                if self.entities.close_containing(ctx.ui, screen, id) == self.selection.selected {
                    self.selection.selected = None;
                }
            } else if ctx.ui.node(id).name == "checkPin" {
                self.entities.pin(ctx.ui, id);
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
            // Toolbar kind 1 → FUN_140108030: start (or end) the build tool.
            Action::Build(Some(template)) => {
                self.placement = self.town.as_mut().and_then(|town| {
                    PlacementTool::start(ctx.archive, town, &template, &mut self.rng)
                });
                if let (Some(tool), Some(screen)) = (&self.placement, self.screen.as_mut()) {
                    if let Some(old) = self.create_bar.take() {
                        old.close(ctx.ui, screen);
                    }
                    self.create_bar = tool.toolbar.as_ref().and_then(|t| {
                        CreateBar::open(ctx.ui, screen, ctx.archive, t, tool.build.as_ref())
                    });
                } else {
                    log::info!("{template}: not a placed building (drag/path tools not ported)");
                }
                self.refresh_placement(ctx);
            }
            Action::Build(None) => self.end_placement(ctx),
            other => log::info!("toolbar action not implemented yet: {other:?}"),
        }
        self.update_hud(ctx.ui);
        Next::Stay
    }

    fn raw(&mut self, ctx: &mut Ctx, input: RawInput) -> Next {
        match input {
            RawInput::Cursor(x, y) => {
                self.cursor = (x, y);
                if self.placement.is_some() {
                    self.refresh_placement(ctx);
                }
                return Next::Stay;
            }
            // StructureLeft / StructureRight turn the RotationSetting through 0..3.
            RawInput::Key('r') if self.placement.is_some() => {
                self.rotation = (self.rotation + 3) % 4;
                self.refresh_placement(ctx);
                return Next::Stay;
            }
            RawInput::Key('t') if self.placement.is_some() => {
                self.rotation = (self.rotation + 1) % 4;
                self.refresh_placement(ctx);
                return Next::Stay;
            }
            RawInput::Key('f') => {
                if let Some(tool) = &mut self.placement {
                    tool.next_variant();
                    self.refresh_placement(ctx);
                }
                return Next::Stay;
            }
            RawInput::Click => {
                if let (Some(tool), Some(town)) = (self.placement.as_mut(), self.town.as_mut()) {
                    match tool.place(town, self.rotation) {
                        Some(id) => log::info!("placed {} as entity {id}", tool.reference),
                        None => log::info!("{}: cannot build here", tool.reference),
                    }
                    self.refresh_placement(ctx);
                } else {
                    self.select_at_cursor(ctx);
                    self.position_entity_window(ctx);
                }
                return Next::Stay;
            }
            RawInput::Escape if self.placement.is_some() => {
                self.end_placement(ctx);
                if let Some(toolbar) = &self.toolbar {
                    toolbar.clear_build(ctx.ui);
                }
                return Next::Stay;
            }
            RawInput::Key(_) => return Next::Stay,
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
            RawInput::Pan(forward, right) => camera.pan(forward, right, terrain),
            RawInput::Escape => return Self::return_to_menu(),
            _ => {}
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
        let mut game = crate::Game::new(packages).unwrap();
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
