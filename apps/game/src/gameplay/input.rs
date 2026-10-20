//! Player input on the map and the windows: picking under the cursor, selection clicks, window dragging, map clicks, key actions and UI events.

use super::*;

/// What the removal tool hears from the mouse.
#[derive(Debug, Clone, Copy)]
pub(super) enum DragInput {
    Press,
    Move,
    Release,
}

impl GameplayLoop {
    /// FUN_140105a70: ray from the camera eye through the viewport coordinate.
    pub(super) fn cursor_ray(&self, screen: [i32; 2]) -> Option<(glam::Vec3, glam::Vec3)> {
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

    pub(super) fn select_at_cursor(&mut self, ctx: &mut Ctx) {
        let picked = self.entity_at_cursor(ctx);
        let Some(town) = self.town.as_ref() else {
            return;
        };
        // `FUN_140102500`: under a RestrictPlayer setup only its template is selected.
        let picked = picked.filter(|e| {
            self.restrict.as_ref().is_none_or(|r| {
                !r.select.is_empty()
                    && e.template(town)
                        .split(':')
                        .next()
                        .unwrap_or("")
                        .eq_ignore_ascii_case(r.select.split(':').next().unwrap_or(""))
            })
        });
        if let Some(entity) = picked {
            if self.selection.selected != Some(entity) {
                self.selection.selected = Some(entity);
                log::info!("selected {entity:?} {}", entity.template(town));
                if let Some(screen) = self.screen.as_mut() {
                    self.entities
                        .select(ctx.ui, screen, ctx.archive, town, entity);
                }
            }
        }
    }

    /// What is under the cursor: the map cell's entity, else the picking scene's hit
    /// (`FUN_140102500`, `FUN_1400e3b80`). The cell's entity is `FUN_1400a5750`: the occupant
    /// (`+0x00`) first, then the object (`+0x08`). A construction site is only an occupant
    /// until its clearing is done (no model stage, so no picking mesh), and an orchard or a
    /// field stays the occupant of the cells its trees and crops are the objects of.
    pub(super) fn entity_at_cursor(&mut self, ctx: &mut Ctx) -> Option<Entity> {
        let (origin, direction) = self.cursor_ray(ctx.ui.screen)?;
        let town = self.town.as_ref()?;
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
                    .entity_at(y as usize * town.cells.size + x as usize)
            })
            .map(Entity::Placed);
        self.selection.cast(
            ctx.archive,
            town,
            origin,
            direction,
            ground.map_or(400., |g| g.0),
            fallback,
        )
    }

    /// PathTool's input (`FUN_1400e3b80`): pick, highlight it (`FUN_1400e25c0`) and draw its
    /// paths (`FUN_1400e2880`).
    pub(super) fn path_click(&mut self, ctx: &mut Ctx) {
        let picked = self.entity_at_cursor(ctx);
        let (Some(entity), true, true) = (picked, self.path_tool.is_some(), self.town.is_some())
        else {
            return;
        };
        self.execute(ctx.archive, PlayerCommand::PathQuery { entity });
        let (Some(tool), Some(town)) = (self.path_tool.as_mut(), self.town.as_mut()) else {
            return;
        };
        self.selection.selected = Some(entity);
        let n = town.cells.size as i32;
        let (lines, cells) = tool.overlay(n);
        if let Some(view) = self.view.as_mut() {
            let grid = banished_game::terrain::HeightGrid {
                width: view.terrain.size + 1,
                heights: &view.terrain.heights,
            };
            let height = |x: f32, y: f32| {
                let mut normal = [0.0; 3];
                grid.ground(x, y, &mut normal)
            };
            let mut vertices = crate::gfx::selection_overlay::line_vertices(&lines, n, height);
            vertices.extend(crate::gfx::selection_overlay::vertices(&cells, n, height));
            view.cell_overlay = vertices;
            if view.overlay_texture.is_none() {
                view.overlay_texture = crate::gfx::selection_overlay::load_texture(ctx.archive);
            }
        }
    }

    pub(super) fn position_entity_window(&mut self, ctx: &mut Ctx) {
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
        // `FUN_140105850` → `FUN_140103e30`: the corners in front of the camera; the rectangle
        // must touch the screen, and is clamped to it.
        let (w, h) = (screen[0] as f32, screen[1] as f32);
        let mut r = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
        let mut any = false;
        for x in [lo.x, hi.x] {
            for y in [lo.y, hi.y] {
                for z in [lo.z, hi.z] {
                    let p = vp * glam::Vec4::new(x, y, z, 1.);
                    if p.w <= 0. {
                        continue;
                    }
                    any = true;
                    let x = (p.x / p.w * 0.5 + 0.5) * w;
                    let y = (0.5 - p.y / p.w * 0.5) * h;
                    r = [r[0].min(x), r[1].min(y), r[2].max(x), r[3].max(y)];
                }
            }
        }
        if !any || w < r[0] || r[2] < 0.0 || h < r[1] || r[3] < 0.0 {
            // `FUN_140102160`: off the screen, an unpinned host closes and the entity is
            // deselected.
            if let Some(screen) = self.screen.as_mut() {
                if self.entities.retire(ctx.ui, screen, entity) {
                    self.selection.selected = None;
                }
            }
            return;
        }
        let r = [
            r[0].clamp(0.0, w),
            r[1].clamp(0.0, h),
            r[2].clamp(0.0, w),
            r[3].clamp(0.0, h),
        ];
        self.entities.position(ctx.ui, entity, r);
    }

    /// The windows' title dragging (ToolUI `FUN_140081100`, `FUN_140080e80`, `FUN_140080e10`)
    /// and EntityUI's press (`FUN_140067430`). Returns whether the input was taken.
    pub(super) fn drag_window(&mut self, ctx: &mut Ctx, input: &RawInput) -> bool {
        match *input {
            // A title is not a control, so a press on it comes as a click on the map.
            RawInput::UiPress | RawInput::Click => {
                let on_map = matches!(input, RawInput::Click);
                let (x, y) = self.cursor;
                if let Some((template, root)) = self.utility.title_at(ctx.ui, x, y) {
                    // A utility window (ToolUI `+0x40` set at bind) follows at once.
                    let r = ctx.ui.node(root).rect;
                    self.window_drag = Some(WindowDrag {
                        root,
                        template: Some(template),
                        press: (x, y),
                        offset: (x - r[0], y - r[1]),
                        active: true,
                    });
                    return on_map;
                }
                let Some((i, title)) = self.entities.window_at(ctx.ui, x, y) else {
                    // Elsewhere on a utility window: the window keeps the click.
                    let inside = self.utility.controllers().iter().any(|(root, _)| {
                        let [x0, y0, x1, y1] = ctx.ui.node(*root).rect;
                        x0 <= x && x < x1 && y0 <= y && y < y1
                    });
                    return on_map && inside;
                };
                let w = &self.entities.open[i];
                let (root, entity, pinned) = (w.root, w.entity, w.pinned);
                if title {
                    let r = ctx.ui.node(root).rect;
                    self.window_drag = Some(WindowDrag {
                        root,
                        template: None,
                        press: (x, y),
                        offset: (x - r[0], y - r[1]),
                        active: pinned,
                    });
                }
                // `FUN_140067430`: a press on a window selects its entity again.
                if self.selection.selected != Some(entity) {
                    self.selection.selected = Some(entity);
                }
                on_map
            }
            RawInput::Cursor(x, y) => {
                let Some(d) = &mut self.window_drag else {
                    return false;
                };
                if !d.active {
                    if (x - d.press.0).abs() <= 5 && (y - d.press.1).abs() <= 5 {
                        return false;
                    }
                    d.active = true;
                    if let Some(w) = self.entities.open.iter_mut().find(|w| w.root == d.root) {
                        w.pinned = true;
                        // EntityUI's update: `checkPin` shows ToolUI `+0x40`.
                        if let Some(pin) = ctx.ui.find(w.root, "checkPin") {
                            ctx.ui.set_checked(pin, true);
                        }
                    }
                }
                let at = ((x - d.offset.0).max(0), (y - d.offset.1).max(0));
                let node = ctx.ui.node_mut(d.root);
                node.alignment = 0;
                node.margins = [at.0, 0, at.1, 0];
                match &d.template {
                    Some(t) => self.utility.remember(t, at),
                    None => {
                        if let Some(w) = self.entities.open.iter_mut().find(|w| w.root == d.root) {
                            w.at = Some(at);
                        }
                    }
                }
                self.cursor = (x, y);
                d.active
            }
            RawInput::Release => {
                self.window_drag = None;
                false
            }
            _ => false,
        }
    }

    /// `FUN_140106740`: the map cell under the cursor. The original casts the camera ray
    /// against the terrain (`FUN_1400afa00`); this marches the ray and refines the hit by
    /// bisection on the exact ground query.
    pub(super) fn cursor_cell(&self, screen: [i32; 2]) -> Option<(i32, i32)> {
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

    pub(super) fn map_click(&self, ctx: &mut Ctx) -> Option<glam::Vec2> {
        let size = self.town.as_ref()?.cells.size as u32;
        self.utility.map_click(ctx.ui, self.cursor, size)
    }

    /// A toolbar button's action, from a click or its key (`FUN_1400be770`).
    pub(super) fn run_action(&mut self, ctx: &mut Ctx, action: Action) {
        match action {
            // `FUN_1400be770` kinds 6-9 write the simulation controller's pause flag and speed.
            Action::Pause => {
                self.execute(ctx.archive, PlayerCommand::SetPaused(true));
            }
            Action::Play => {
                self.execute(ctx.archive, PlayerCommand::SetPaused(false));
            }
            Action::Slower => {
                let speed = self.speed.saturating_sub(1);
                self.execute(ctx.archive, PlayerCommand::SetSpeed(speed));
            }
            Action::Faster => {
                let speed = (self.speed + 1).min((SPEED_LEVELS.len() - 1) as u8);
                self.execute(ctx.archive, PlayerCommand::SetSpeed(speed));
            }
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
            // `FUN_140108030`: under a RestrictPlayer setup the tool is its template's.
            Action::Build(Some(template)) => {
                let template = match &self.restrict {
                    Some(r) if !r.place.is_empty() => {
                        format!("{}:resource", r.place.split(':').next().unwrap_or(""))
                    }
                    _ => template,
                };
                self.start_tool(ctx, &template)
            }
            Action::Build(None) => self.end_placement(ctx),
            Action::Options => self.open_pause_menu(ctx),
            other => log::info!("toolbar action not implemented yet: {other:?}"),
        }
    }
}

impl GameplayLoop {
    /// The UI events of `GameLoop::event`, before the queued commands are applied.
    pub(super) fn window_event(&mut self, ctx: &mut Ctx, event: Event) -> Next {
        if let (Some(d), Some(screen)) = (self.mods_dialog.as_mut(), self.screen.as_mut()) {
            if let Event::Selected(id, index) = event {
                d.selected(ctx.ui, &ctx.settings.persistence, ctx.mods, id, index);
            }
            if let Event::Clicked(id) | Event::Changed(id) = event {
                use crate::ui::mod_ui::Outcome;
                match d.click(ctx.ui, screen, ctx.mods, &ctx.settings.persistence, id) {
                    Outcome::Stay => {}
                    Outcome::Closed => self.mods_dialog = None,
                    Outcome::Reload => {
                        // `FUN_140072110` over a game: saved to memory, then loaded again.
                        self.mods_dialog = None;
                        self.store_window_settings(ctx.ui, true);
                        let saved = self.save_file(ctx.archive, ctx.ui, ctx.mods, None, None);
                        *ctx.reload_mods = Some(match saved {
                            Some(bytes) => crate::mods::Reload::Game(bytes),
                            None => crate::mods::Reload::Menu,
                        });
                    }
                }
            }
            return Next::Stay;
        }
        if let (Some(d), Some(screen)) = (self.save_dialog.as_mut(), self.screen.as_mut()) {
            if let Event::Clicked(id) = event {
                let outcome = d.click(ctx.ui, screen, ctx.audio, ctx.archive, ctx.mods, id);
                if let Some(next) = self.save_outcome(ctx, outcome) {
                    return next;
                }
            }
            return Next::Stay;
        }
        if let (Some(options), Some(screen)) = (&mut self.options, &mut self.screen) {
            if options.event(ctx, screen, event) == crate::ui::options::Outcome::Closed {
                self.options = None;
            }
            return Next::Stay;
        }
        // `FUN_1400e41b0`'s "pressed": a click, or a toggle (the toolbar's menu buttons are check
        // controls, which send Changed). Recording only clicks left a pointer press on Housing
        // unseen, so the first tutorial stopped at its first house (issue #11).
        if let (Event::Clicked(id) | Event::Changed(id), Some(t)) = (event, self.tutorial.as_mut())
        {
            t.pressed.push(id);
        }
        // TutorialUI's quit (`FUN_1400e8bb0`): back to the main menu.
        if let Event::Clicked(id) = event {
            if self.tutorial_quit(id) {
                return Self::return_to_menu();
            }
        }
        if self.utility.help_event(ctx.ui, event) {
            return Next::Stay;
        }
        // A combo in a building's window (event `0x14`).
        if let Event::Selected(id, index) = event {
            if self.entities.town_hall_select(ctx.ui, &self.statistics, id) {
                return Next::Stay;
            }
            let chosen = self
                .town
                .as_ref()
                .and_then(|town| self.entities.select_item(ctx.ui, town, id, index));
            if let (Some(WorkPlaceClick::Command(c)), Some(_)) = (chosen, &self.town) {
                self.execute(ctx.archive, PlayerCommand::Town(c));
            }
            return Next::Stay;
        }
        let (Event::Clicked(id) | Event::Changed(id)) = event else {
            return Next::Stay;
        };
        // The Achievement and Goal dialogs over the pause menu (OK closes them).
        if let Some(d) = &self.menu_dialog {
            let root = match d {
                MenuDialog::Achievements(a) => a.closes(id).then_some(a.root),
                MenuDialog::Goal(g) => g.closes(id).then_some(g.root),
                MenuDialog::Map(m) => match m.click(ctx.ui, id) {
                    crate::view::map_params::MapParamsClick::Rename(name) => {
                        let root = m.root;
                        self.execute(ctx.archive, PlayerCommand::RenameTown { name });
                        Some(root)
                    }
                    crate::view::map_params::MapParamsClick::NewName(max_length) => {
                        let random = PlayerCommand::RandomTownName { max_length };
                        if let Outcome::Name(name) = self.execute(ctx.archive, random) {
                            if let Some(MenuDialog::Map(m)) = &self.menu_dialog {
                                m.set_name(ctx.ui, &name);
                            }
                        }
                        None
                    }
                    crate::view::map_params::MapParamsClick::Close => Some(m.root),
                    crate::view::map_params::MapParamsClick::None => None,
                },
            };
            if let (Some(root), Some(screen)) = (root, self.screen.as_mut()) {
                screen.remove_root(ctx.ui, root);
                self.menu_dialog = None;
            }
            return Next::Stay;
        }
        if let Some(menu) = &self.pause_menu {
            // `FUN_1400b9020`.
            match menu.click(ctx.ui, id) {
                crate::ui::pause_menu::PauseClick::Resume => self.close_pause_menu(ctx),
                crate::ui::pause_menu::PauseClick::Menu => {
                    self.close_pause_menu(ctx);
                    return Self::return_to_menu();
                }
                crate::ui::pause_menu::PauseClick::Quit => return Next::Quit,
                crate::ui::pause_menu::PauseClick::Options => {
                    if let Some(screen) = &mut self.screen {
                        self.options = crate::ui::options::OptionsDialog::open(ctx, screen);
                    }
                }
                crate::ui::pause_menu::PauseClick::Achievement => {
                    if let Some(screen) = &mut self.screen {
                        let awarded = self.achievement_manager.awarded.clone();
                        if let Some(d) = crate::ui::achievement_ui::AchievementDialog::open(
                            ctx.ui,
                            screen,
                            &self.achievements,
                        ) {
                            d.refresh(ctx.ui, &awarded);
                            self.menu_dialog = Some(MenuDialog::Achievements(d));
                        }
                    }
                }
                crate::ui::pause_menu::PauseClick::Goal => {
                    if let Some(screen) = &mut self.screen {
                        let goal = self.goal.as_ref().map(|g| (&g.achievement, &g.instance));
                        self.menu_dialog =
                            crate::ui::achievement_ui::GoalDialog::open(ctx.ui, screen, goal)
                                .map(MenuDialog::Goal);
                    }
                }
                crate::ui::pause_menu::PauseClick::Map => {
                    if self.town_names.is_none() {
                        self.town_names = ctx
                            .archive
                            .read("Game/Names/Cities_resource.crs")
                            .and_then(|b| banished_formats::names::NameDatabase::read(&b))
                            .map_err(|e| log::error!("town names: {e}"))
                            .ok();
                    }
                    let flags = self.town.as_ref().map_or(0, |t| t.construction.game_flags);
                    if let Some(screen) = &mut self.screen {
                        self.menu_dialog = crate::view::map_params::MapParamsDialog::open(
                            ctx.ui,
                            screen,
                            &self.params,
                            flags,
                        )
                        .map(MenuDialog::Map);
                    }
                }
                crate::ui::pause_menu::PauseClick::Mod => {
                    if let Some(screen) = &mut self.screen {
                        self.mods_dialog = crate::ui::mod_ui::ModDialog::open(
                            ctx.ui,
                            screen,
                            ctx.mods,
                            true,
                            &ctx.settings.persistence,
                            ctx.pack_dir.map(std::path::Path::to_path_buf),
                        );
                    }
                }
                crate::ui::pause_menu::PauseClick::Load
                | crate::ui::pause_menu::PauseClick::Save => {
                    if let Some(screen) = &mut self.screen {
                        let loading = matches!(
                            menu.click(ctx.ui, id),
                            crate::ui::pause_menu::PauseClick::Load
                        );
                        self.save_dialog = crate::ui::save_dialog::SaveLoadDialog::open(
                            ctx.ui, screen, loading, true,
                        );
                    }
                }
                crate::ui::pause_menu::PauseClick::Other => {}
            }
            return Next::Stay;
        }
        // The Follow dialog's close button (`FUN_140069b90`, id 100).
        if self.follow.as_ref().is_some_and(|f| f.is_close(id)) {
            self.close_follow(ctx);
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
                    if self.town.is_some() {
                        let set = banished_game::work::professions::Command::SetCount {
                            profession,
                            count,
                        };
                        self.execute(ctx.archive, PlayerCommand::Town(set));
                    }
                    return Next::Stay;
                }
                Some(UtilityClick::Notify(i, on)) => {
                    self.events.notify[i] = on;
                    return Next::Stay;
                }
                Some(UtilityClick::StockLimit(slot, limit)) => {
                    self.stock_warnings.limits[slot] = limit;
                    return Next::Stay;
                }
                Some(UtilityClick::GoTo(cell)) => {
                    self.go_to(cell);
                    return Next::Stay;
                }
                Some(UtilityClick::ShowCitizen(citizen)) => {
                    // `FUN_140108510`: centre the camera on the citizen and select it.
                    if let (Some(town), Some(view), Some(screen)) =
                        (self.town.as_ref(), self.view.as_mut(), self.screen.as_mut())
                    {
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
                Some(UtilityClick::Handled) => return Next::Stay,
                None => {}
            }
            if self.entities.toggle_page(ctx.ui, id) {
                return Next::Stay;
            }
            let clicked = self
                .town
                .as_ref()
                .and_then(|town| self.entities.click(ctx.ui, town, id));
            match clicked {
                Some(WorkPlaceClick::Command(command)) => {
                    self.execute(ctx.archive, PlayerCommand::Town(command));
                    return Next::Stay;
                }
                // UpgradeUI (`FUN_14008c160`): the template is loaded for the rebuild.
                Some(WorkPlaceClick::Upgrade { entity, template }) => {
                    self.execute(ctx.archive, PlayerCommand::Upgrade { entity, template });
                    return Next::Stay;
                }
                // ResourceLimitUI (`FUN_14007ccb0`): the slot's limit.
                Some(WorkPlaceClick::StockLimit { slot, limit }) => {
                    self.stock_warnings.limits[slot] = limit;
                    return Next::Stay;
                }
                Some(WorkPlaceClick::ShowBuilding(building)) => {
                    self.entities.pin_containing(ctx.ui, id);
                    if let (Some(town), Some(view)) = (self.town.as_ref(), self.view.as_mut()) {
                        let entity = Entity::Placed(building);
                        if let SceneCamera::Map(camera) = &mut view.camera {
                            camera.target = entity.position(town).truncate();
                        }
                        self.selection.selected = Some(entity);
                        self.entities
                            .select(ctx.ui, screen, ctx.archive, town, entity);
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
            } else if matches!(
                ctx.ui.node(id).name.as_str(),
                "buttonProfession" | "buttonHome"
            ) {
                // CitizenUI event `0x12`: ids `0x227` (`FUN_140065330`: the school, else the
                // workplace) and `0x226` (`FUN_140065270`: the home) open that building through
                // `FUN_140108510`: the asking window pinned, the camera on it, selected.
                let profession = ctx.ui.node(id).name == "buttonProfession";
                let target = match (self.entities.entity_containing(ctx.ui, id), &self.town) {
                    (Some(Entity::Citizen(c)), Some(town)) => {
                        let s = &town.agents.states[c];
                        if profession {
                            s.school.or(s.workplace)
                        } else {
                            s.home
                        }
                    }
                    _ => None,
                };
                if let (Some(building), Some(town), Some(view)) =
                    (target, self.town.as_ref(), self.view.as_mut())
                {
                    self.entities.pin_containing(ctx.ui, id);
                    let entity = Entity::Placed(building);
                    if let SceneCamera::Map(camera) = &mut view.camera {
                        camera.target = entity.position(town).truncate();
                    }
                    self.selection.selected = Some(entity);
                    self.entities
                        .select(ctx.ui, screen, ctx.archive, town, entity);
                }
            } else if ctx.ui.node(id).name == "buttonFollow" {
                // CitizenUI event `0x12` id `0x228` (`FUN_140065080`).
                if let (Some(Entity::Citizen(c)), Some(owner), Some(town)) = (
                    self.entities.entity_containing(ctx.ui, id),
                    self.entities.root_containing(ctx.ui, id),
                    self.town.as_ref(),
                ) {
                    if let Some(old) = self.follow.take() {
                        old.close(ctx.ui, screen);
                    }
                    self.follow =
                        crate::tools::follow::Follow::open(ctx.ui, screen, town, c, owner);
                }
            } else if ctx.ui.node(id).name == "buttonCenter" {
                // EntityUI event `0x12` id `0x65` (`FUN_1400671d0`).
                if let (Some(entity), Some(town), Some(view)) = (
                    self.entities.entity_containing(ctx.ui, id),
                    self.town.as_ref(),
                    self.view.as_mut(),
                ) {
                    if let SceneCamera::Map(camera) = &mut view.camera {
                        camera.target = center_target(town, entity);
                    }
                }
            }
            return Next::Stay;
        };
        self.run_action(ctx, action);
        self.update_hud(ctx.ui);
        Next::Stay
    }
}
