//! The tutorial's highlight: which element or cells a step points at, placing the highlight on screen and focusing the camera.

use super::*;

impl GameplayLoop {
    /// `FUN_1400e7bd0`: the highlight dialog points at the element (`+0x40`), or at the
    /// window of the component (`+0x58`, the `+0x70`th of its kind).
    pub(super) fn update_highlight(&mut self, ctx: &mut Ctx, s: &Setup, update: bool) {
        let Setup::Dialog {
            point,
            highlight,
            angle,
            component,
            index,
            ..
        } = s
        else {
            return;
        };
        let Some(dialog) = highlight else {
            return;
        };
        let target = if !point.is_empty() {
            self.tutorial_element(ctx.ui, point).map(Target::Element)
        } else if !component.is_empty() {
            // The `index`th entity of the template (stock manager `+0x160`).
            self.sim.town.as_ref().and_then(|town| {
                (0..town.placed.len())
                    .filter(|&e| {
                        town.placed[e].alive
                            && file(&town.templates[town.placed[e].template].reference)
                                == file(component)
                    })
                    .nth((*index).max(0) as usize)
                    .map(Target::Entity)
            })
        } else {
            None
        };
        let current = self.tutorial.as_ref().and_then(|t| t.highlight);
        match (target, current) {
            (Some(target), Some((root, arrow, old, _, time))) if update => {
                if old != target {
                    if let Some(t) = &mut self.tutorial {
                        t.highlight = Some((root, arrow, target, *angle, time));
                    }
                }
            }
            (Some(target), _) => {
                self.close_highlight(ctx);
                let Some(screen) = self.screen.as_mut() else {
                    return;
                };
                let reference = dialog.replace('\\', "/");
                let Some(i) = screen.add_dialog(ctx.ui, &reference) else {
                    return;
                };
                let root = screen.dialogs[i].root;
                // `FUN_1400e54f0`: the glow hidden, the arrow kept.
                if let Some(g) = ctx.ui.find(root, "glowGroup") {
                    ctx.ui.set_visible(g, false);
                }
                let arrow = ctx.ui.find(root, "arrow");
                if let Some(t) = &mut self.tutorial {
                    t.highlight = Some((root, arrow, target, *angle, 0.0));
                }
            }
            (None, _) if !update => self.close_highlight(ctx),
            _ => {}
        }
    }

    pub(super) fn close_highlight(&mut self, ctx: &mut Ctx) {
        let Some((root, ..)) = self.tutorial.as_mut().and_then(|t| t.highlight.take()) else {
            return;
        };
        if let Some(screen) = self.screen.as_mut() {
            screen.remove_root(ctx.ui, root);
        }
    }

    /// `FUN_1400e54b0`: an element target (`FUN_1400e4f90`) gets the arrow at the corner its
    /// angle names, an entity (`FUN_1400e5270`) the arrow over its projected point; it bobs
    /// `sin(t·π)·8` pixels and is turned by `-angle` about its centre.
    pub(super) fn place_highlight(&mut self, ui: &mut Ui, dt: f32) {
        let Some((root, arrow, target, angle, time)) = self
            .tutorial
            .as_mut()
            .and_then(|t| t.highlight.as_mut())
            .map(|h| {
                h.4 += dt;
                *h
            })
        else {
            return;
        };
        let Some(arrow) = arrow else { return };
        let size = match &ui.node(arrow).kind {
            Kind::Image {
                sprite: Some(s), ..
            } => [s.width, s.height],
            _ => [0, 0],
        };
        let bob = (time * std::f32::consts::PI).sin() * 8.0;
        let (x, y) = match target {
            // `FUN_1400e4f90`: the arrow's box (the sprite's width square) against a corner of
            // the element's rectangle, the sprite's point (down and right at 0°) on it: 0°
            // above and left of the top left, 90° below and left of the bottom left, 180°
            // below and right of the bottom right, 270° above and right of the top right. It
            // bobs along the same diagonal.
            Target::Element(n) => {
                let [l, t, r, b] = ui.node(n).rect;
                let w = size[0];
                let (x, y, sx, sy) = match angle {
                    90 => (l - w, b, 1.0, -1.0),
                    180 => (r, b, -1.0, -1.0),
                    270 => (r, t - w, -1.0, 1.0),
                    _ => (l - w, t - w, 1.0, 1.0),
                };
                (sx * bob + x as f32, sy * bob + y as f32)
            }
            // `FUN_1400e5270`: centred over the point, its foot there, bobbing up and down.
            Target::Entity(e) => {
                let Some((cx, cy)) = self.screen_point(ui.screen, e) else {
                    ui.set_visible(arrow, false);
                    return;
                };
                (
                    (cx as i32 - (size[0] >> 1)) as f32,
                    (cy as i32 - size[1]) as f32 + bob,
                )
            }
        };
        ui.set_visible(arrow, true);
        let node = ui.node_mut(root);
        node.alignment = 0;
        node.margins = [(x as i32).max(0), 0, (y as i32).max(0), 0];
        ui.node_mut(arrow).rotation = -(angle as f32);
    }

    /// An entity's position on the screen (`FUN_140105850`).
    pub(super) fn screen_point(&self, screen: [i32; 2], e: usize) -> Option<(f32, f32)> {
        let (town, view) = (self.sim.town.as_ref()?, self.view.as_ref()?);
        if screen[0] <= 0 || screen[1] <= 0 {
            return None;
        }
        let pos = crate::tools::selection::Entity::Placed(e).position(town);
        let vp = view.view_proj(screen[0] as f32 / screen[1] as f32);
        let p = vp * pos.extend(1.0);
        if p.w <= 0.0 {
            return None;
        }
        Some((
            (p.x / p.w * 0.5 + 0.5) * screen[0] as f32,
            (0.5 - p.y / p.w * 0.5) * screen[1] as f32,
        ))
    }

    /// The camera to the middle of the cells.
    pub(super) fn focus_cells(&mut self, x0: i32, y0: i32, x1: i32, y1: i32) {
        let Some(town) = &self.sim.town else { return };
        if x0 < 0 || y0 < 0 {
            return;
        }
        let half = (town.cells.size as i32 >> 1) as f32;
        let x = (x0 + x1) as f32 * 0.5 - half + 0.5;
        let y = (y0 + y1) as f32 * 0.5 - half + 0.5;
        if let Some(super::super::SceneView {
            camera: super::super::SceneCamera::Map(camera),
            ..
        }) = &mut self.view
        {
            camera.target = glam::Vec2::new(x, y);
        }
    }
}
