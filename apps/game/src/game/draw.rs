//! Drawing a frame: the scene, the interface and the cursor.

use super::*;

/// UI units to window pixels (`UserInterface::UIScaled` at draw time).
pub(crate) fn scale_quads(quads: &mut [unbanished_ui::Quad], s: f32) {
    if s == 1.0 {
        return;
    }
    for q in quads {
        // Whole device pixels: the corner and the size are rounded, so a bitmap glyph or image
        // starts on a pixel and equal ones get equal widths. Unrounded, a fractional scale
        // (a pixel ratio of 2.625 on some tablets) put most glyphs between pixels, and the
        // linear filtering smeared them (issue #12).
        let [x0, y0, x1, y1] = q.rect;
        let (left, top) = ((x0 * s).round(), (y0 * s).round());
        q.rect = [
            left,
            top,
            left + ((x1 - x0) * s).round(),
            top + ((y1 - y0) * s).round(),
        ];
        if let Some(c) = &mut q.clip {
            *c = c.map(|v| (v as f32 * s).round() as i32);
        }
    }
}

impl Game {
    /// "Use Software Mouse" (`Input::Configuration +10`): the pointer is drawn by the game, at
    /// its image's own size, instead of by the system (`UserInterface::SetCursor` hands the
    /// cursor's sheet image and hotspot to the device).
    pub(crate) fn cursor_quad(&mut self) -> Option<unbanished_ui::Quad> {
        let (x, y) = self.pointer?;
        if self.cursors.is_empty() {
            self.cursors = CURSORS
                .iter()
                .map(|name| {
                    let data = self
                        .archive
                        .read_ref(&format!("Cursor/{name}.rsc:cursor"))
                        .ok()?;
                    let c = unbanished_formats::ui::cursor(&data).ok()?;
                    let sheet_ref = c.sprite_sheet?.replace('\\', "/");
                    let sheet = self.ui.res.sheet(&sheet_ref)?;
                    let loaded = &self.ui.res.sheets[sheet].sheet;
                    let sprite = unbanished_ui::SpriteRef {
                        sheet,
                        width: loaded.width,
                        height: loaded.height,
                        uv: loaded
                            .sprites
                            .first()
                            .map_or([0.0, 0.0, 1.0, 1.0], |s| s.uv),
                    };
                    Some((sprite, c.hotspot))
                })
                .collect();
        }
        // The loop's cursor this frame (`UserInterface::SetCursor`), the arrow if it is missing.
        let index = self.main.current().cursor(&self.ui, self.ui.pointer());
        let cursor = self.cursors.get(index).copied().flatten();
        let cursor = cursor.or_else(|| self.cursors.first().copied().flatten());
        let (sprite, [hx, hy]) = cursor?;
        let (x, y) = ((x - hx) as f32, (y - hy) as f32);
        Some(unbanished_ui::Quad {
            texture: unbanished_ui::Texture::Sheet(sprite.sheet),
            rect: [x, y, x + sprite.width as f32, y + sprite.height as f32],
            uv: sprite.uv,
            color: [255; 4],
            clip: None,
            rotation: 0.0,
        })
    }

    pub fn draw(&mut self, renderer: &mut Renderer, capture: bool) -> Option<Vec<u8>> {
        if std::mem::take(&mut self.ui_reset) {
            renderer.reset_ui_sheets();
        }
        let size = renderer.size();
        let logical = self.logical(size);
        if let Some(screen) = self.main.current().screen() {
            screen.layout(&mut self.ui, logical);
        }
        let mut quads = self
            .main
            .current()
            .screen()
            .map(|s| self.ui.draw(s.root))
            .unwrap_or_default();
        scale_quads(&mut quads, self.ui_scale());
        quads.extend(loops::fade_quad(self.main.fade.level, size));
        if self.settings.input.mouse_display {
            quads.extend(self.cursor_quad());
        }
        renderer.set_ui_time(self.main.elapsed);
        let g = &self.settings.game;
        renderer.icon_options = gfx::status_icons::IconOptions {
            opacity: g.status_icon_opacity,
            enabled: g.status_icon_enable,
            scale: g.status_icon_scale,
        };
        let (terrain, clear) = match self.main.current().view() {
            View::World(view) => {
                renderer.set_scene(view);
                renderer.update_minimap(view);
                let fog = view.lighting.fog_color;
                let aspect = renderer.aspect();
                (
                    Some(match self.cinematic {
                        Some((eye, target, fov)) => {
                            view.frame_from(eye, target, fov, aspect, self.main.elapsed)
                        }
                        None => view.frame(aspect, self.main.elapsed),
                    }),
                    // Until the sky mesh is drawn, clear to the fog colour.
                    wgpu::Color {
                        r: fog.x as f64,
                        g: fog.y as f64,
                        b: fog.z as f64,
                        a: 1.0,
                    },
                )
            }
            View::None => (None, CLEAR),
        };
        let mut frame = Frame {
            terrain,
            clear,
            ui: None,
        };
        // A save's preview: the scene alone, cropped and scaled, read back over the next frames
        // with the minimap (`SaveLoad::Draw`).
        if frame.terrain.is_some()
            && self.save_capture.is_none()
            && self.main.current().wants_save_preview()
        {
            self.save_capture = Some(renderer.begin_capture(&frame));
        }
        if let Some(pending) = &self.save_capture {
            if let Some((rgba, minimap)) = renderer.poll_capture(pending) {
                self.save_capture = None;
                let (w, h) = renderer.size();
                let image = if rgba.len() == (w * h * 4) as usize {
                    crate::saves::saved::preview(&rgba, w, h)
                } else {
                    let (pw, ph) = crate::saves::saved::PREVIEW_SIZE;
                    vec![0; (pw * ph * 4) as usize]
                };
                self.main.current_mut().save_preview(image, minimap);
            }
        }
        // `FUN_14009b4e0`: the screenshot without the UI after the scene, with it after the UI.
        let shot = if self.screenshot.is_none() && frame.terrain.is_some() {
            self.main.current_mut().take_screenshot()
        } else {
            None
        };
        if shot == Some(false) {
            self.screenshot = Some(renderer.begin_capture(&frame));
        }
        frame.ui = Some((&self.ui, &quads));
        if shot == Some(true) {
            self.screenshot = Some(renderer.begin_capture(&frame));
        }
        if let Some(pending) = &self.screenshot {
            if let Some((rgba, _)) = renderer.poll_capture(pending) {
                self.screenshot = None;
                let (w, h) = renderer.size();
                if rgba.len() == (w * h * 4) as usize {
                    crate::gfx::screenshot::save(&rgba, w, h);
                }
            }
        }
        if capture {
            #[cfg(not(target_arch = "wasm32"))]
            return Some(renderer.capture(&frame));
        }
        renderer.render(&frame);
        None
    }
}
