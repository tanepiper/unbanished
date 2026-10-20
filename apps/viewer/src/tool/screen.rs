//! The viewer's screen: its layout, the preview's viewport, and the frame handed to the renderer.

use super::*;

impl Tool {
    pub fn layout(&mut self, size: (u32, u32)) {
        self.ui.screen = [size.0 as i32, size.1 as i32];
        // Dialog roots have their own placement pass. Laying them out as ordinary children
        // first can change the constraints of their containers before their first frame.
        let dialogs = std::mem::take(&mut self.ui.node_mut(self.dialog_host).children);
        self.ui
            .layout(self.root, [0, 0, size.0 as i32, size.1 as i32]);
        if self.mode == Mode::Dialogs {
            // Centre previewed dialogs in the main area (DialogInstance::ComputeLayout).
            let r = self.ui.node(self.main).rect;
            for &c in &dialogs {
                self.ui
                    .layout_dialog(c, [r[0] + 12, r[1] + 48, r[2] - 12, r[3] - 12]);
                self.suppress_overlapping_text(c);
            }
        }
        self.ui.node_mut(self.dialog_host).children = dialogs;
        if self.mode == Mode::Audio {
            let area = self.ui.node(self.main).rect;
            layout_floating_window(
                &mut self.ui,
                self.audio_player.window,
                area,
                [520, 220],
                &mut self.audio_window_position,
            );
        } else if self.mode == Mode::Models && self.ui.node(self.model_player.window).visible() {
            let area = self.ui.node(self.main).rect;
            if self.model_window_position.is_none() {
                self.model_window_position = Some([area[0] + 12, area[1] + 12]);
            }
            layout_floating_window(
                &mut self.ui,
                self.model_player.window,
                area,
                [540, 206],
                &mut self.model_window_position,
            );
        } else if self.mode == Mode::Dialogs && self.ui.node(self.dialog_player.window).visible() {
            let area = self.ui.node(self.main).rect;
            if self.dialog_window_position.is_none() {
                self.dialog_window_position = Some([area[0] + 12, area[1] + 12]);
            }
            layout_floating_window(
                &mut self.ui,
                self.dialog_player.window,
                area,
                [440, 184],
                &mut self.dialog_window_position,
            );
        }
    }

    pub(super) fn suppress_overlapping_text(&mut self, root: NodeId) {
        let mut pending = vec![(root, root)];
        let mut texts = Vec::new();
        while let Some((id, parent)) = pending.pop() {
            let node = self.ui.node(id);
            if !node.visible() {
                continue;
            }
            pending.extend(self.ui.children(id).into_iter().map(|child| (child, id)));
            {
                let rect = match &node.kind {
                    Kind::Text(text) if !text.text.is_empty() => {
                        let [l, t, r, _] = node.rect;
                        let x = match text.justify {
                            1 => l + ((r - l) - text.layout.width) / 2,
                            2 => r - text.layout.width,
                            _ => l,
                        };
                        Some([x, t, x + text.layout.width, t + text.layout.height])
                    }
                    Kind::Label {
                        text,
                        icon,
                        icon_size,
                        text_alignment,
                        orientation,
                        gap,
                        ..
                    } if !text.text.is_empty() => {
                        let [l, t, r, b] = node.rect;
                        let iw = if icon_size[0] > 0 {
                            icon_size[0]
                        } else {
                            icon.map_or(0, |s| s.width)
                        };
                        let ih = if icon_size[1] > 0 {
                            icon_size[1]
                        } else {
                            icon.map_or(0, |s| s.height)
                        };
                        let [al, at, ar, ab] = match orientation {
                            0 => [l, t, r - gap - iw, b],
                            1 => [l + iw + gap, t, r, b],
                            3 => [l, t + ih + gap, r, b],
                            _ => [l, t, r, b - ih - gap],
                        };
                        let w = text.layout.width;
                        let h = text.layout.height;
                        let x = match text_alignment % 3 {
                            1 => al + ((ar - al) - w) / 2,
                            2 => ar - w,
                            _ => al,
                        };
                        let y = match text_alignment / 3 {
                            1 => at + ((ab - at) - h) / 2,
                            2 => ab - h,
                            _ => at,
                        };
                        Some([x, y, x + w, y + h])
                    }
                    _ => None,
                };
                if let Some(rect) = rect {
                    texts.push((id, parent, rect));
                }
            }
        }
        texts.sort_by_key(|(id, _, _)| *id);
        let mut kept: Vec<(NodeId, [i32; 4])> = Vec::new();
        for (id, parent, rect) in texts {
            let area = ((rect[2] - rect[0]).max(0) as i64) * ((rect[3] - rect[1]).max(0) as i64);
            let covered = kept.iter().any(|(kept_parent, other)| {
                if *kept_parent != parent {
                    return false;
                }
                let w = (rect[2].min(other[2]) - rect[0].max(other[0])).max(0) as i64;
                let h = (rect[3].min(other[3]) - rect[1].max(other[1])).max(0) as i64;
                let other_area =
                    ((other[2] - other[0]).max(0) as i64) * ((other[3] - other[1]).max(0) as i64);
                area.min(other_area) > 0 && w * h * 10 >= area.min(other_area) * 9
            });
            if covered {
                self.ui.set_visible(id, false);
            } else {
                kept.push((parent, rect));
            }
        }
    }

    pub fn viewport(&self) -> [f32; 4] {
        let r = self.ui.node(self.main).rect;
        [
            r[0] as f32,
            r[1] as f32,
            (r[2] - r[0]) as f32,
            (r[3] - r[1]) as f32,
        ]
    }

    pub fn frame<'a>(&'a self, quads: &'a [unbanished_ui::Quad]) -> Frame<'a> {
        let vp = self.viewport();
        Frame {
            clear: if self.mode == Mode::Models {
                CLEAR_SKY
            } else {
                CLEAR_DARK
            },
            mesh: if self.mode == Mode::Models {
                self.mesh.as_ref()
            } else {
                None
            },
            view_proj: self.camera.view_proj(vp[2] / vp[3].max(1.0)),
            eye: self.camera.eye(),
            viewport: Some(vp),
            ui: Some((&self.ui, quads)),
            render_mode: self.render_mode,
        }
    }

    pub fn quads(&self) -> Vec<unbanished_ui::Quad> {
        self.ui.draw(self.root)
    }
}
