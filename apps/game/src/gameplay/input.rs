//! Player input on the map and the windows: picking under the cursor, selection clicks, window dragging, map clicks, key actions and UI events.

mod window_event;

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
        let Some(town) = self.sim.town.as_ref() else {
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
        let town = self.sim.town.as_ref()?;
        let heights = self.sim.heights.as_ref().unwrap();
        let grid = heights.grid();
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
        let (Some(entity), true, true) =
            (picked, self.path_tool.is_some(), self.sim.town.is_some())
        else {
            return;
        };
        self.execute(ctx.archive, PlayerCommand::PathQuery { entity });
        let (Some(tool), Some(town)) = (self.path_tool.as_mut(), self.sim.town.as_mut()) else {
            return;
        };
        self.selection.selected = Some(entity);
        let n = town.cells.size as i32;
        let (lines, cells) = tool.overlay(n);
        if let Some(view) = self.view.as_mut() {
            let grid = self
                .sim
                .heights
                .as_ref()
                .expect("the heights come with the view")
                .grid();
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
            self.sim.town.as_ref(),
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
        let (view, town) = (self.view.as_ref()?, self.sim.town.as_ref()?);
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
        let grid = self
            .sim
            .heights
            .as_ref()
            .expect("the heights come with the view")
            .grid();
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
        let size = self.sim.town.as_ref()?.cells.size as u32;
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
                let map_size = self.sim.town.as_ref().map_or(0, |t| t.cells.size as u32);
                let Some(screen) = self.screen.as_mut() else {
                    return;
                };
                self.utility.toggle(
                    ctx.ui,
                    screen,
                    ctx.archive,
                    &template,
                    Some(&self.sim.events),
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
