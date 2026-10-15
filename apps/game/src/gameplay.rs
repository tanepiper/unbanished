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
    selection: Selection,
    entities: EntityWindows,
    highlight: crate::highlight::Highlight,
    /// The event log (`Game +0x138`) and the stock manager's warning state.
    pub events: banished_game::events::EventLog,
    stock_warnings: banished_game::events::StockWarnings,
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
            removal: None,
            create_bar: None,
            rotation: 0,
            cursor: (0, 0),
            cursor_seen: false,
            held: [false; keys::COUNT],
            selection: Selection::default(),
            entities: EntityWindows::default(),
            highlight: Default::default(),
            events: Default::default(),
            stock_warnings: Default::default(),
            notify: None,
            aspect: 1.6,
            music: None,
            placement_cell: None,
            move_cooldown: 0.0,
            pending_sounds: Vec::new(),
            ambience: None,
            ambience_stale: false,
            pause_menu: None,
            options: None,
            show_celsius: false,
            weather: Vec::new(),
            river_paths: Vec::new(),
        }
    }

    /// `FUN_14009d0d0` installs the toolbar with the other in-game dialogs.
    fn build_hud(
        &mut self,
        ui: &mut Ui,
        archive: &banished_formats::pkg::Archive,
        keymap: &crate::keymap::KeyMap,
    ) {
        let mut screen = Screen::new(ui);
        let mut toolbar = Toolbar::load(archive);
        toolbar.install(ui, &mut screen, keymap);
        self.toolbar = Some(toolbar);
        self.notify = crate::event_notify::EventNotify::open(ui, &mut screen, &self.events);
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
            stock: self.town.as_ref().map_or([0; 21], |t| {
                banished_game::stock::stock_table(&t.registered_storages(), &t.materials)
            }),
            show_celsius: self.show_celsius,
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
        if self.placement.is_none() && self.removal.is_none() {
            log::info!("{template}: no tool for it yet (drag and path tools not ported)");
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
        match (tool.drag_ends(), tool.drag_size(n)) {
            (Some((a, b)), Some(size)) => {
                if self.create_bar.is_none() {
                    if let (Some(t), Some(screen)) = (tool.toolbar.as_ref(), self.screen.as_mut()) {
                        self.create_bar = CreateBar::open(ctx.ui, screen, ctx.archive, t, None);
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
                    bar.update_drag(ctx.ui, size, visible.then_some(r));
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
    fn iterate(&mut self, dt: f32) -> bool {
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
        self.calendar.advance(ticks(dt));
        if let Some(env) = &mut self.environment {
            env.update(self.calendar.seconds(), dt, &mut self.rng);
        }
        // `FUN_140091780`: recompute stock (`FUN_14008f3a0`), then its warnings
        // (`FUN_1400915d0`, posted through `FUN_14009aa50` at severity 3).
        if let Some(town) = &mut self.town {
            self.stock_warnings
                .recompute(&town.registered_storages(), &town.materials);
            // The job manager's pass and clock (`FUN_14009b850`, after the stock update).
            town.construction.jobs.step(ticks(dt));
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
                    a.fade(ctx.audio, temperature);
                }
            }
        }
        // The precipitation emitters follow the environment's effects (`FUN_1400999b0`); with
        // the game option ShowWeather (`+0x16`) off the effects and their emitters are removed.
        // Not ported: ShowWeather's other effect, the weather lighting blend in `FUN_140098f90`.
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
            Action::Build(Some(template)) => self.start_tool(ctx, &template),
            Action::Build(None) => self.end_placement(ctx),
            Action::Options => self.open_pause_menu(ctx),
            other => log::info!("toolbar action not implemented yet: {other:?}"),
        }
    }

    /// `FUN_1401071b0`: open the pause menu (pauses effects, ambience and music).
    fn open_pause_menu(&mut self, ctx: &mut Ctx) {
        if let (None, Some(screen)) = (&self.pause_menu, self.screen.as_mut()) {
            self.pause_menu = crate::pause_menu::PauseMenu::open(ctx.ui, screen, ctx.audio);
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
        match World::generate(ctx.archive, &self.params) {
            Ok(world) => {
                let lighting = Lighting::from_month(&world.environment.months[0]);
                self.weather = world.environment.weather.clone();
                self.river_paths = world.river_paths.clone();
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
                self.environment = Some(env);
            }
            Err(e) => log::error!("{reference}: {e}"),
        }
    }

    fn startup(&mut self, ctx: &mut Ctx) {
        self.build_hud(ctx.ui, ctx.archive, &ctx.settings.keymap);
    }

    fn update(&mut self, ctx: &mut Ctx, dt: f32) -> Next {
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
        if !self.paused && !pauses_game {
            for _ in 0..SPEED_LEVELS[self.speed as usize] {
                models_changed |= self.iterate(dt);
            }
        }
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
        self.move_cooldown -= dt;
        self.update_audio(ctx, dt);
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
        if let (Some(options), Some(screen)) = (&mut self.options, &mut self.screen) {
            if options.event(ctx, screen, event) == crate::options::Outcome::Closed {
                self.options = None;
            }
            return Next::Stay;
        }
        let (Event::Clicked(id) | Event::Changed(id)) = event else {
            return Next::Stay;
        };
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
                Some(UtilityClick::GoTo(cell)) => {
                    self.go_to(cell);
                    return Next::Stay;
                }
                Some(UtilityClick::Handled) => return Next::Stay,
                None => {}
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
            RawInput::Action(keys::STRUCTURE_LEFT) if self.placement.is_some() => {
                self.rotation = (self.rotation + 3) % 4;
                self.refresh_placement(ctx);
                return Next::Stay;
            }
            RawInput::Action(keys::STRUCTURE_RIGHT) if self.placement.is_some() => {
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
                    // `FUN_1401055c0` / `FUN_140105610`: thunk on placement, error otherwise.
                    let sound = match tool.place(ctx.archive, town, self.rotation) {
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
            RawInput::Escape if self.placement.is_some() || self.removal.is_some() => {
                self.end_placement(ctx);
                if let Some(toolbar) = &self.toolbar {
                    toolbar.clear_build(ctx.ui);
                }
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
                log::info!("script placed {template} as {placed:?}");
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
                return true;
            }
            // `tool TEMPLATE`: start a template's tool, as its toolbar button does.
            Some("tool") => {
                let Some(template) = words.next() else {
                    return false;
                };
                self.start_tool(ctx, template);
                return self.placement.is_some() || self.removal.is_some();
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
