//! The active tool: placement, following a citizen, a scenario's restricted tools, and removal.

use super::*;

impl GameplayLoop {
    /// Update the placement ghost after the cursor, rotation or variant changed.
    pub(super) fn refresh_placement(&mut self, ctx: &mut Ctx) {
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
                    crate::audio::sound::global::PLACEMENT_MOVE,
                    crate::audio::sound::sound_type::UI,
                );
                self.move_cooldown = 0.05;
            }
        }
        // `FUN_140103190`: the footprint's cells under the cursor, then (game UI `+0x80`) the
        // tutorial's target in yellow, with the building's footprint and its rotation.
        let degrees = self.rotation * banished_game::world::placement::ROTATION_STEP as i32;
        let mut cells = tool.rect().map_or_else(Vec::new, |rect| {
            tool.overlay(town, &grid, rect, degrees, false)
        });
        if let Some(r) = self.restrict.as_ref().filter(|r| !r.place.is_empty()) {
            let rect = banished_game::world::placement::Rect {
                x0: r.rect[0],
                y0: r.rect[1],
                x1: r.rect[2],
                y1: r.rect[3],
            };
            cells.extend(tool.overlay(town, &grid, rect, r.rotation, true));
        }
        let n = town.cells.size as i32;
        view.cell_overlay = crate::gfx::selection_overlay::vertices(&cells, n, |x, y| {
            let mut normal = [0.0; 3];
            grid.ground(x, y, &mut normal)
        });
        if view.overlay_texture.is_none() && !cells.is_empty() {
            view.overlay_texture = crate::gfx::selection_overlay::load_texture(ctx.archive);
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
        let allowed = Self::restrict_allows(self.restrict.as_ref(), tool, self.rotation);
        if let Some(bar) = &mut self.create_bar {
            bar.update(ctx.ui, tool.size(), tool.valid() && allowed, rect);
        }
    }

    /// `FUN_140069860`.
    pub(super) fn close_follow(&mut self, ctx: &mut Ctx) {
        if let (Some(f), Some(screen)) = (self.follow.take(), self.screen.as_mut()) {
            f.close(ctx.ui, screen);
        }
    }

    /// The Follow dialog each frame (`FUN_140069bb0`, `FUN_1400698d0`): the camera on the
    /// citizen and its texts; it closes with its citizen window (`FUN_140065a90`) or citizen.
    pub(super) fn update_follow(&mut self, ctx: &mut Ctx) {
        let Some(f) = &self.follow else { return };
        let target = self.town.as_ref().and_then(|t| f.target(t));
        let (Some(target), true) = (target, self.entities.is_open(f.owner)) else {
            self.close_follow(ctx);
            return;
        };
        if let (Some(town), Some(view)) = (self.town.as_ref(), self.view.as_mut()) {
            if let SceneCamera::Map(camera) = &mut view.camera {
                camera.target = target;
            }
            f.layout(ctx.ui, town);
        }
    }

    /// `FUN_140102b20` / `FUN_1401033b0`: a RestrictPlayer setup allows the placement only
    /// on its cells and turned its way.
    pub(super) fn restrict_allows(
        restrict: Option<&tutorial::Restrict>,
        tool: &PlacementTool,
        rotation: i32,
    ) -> bool {
        let Some(r) = restrict else { return true };
        let Some(rect) = tool.rect() else {
            return false;
        };
        rect.x0 == r.rect[0]
            && rect.y0 == r.rect[1]
            && rect.x1 == r.rect[2]
            && rect.y1 == r.rect[3]
            && rotation * 90 == r.rotation
    }

    /// `FUN_140107b70`: with a template to place, the player's tool is its.
    pub(super) fn force_restricted_tool(&mut self, ctx: &mut Ctx) {
        let Some(place) = self
            .restrict
            .as_ref()
            .filter(|r| !r.place.is_empty())
            .map(|r| r.place.split(':').next().unwrap_or("").to_string())
        else {
            return;
        };
        let current = self
            .placement
            .as_ref()
            .map(|t| t.reference.clone())
            .or_else(|| self.removal.as_ref().map(|t| t.reference.clone()));
        let same = current.as_deref().is_some_and(|c| {
            c.split(':')
                .next()
                .unwrap_or("")
                .eq_ignore_ascii_case(&place)
        });
        if !same {
            self.start_tool(ctx, &format!("{place}:resource"));
        }
    }

    /// Toolbar kind 1 → `FUN_140108030`: start the tool for `template`.
    pub(super) fn start_tool(&mut self, ctx: &mut Ctx, template: &str) {
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
        // `FUN_1400e2720`: the Path tool's highlight goes with it.
        if self.path_tool.take().is_some_and(|t| t.entity.is_some()) {
            self.selection.selected = None;
        }
        if self.placement.is_none() && self.removal.is_none() {
            self.path_tool = crate::tools::path_tool::PathTool::start(ctx.archive, template);
        }
        if self.placement.is_none() && self.removal.is_none() && self.path_tool.is_none() {
            log::info!("{template}: no tool for it");
        }
        self.refresh_placement(ctx);
    }

    /// The removal tool's input (DragTool `FUN_140101b80`) with the cell under the cursor
    /// (`FUN_140106740`, −1 off the map).
    pub(super) fn removal_input(&mut self, ctx: &mut Ctx, input: DragInput) {
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
        tool.rotation = self.rotation;
        tool.restrict = self.restrict.as_ref().map(|r| r.rect);
        tool.manhattan = !self.held[keys::PATH_METHOD];
        let sound = match input {
            DragInput::Press => tool.press(ctx.archive, town, &grid, cell),
            DragInput::Move => tool.motion(ctx.archive, town, &grid, cell),
            DragInput::Release => tool.release(ctx.archive, town, &grid, cell),
        };
        // The original's tools act inside the input handler: RoadTool's apply
        // (`FUN_14010a200`) plans each cell (`FUN_1400a1e30`) and rebuilds the road decals
        // (`FUN_1400b4840`) there and then, paused or not. The port queues the commands;
        // applying them now is the order the next iteration would apply them in.
        if sound.is_some() {
            town.apply_commands(&grid, &mut self.rng);
        }
        let cells = tool.overlay(town, &grid, cell);
        view.cell_overlay = crate::gfx::selection_overlay::vertices(&cells, n, |x, y| {
            let mut normal = [0.0; 3];
            grid.ground(x, y, &mut normal)
        });
        if view.overlay_texture.is_none() && !cells.is_empty() {
            view.overlay_texture = crate::gfx::selection_overlay::load_texture(ctx.archive);
        }
        // DragTool `+0x30`: the CreateBar is open while dragging (`FUN_140104eb0`), shows the
        // drag's size and the toolbar message 0 (`FUN_140105130`) and closes with the drag.
        let screen_size = ctx.ui.screen;
        match (tool.drag_ends(), tool.drag_info(town, &grid)) {
            (Some((a, b)), Some(info)) => {
                if self.create_bar.is_none() {
                    if let (Some(t), Some(screen)) = (tool.toolbar.as_ref(), self.screen.as_mut()) {
                        // The bar lists the tool template's build requirements.
                        let file = tool.reference.split(':').next().unwrap_or("");
                        let build = ctx
                            .archive
                            .read_ref(&format!("{}:build", file.replace('\\', "/")))
                            .ok()
                            .and_then(|d| banished_formats::game::build_description(&d).ok());
                        self.create_bar =
                            CreateBar::open(ctx.ui, screen, ctx.archive, t, build.as_ref());
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
                    bar.update_drag(ctx.ui, info, visible.then_some(r));
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
                Some(crate::audio::sound::global::PLACEMENT_MOVE)
            }
            Some(DragSound::Thunk) => Some(crate::audio::sound::global::PLACEMENT_THUNK),
            Some(DragSound::Error) => Some(crate::audio::sound::global::PLACEMENT_ERROR),
            None => None,
        };
        if let Some(sound) = sound {
            ctx.audio
                .play(ctx.archive, sound, crate::audio::sound::sound_type::UI);
        }
    }

    pub(super) fn end_placement(&mut self, ctx: &mut Ctx) {
        self.placement = None;
        self.removal = None;
        if self.path_tool.take().is_some_and(|t| t.entity.is_some()) {
            self.selection.selected = None;
        }
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
}
