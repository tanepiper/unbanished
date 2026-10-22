//! The map window: binding it, drawing the town on it, and a click on it.

use super::*;

/// `MapUI` (vtable `0x140162490`): OnOpened `FUN_14006d3e0` shows the minimap's two images
/// (`Game +0x148`: terrain render `+0x8`, entity overlay `+0x10`) in `imageMap` and
/// `imageOverlay`; the overlay takes clicks (id 1000, `FUN_14006d340` → `FUN_14006d1b0`).
/// `frustum` is a `FrustumOutlineElement` (`FUN_14006c7f0`). Read-only.
pub struct MapUi {
    overlay: Option<NodeId>,
    frustum: Option<NodeId>,
}

impl MapUi {
    pub(super) fn bind(ui: &mut Ui, root: NodeId, size: u32) -> MapUi {
        for (name, texture) in [
            ("imageMap", crate::loops::MINIMAP),
            ("imageOverlay", crate::loops::MINIMAP_OVERLAY),
        ] {
            if let Some(n) = ui.find(root, name) {
                let shown = match ui.node(n).kind {
                    Kind::Image { size, .. } => size,
                    _ => [0, 0],
                };
                let node = ui.node_mut(n);
                node.kind = Kind::Picture {
                    texture,
                    size: [size as i32; 2],
                };
                node.min_size = [
                    node.min_size[0].max(shown[0]),
                    node.min_size[1].max(shown[1]),
                ];
            }
        }
        MapUi {
            overlay: ui.find(root, "imageOverlay"),
            frustum: ui.find(root, "frustum"),
        }
    }

    /// `FUN_14006c7f0`: the camera's far corners on the ground, in element pixels
    /// (`x·W/n + W/2`, `(W - 1) - (y·W/n + H/2)`), joined 0-1, 1-3, 3-2, 2-0.
    pub(super) fn update(&self, ui: &mut Ui, v: &StatusValues) {
        let (Some(f), Some(corners)) = (self.frustum, v.frustum) else {
            return;
        };
        let r = ui.node(f).rect;
        let (w, h) = ((r[2] - r[0]) as f32, (r[3] - r[1]) as f32);
        let scale = w / v.map_size.max(1) as f32;
        let p: Vec<[i32; 2]> = corners
            .iter()
            .map(|c| {
                let x = c[0] * scale + w * 0.5;
                let y = (w - 1.0) - (c[1] * scale + h * 0.5);
                [x as i32, y as i32]
            })
            .collect();
        if let Kind::Lines { lines, .. } = &mut ui.node_mut(f).kind {
            *lines = [(0, 1), (1, 3), (3, 2), (2, 0)]
                .iter()
                .map(|&(a, b)| [p[a][0], p[a][1], p[b][0], p[b][1]])
                .collect();
        }
    }

    /// `FUN_14006d1b0`: a click at element pixel (x, y) of a W×H overlay targets
    /// `((x - W/2)·n/W, (H/2 - y)·n/W)`.
    pub(super) fn click(&self, ui: &Ui, at: (i32, i32), size: u32) -> Option<glam::Vec2> {
        let r = ui.node(self.overlay?).rect;
        let (x, y, w, h) = (at.0 - r[0], at.1 - r[1], r[2] - r[0], r[3] - r[1]);
        if x < 0 || x >= w || y < 0 || y >= h {
            return None;
        }
        let scale = size as f32 / w as f32;
        Some(glam::Vec2::new(
            (x - (w >> 1)) as f32 * scale,
            ((h >> 1) - y) as f32 * scale,
        ))
    }
}
