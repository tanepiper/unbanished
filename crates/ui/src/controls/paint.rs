//! Drawing the controls, and the dropdown lists drawn above everything else.

use super::*;

/// `ScrollElement::GetBarLayout`: the track is the bar less `inset` at each end; the thumb
/// covers `page` of it, at least its three sprites' length, placed by `value` (0..1).
pub(crate) fn scroll_thumb(
    rc: [i32; 4],
    vertical: bool,
    value: f32,
    page: f32,
    inset: i32,
    sprites: &[Option<SpriteRef>],
) -> [i32; 4] {
    let axis = if vertical { 1 } else { 0 };
    let mut out = rc;
    out[axis] += inset;
    out[axis + 2] -= inset;
    let length = out[axis + 2] - out[axis];
    let least: i32 = sprites
        .iter()
        .skip(3)
        .take(3)
        .flatten()
        .map(|s| if vertical { s.height } else { s.width })
        .sum();
    let size = (length as f32 * page.clamp(0.0, 1.0)).max(least as f32);
    let start = out[axis] + ((length as f32 - size) * value.clamp(0.0, 1.0)) as i32;
    out[axis] = start;
    out[axis + 2] = size as i32 + start;
    out
}

pub(super) fn strip(
    out: &mut Vec<Quad>,
    s: &[Option<SpriteRef>],
    rc: [i32; 4],
    color: u32,
    clip: Option<[i32; 4]>,
    vertical: bool,
) {
    let axis = if vertical { 1 } else { 0 };
    let length = (rc[axis + 2] - rc[axis]).max(0);
    let size = |i: usize| {
        s.get(i)
            .and_then(|s| *s)
            .map_or(0, |s| if vertical { s.height } else { s.width })
    };
    let a = size(0).min(length);
    let b = size(2).min(length - a);
    let mut cells = [rc; 3];
    cells[0][axis + 2] = rc[axis] + a;
    cells[1][axis] = rc[axis] + a;
    cells[1][axis + 2] = rc[axis + 2] - b;
    cells[2][axis] = rc[axis + 2] - b;
    for (s, r) in s.iter().take(3).zip(cells) {
        if let Some(s) = s {
            Ui::sprite_quad(out, s, r, color, clip);
        }
    }
}

impl Ui {
    /// The open dropdown lists under `root`, in tree order.
    pub(crate) fn open_lists(&self, root: NodeId) -> Vec<NodeId> {
        self.active_nodes(root)
            .into_iter()
            .filter_map(|id| match self.nodes[id].kind {
                Kind::Control(Control::Combo {
                    list, open: true, ..
                }) => Some(list),
                _ => None,
            })
            .collect()
    }

    /// Open dropdown lists above everything else in the tree (the original's popup layer),
    /// each on a solid backing.
    pub(crate) fn draw_popups(&self, root: NodeId, out: &mut Vec<Quad>) {
        let solid = self.res.sheets.iter().enumerate().find_map(|(sheet, s)| {
            s.sheet.sprite("White").map(|sp| SpriteRef {
                sheet,
                width: sp.width,
                height: sp.height,
                uv: sp.uv,
            })
        });
        for list in self.open_lists(root) {
            if let Some(sp) = &solid {
                Self::sprite_quad(out, sp, self.nodes[list].rect, 0xFF30281F, None);
            }
            self.draw_node(list, 0xFFFF_FFFF, None, out);
        }
    }

    pub(crate) fn draw_control(
        &self,
        id: NodeId,
        c: &Control,
        color: u32,
        clip: Option<[i32; 4]>,
        out: &mut Vec<Quad>,
    ) {
        let rc = self.nodes[id].rect;
        let [l, t, r, b] = rc;
        let solid = self.res.sheets.iter().enumerate().find_map(|(sheet, s)| {
            s.sheet.sprite("White").map(|sp| SpriteRef {
                sheet,
                width: sp.width,
                height: sp.height,
                uv: sp.uv,
            })
        });
        match c {
            Control::Slider(s) => {
                let track_sprites = &s.sprites[..3];
                let knob_sprite = s
                    .sprites
                    .get(if s.over_knob { 4 } else { 3 })
                    .and_then(|s| *s);
                if track_sprites.iter().any(Option::is_some) {
                    strip(
                        out,
                        track_sprites,
                        [
                            l,
                            t + s.track_y,
                            r,
                            t + s.track_y
                                + s.sprites.get(1).and_then(|s| *s).map_or(4, |s| s.height),
                        ],
                        color,
                        clip,
                        false,
                    );
                }
                let f = fraction(s.value, s.range);
                if let Some(sp) = knob_sprite {
                    let x = s.knob([l, t, r, b])[0];
                    Self::sprite_quad(out, &sp, [x, t, x + sp.width, t + sp.height], color, clip);
                } else if let Some(solid) = solid {
                    let track = [
                        l + s.inset,
                        t + (b - t) / 2 - 3,
                        r - s.inset,
                        t + (b - t) / 2 + 3,
                    ];
                    Self::sprite_quad(out, &solid, track, modulate(0xFF49453F, color), clip);
                    let fill = [
                        track[0],
                        track[1],
                        track[0] + ((track[2] - track[0]) as f32 * f) as i32,
                        track[3],
                    ];
                    Self::sprite_quad(out, &solid, fill, modulate(0xFFC7AF61, color), clip);
                    let x = l + s.inset + ((r - l - 2 * s.inset - 14).max(0) as f32 * f) as i32;
                    let y = t + ((b - t - 18).max(0) / 2);
                    Self::sprite_quad(
                        out,
                        &solid,
                        [x, y, x + 14, y + 18],
                        modulate(0xFFE9E0C8, color),
                        clip,
                    );
                }
            }
            Control::Progress {
                sprites,
                background_color,
                value,
                ..
            } => {
                // `ProgressElement::Draw` (0x18011bb50) tints the track with `+0x120`, which the
                // constructor sets to white and nothing changes; the description's colour
                // (`+0x68`) is not read by `CreateInstance`.
                let _ = background_color;
                strip(out, &sprites[3..], rc, color, clip, false);
                if *value > 0.0 {
                    let fill = [l, t, l + ((r - l) as f32 * value.clamp(0.0, 1.0)) as i32, b];
                    strip(
                        out,
                        &sprites[..3],
                        fill,
                        color,
                        intersect(clip, fill),
                        false,
                    );
                }
            }
            // `ScrollElement::Draw`: the track over the whole bar, then the thumb in its own
            // colour, hovered (sprites 6–8) while the pointer is on a bar that scrolls.
            Control::Scroll {
                sprites,
                vertical,
                value,
                page,
                inset,
                thumb_color,
                ..
            } => {
                strip(out, &sprites[..3], rc, color, clip, *vertical);
                let thumb = scroll_thumb(rc, *vertical, *value, *page, *inset, sprites);
                let start = if self.hot == Some(id) && *page < 1.0 {
                    6
                } else {
                    3
                };
                let thumb_color = (thumb_color & 0x00FF_FFFF)
                    | (((thumb_color >> 24) * (color >> 24) / 255) << 24);
                strip(
                    out,
                    &sprites[start..start + 3],
                    thumb,
                    thumb_color,
                    intersect(clip, rc),
                    *vertical,
                );
            }
            // `EditElement::Draw`: the text (as `*`s for a password) from the left edge, and
            // while it has focus a blinking `|` one pixel before the cursor's character.
            Control::Edit(e) => {
                let clip = intersect(clip, rc);
                let with = |text: String| -> TextData {
                    let mut t = e.text.clone();
                    t.layout = e.text.font.map_or(TextLayout::default(), |f| {
                        TextLayout::compute(&self.res, f, &text, None, 0)
                    });
                    t.text = text;
                    t
                };
                let shown = if e.password {
                    with("*".repeat(e.text.text.chars().count()))
                } else {
                    e.text.clone()
                };
                self.draw_text(&shown, [l, t], color, clip, out);
                if self.focus == Some(id) && e.max_length > 0 && e.caret {
                    let prefix = shown.text.chars().take(e.cursor).collect::<String>();
                    let x = e.text.font.map_or(0, |f| {
                        TextLayout::compute(&self.res, f, &prefix, None, 0).width
                    });
                    self.draw_text(&with("|".into()), [l + x - 1, t], color, clip, out);
                }
            }
            // `GraphElement::Draw`, in `SpriteContext::AddLine`s of its sprite (none without).
            Control::Graph {
                sprite,
                tick,
                y_range,
                x_divisions,
                y_divisions,
                series,
                ..
            } => {
                let line = |out: &mut Vec<Quad>, x0: i32, y0: i32, x1: i32, y1: i32, c: u32| {
                    if let Some(sp) = sprite {
                        Self::line_quads(out, sp, [x0, y0, x1, y1], c, clip);
                    }
                };
                line(out, l, t, l, b, color);
                line(out, l, t, r, t, color);
                line(out, l, b, r, b, color);
                line(out, r, t, r, b, color);
                if 0 <= *y_divisions {
                    let mut y = t as f32;
                    for _ in 0..=*y_divisions {
                        let yi = (y + 0.5) as i32;
                        line(out, l - tick, yi, r, yi, color);
                        y += (b - t) as f32 / *y_divisions as f32;
                    }
                }
                if 0 <= *x_divisions {
                    let mut x = l as f32;
                    for _ in 0..=*x_divisions {
                        let xi = (x + 0.5) as i32;
                        line(out, xi, b, xi, tick + b, color);
                        x += (r - l) as f32 / *x_divisions as f32;
                    }
                }
                let scale = (1.0 / *y_range) * (b - t) as f32;
                for (c, points) in series {
                    let n = points.len() as i32 - 1;
                    let mut x = l as f32;
                    for k in 0..n.max(0) as usize {
                        let next = x + (r - l) as f32 / n as f32;
                        let y0 = b - (points[k] as f32 * scale) as i32;
                        let y1 = b - (points[k + 1] as f32 * scale) as i32;
                        line(out, x as i32, y0, next as i32, y1, *c);
                        x = next;
                    }
                }
            }
            Control::Clip { content, .. } => {
                if let Some(n) = content {
                    self.draw_node(*n, color, intersect(clip, rc), out);
                }
                return;
            }
            Control::Container {
                content,
                bars,
                view,
                ..
            } => {
                if let Some(n) = content {
                    self.draw_node(*n, color, intersect(clip, *view), out);
                }
                for n in bars.iter().flatten() {
                    self.draw_node(*n, color, clip, out);
                }
                return;
            }
            Control::Document(d) => {
                self.draw_document(d, [l, t], color, intersect(clip, rc), out);
            }
            // The open list is a popup, drawn above the whole tree by `Ui::draw_popups`.
            Control::Combo { button, .. } => {
                self.draw_node(*button, color, clip, out);
                return;
            }
            _ => {}
        }
        for child in c.active_children() {
            self.draw_node(child, color, clip, out);
        }
    }
}
