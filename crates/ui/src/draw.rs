//! Drawing: the tree as a list of sprite quads, clipped and coloured.

use super::*;

pub(super) fn argb_to_rgba(c: u32, alpha: u8) -> [u8; 4] {
    let a = ((c >> 24) & 0xFF) * alpha as u32 / 255;
    [(c >> 16) as u8, (c >> 8) as u8, c as u8, a as u8]
}

pub(super) fn modulate(a: u32, b: u32) -> u32 {
    let ch = |s: u32| ((a >> s & 0xFF) * (b >> s & 0xFF) / 255) << s;
    ch(24) | ch(16) | ch(8) | ch(0)
}

impl Ui {
    /// The quads that draw `root` and its children, in drawing order, then the open combo lists
    /// and the tooltip.
    pub fn draw(&self, root: NodeId) -> Vec<Quad> {
        let mut out = Vec::new();
        self.draw_node(root, 0xFFFF_FFFF, None, &mut out);
        self.draw_popups(root, &mut out);
        self.draw_tooltip(root, &mut out);
        out
    }

    /// `SpriteContext::AddLine`: a one-pixel line from `(x0, y0)` towards `(x1, y1)`, its
    /// last pixel left off as a Direct3D line list draws it.
    pub(crate) fn line_quads(
        out: &mut Vec<Quad>,
        s: &SpriteRef,
        [x0, y0, x1, y1]: [i32; 4],
        color: u32,
        clip: Option<[i32; 4]>,
    ) {
        if y0 == y1 || x0 == x1 {
            let rect = [x0.min(x1), y0.min(y1), x0.max(x1), y0.max(y1)];
            let rect = if y0 == y1 {
                [rect[0], rect[1], rect[2], rect[1] + 1]
            } else {
                [rect[0], rect[1], rect[0] + 1, rect[3]]
            };
            Self::sprite_quad(out, s, rect, color, clip);
            return;
        }
        let steps = (x1 - x0).abs().max((y1 - y0).abs());
        for k in 0..steps {
            let x = x0 + (x1 - x0) * k / steps;
            let y = y0 + (y1 - y0) * k / steps;
            Self::sprite_quad(out, s, [x, y, x + 1, y + 1], color, clip);
        }
    }

    pub(super) fn sprite_quad(
        out: &mut Vec<Quad>,
        s: &SpriteRef,
        rect: [i32; 4],
        color: u32,
        clip: Option<[i32; 4]>,
    ) {
        if rect[2] <= rect[0] || rect[3] <= rect[1] {
            return;
        }
        out.push(Quad {
            texture: Texture::Sheet(s.sheet),
            rect: rect.map(|v| v as f32),
            uv: s.uv,
            color: argb_to_rgba(color, 255),
            clip,
            rotation: 0.0,
        });
    }

    pub(super) fn draw_text(
        &self,
        t: &TextData,
        origin: [i32; 2],
        color: u32,
        clip: Option<[i32; 4]>,
        out: &mut Vec<Quad>,
    ) {
        let Some(font) = t.font else { return };
        let Some(sheet) = self.res.fonts[font].sheet else {
            return;
        };
        let sprites = &self.res.sheets[sheet].sheet.sprites;
        if t.shadow {
            let mut shadow = t.clone();
            shadow.shadow = false;
            self.draw_text(
                &shadow,
                [origin[0] + 1, origin[1] + 1],
                color & 0xFF000000,
                clip,
                out,
            );
        }
        for g in &t.layout.glyphs {
            let Some(sp) = sprites.get(g.glyph) else {
                continue;
            };
            let (x, y) = (origin[0] + g.x, origin[1] + g.y);
            out.push(Quad {
                texture: Texture::Sheet(sheet),
                rect: [
                    x as f32,
                    y as f32,
                    (x + sp.width) as f32,
                    (y + sp.height) as f32,
                ],
                uv: sp.uv,
                color: argb_to_rgba(color, 255),
                clip,
                rotation: 0.0,
            });
        }
    }

    pub(super) fn draw_node(
        &self,
        id: NodeId,
        inherited: u32,
        clip: Option<[i32; 4]>,
        out: &mut Vec<Quad>,
    ) {
        let n = &self.nodes[id];
        if !n.visible() {
            return;
        }
        let mut color = modulate(n.color, inherited);
        if n.flags & flags::DIM != 0 {
            color = (color & 0x00FF_FFFF) | ((color >> 25) << 24);
        }
        let [l, t, r, b] = n.rect;
        if let Some(border) = n.border {
            self.draw_node(border, color, clip, out);
        }
        match &n.kind {
            Kind::Control(c) => self.draw_control(id, c, color, clip, out),
            Kind::Border { sprites } => {
                let w = |i: usize| sprites[i].map_or(0, |s| s.width);
                let h = |i: usize| sprites[i].map_or(0, |s| s.height);
                let cells = [
                    [l, t, l + w(0), t + h(0)],
                    [l + w(0), t, r - w(2), t + h(1)],
                    [r - w(2), t, r, t + h(2)],
                    [l, t + h(0), l + w(3), b - h(6)],
                    [l + w(3), t + h(1), r - w(5), b - h(7)],
                    [r - w(5), t + h(2), r, b - h(8)],
                    [l, b - h(6), l + w(6), b],
                    [l + w(6), b - h(7), r - w(8), b],
                    [r - w(8), b - h(8), r, b],
                ];
                for (s, rect) in sprites.iter().zip(cells) {
                    if let Some(s) = s {
                        Self::sprite_quad(out, s, rect, color, clip);
                    }
                }
            }
            Kind::Image {
                sprite: Some(s), ..
            } => {
                Self::sprite_quad(out, s, n.rect, color, clip);
                if n.rotation != 0.0 {
                    if let Some(q) = out.last_mut() {
                        q.rotation = n.rotation;
                    }
                }
            }
            Kind::Lines {
                sprite: Some(s),
                lines,
                ..
            } => {
                // One-pixel steps along each segment, clipped to the element.
                let r = n.rect;
                let clip = Some(match clip {
                    Some(c) => [
                        c[0].max(r[0]),
                        c[1].max(r[1]),
                        c[2].min(r[2]),
                        c[3].min(r[3]),
                    ],
                    None => r,
                });
                for &[x0, y0, x1, y1] in lines {
                    let steps = (x1 - x0).abs().max((y1 - y0).abs()).max(1);
                    for i in 0..=steps {
                        let x = r[0] + x0 + (x1 - x0) * i / steps;
                        let y = r[1] + y0 + (y1 - y0) * i / steps;
                        Self::sprite_quad(out, s, [x, y, x + 1, y + 1], color, clip);
                    }
                }
            }
            Kind::AnimatedImage { sprites, frame, .. } => {
                if let Some(Some(sprite)) = sprites.get(*frame) {
                    Self::sprite_quad(out, sprite, n.rect, color, clip);
                }
            }
            Kind::Button {
                states,
                content,
                colors,
                state,
            } => {
                // A button made unusable shows its disabled state at once, not only after the
                // pointer next moves (`ButtonElement` draws from its enabled flag).
                let state = &if n.usable() { *state } else { 1 };
                if let Some(s) = states[*state].or(states[0]) {
                    self.draw_node(s, color, clip, out);
                }
                if let Some(c) = content {
                    let tint = if colors[*state] != 0 {
                        modulate(colors[*state], color)
                    } else {
                        color
                    };
                    self.draw_node(*c, tint, clip, out);
                }
            }
            Kind::Text(text) => {
                let x = match text.justify {
                    1 => l + ((r - l) - text.layout.width) / 2,
                    2 => r - text.layout.width,
                    _ => l,
                };
                self.draw_text(text, [x, t], color, clip, out);
            }
            Kind::Label {
                text,
                icon,
                icon_size,
                text_alignment,
                orientation,
                gap,
                icon_color,
                ..
            } => {
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
                let (tw, th) = (text.layout.width, text.layout.height);
                // Areas for the text and the icon by orientation (LabelElement::Draw).
                let (text_area, icon_pos) = match orientation {
                    0 => ([l, t, r - gap - iw, b], [r - iw, t + ((b - t) - ih) / 2]),
                    1 => ([l + iw + gap, t, r, b], [l, t + ((b - t) - ih) / 2]),
                    3 => ([l, t + ih + gap, r, b], [l + ((r - l) - iw) / 2, t]),
                    _ => ([l, t, r, b - ih - gap], [l + ((r - l) - iw) / 2, b - ih]),
                };
                let [al, at, ar, ab] = text_area;
                let x = match text_alignment % 3 {
                    1 => al + ((ar - al) - tw) / 2,
                    2 => ar - tw,
                    _ => al,
                };
                let y = match text_alignment / 3 {
                    1 => at + ((ab - at) - th) / 2,
                    2 => ab - th,
                    _ => at,
                };
                if let Some(s) = icon {
                    Self::sprite_quad(
                        out,
                        s,
                        [icon_pos[0], icon_pos[1], icon_pos[0] + iw, icon_pos[1] + ih],
                        *icon_color,
                        clip,
                    );
                }
                self.draw_text(text, [x, y], color, clip, out);
            }
            Kind::List(list) => {
                let list_clip = controls::intersect(clip, [l, t, r, b]);
                let first = (list.scroll / list.row_height.max(1)).max(0) as usize;
                let rows = ((b - t) / list.row_height.max(1) + 2) as usize;
                for i in first..(first + rows).min(list.items.len()) {
                    let y = t + i as i32 * list.row_height - list.scroll;
                    let row = [l, y, r, y + list.row_height];
                    let tint = if list.selected == Some(i) {
                        Some(0xC0C9_A24A)
                    } else if list.hovered == Some(i) {
                        Some(0x40FF_FFFF)
                    } else {
                        None
                    };
                    if let (Some(c), Some(s)) = (tint, &list.highlight) {
                        Self::sprite_quad(out, s, row, c, list_clip);
                    }
                    let layout = match list.font {
                        Some(f) => TextLayout::compute(&self.res, f, &list.items[i], None, 0),
                        None => continue,
                    };
                    let text = TextData {
                        font: list.font,
                        text: String::new(),
                        max_width: 0,
                        justify: 0,
                        wrap: false,
                        shadow: false,
                        ellipsis_width: 0,
                        layout,
                    };
                    // Inferred: an item's sprite at the row's left, fitted to the row's
                    // height, its text after it.
                    let mut x = l + 6;
                    if let Some(Some(s)) = list.icons.get(i) {
                        let h = s.height.min(list.row_height).max(1);
                        let w = (s.width * h / s.height.max(1)).max(1);
                        let iy = y + (list.row_height - h) / 2;
                        Self::sprite_quad(out, s, [x, iy, x + w, iy + h], color, list_clip);
                        x += w + 4;
                    }
                    let ty = y + (list.row_height - text.layout.height) / 2;
                    self.draw_text(&text, [x, ty], color, list_clip, out);
                }
                // Scroll indicator.
                let total = list.items.len() as i32 * list.row_height;
                if total > b - t {
                    if let Some(s) = &list.highlight {
                        let h = ((b - t) * (b - t) / total).max(12);
                        let y = t + (list.scroll * ((b - t) - h)) / (total - (b - t)).max(1);
                        Self::sprite_quad(out, s, [r - 4, y, r, y + h], 0x80FF_FFFF, list_clip);
                    }
                }
            }
            Kind::Picture { texture, size } if size[0] > 0 && size[1] > 0 => {
                // Fit inside the rect, preserving aspect, never upscaling past 4x.
                let (aw, ah) = ((r - l) as f32, (b - t) as f32);
                let scale = (aw / size[0] as f32).min(ah / size[1] as f32).min(4.0);
                let (w, h) = (size[0] as f32 * scale, size[1] as f32 * scale);
                let (x, y) = (l as f32 + (aw - w) / 2.0, t as f32 + (ah - h) / 2.0);
                out.push(Quad {
                    texture: Texture::External(*texture),
                    rect: [x, y, x + w, y + h],
                    uv: [0.0, 0.0, 1.0, 1.0],
                    color: [255, 255, 255, 255],
                    clip,
                    rotation: 0.0,
                });
            }
            _ => {}
        }
        for &c in &n.children {
            self.draw_node(c, color, clip, out);
        }
    }
}
