//! The controls' sizes and the layout of what they hold (`ComputeSize` and `ComputeLayout` of each
//! control class).

use super::*;

pub(super) fn axis_size(v: &[i32], gap: i32) -> i32 {
    v.iter().sum::<i32>() + (v.iter().filter(|&&v| v > 0).count() as i32 - 1).max(0) * gap
}

pub(super) fn axis_offset(v: &[i32], i: usize, gap: i32) -> i32 {
    v[..i].iter().sum::<i32>() + v[..i].iter().filter(|&&v| v > 0).count() as i32 * gap
}

pub(super) fn bounds_size(rects: &[[i32; 4]]) -> [i32; 2] {
    [
        rects.iter().map(|r| r[2]).max().unwrap_or(0),
        rects.iter().map(|r| r[3]).max().unwrap_or(0),
    ]
}

impl Ui {
    pub(crate) fn measure_control(&mut self, id: NodeId, mut c: Control) -> [i32; 2] {
        let mut max = [0, 0];
        for child in c.children() {
            let s = self.padded_size(child);
            max = [max[0].max(s[0]), max[1].max(s[1])];
        }
        let size = match &mut c {
            Control::Edit(e) => {
                self.layout_text(&mut e.text);
                [
                    e.text.layout.width.max(24),
                    e.text
                        .layout
                        .height
                        .max(e.text.font.map_or(16, |f| self.res.fonts[f].font.size)),
                ]
            }
            Control::Slider(s) => [
                s.sprites
                    .iter()
                    .take(3)
                    .flatten()
                    .map(|s| s.width)
                    .sum::<i32>()
                    .max(80)
                    + max[0],
                s.sprites
                    .iter()
                    .flatten()
                    .map(|s| s.height)
                    .max()
                    .unwrap_or(16)
                    .max(max[1]),
            ],
            Control::Scroll {
                sprites, vertical, ..
            } => {
                if *vertical {
                    [
                        sprites.first().and_then(|s| *s).map_or(12, |s| s.width),
                        sprites
                            .iter()
                            .take(3)
                            .flatten()
                            .map(|s| s.height)
                            .sum::<i32>()
                            .max(32),
                    ]
                } else {
                    [
                        sprites
                            .iter()
                            .take(3)
                            .flatten()
                            .map(|s| s.width)
                            .sum::<i32>()
                            .max(32),
                        sprites.first().and_then(|s| *s).map_or(12, |s| s.height),
                    ]
                }
            }
            Control::Progress { sprites, .. } => [
                max[0].max(sprites.iter().take(3).flatten().map(|s| s.width).sum()),
                max[1].max(
                    sprites
                        .iter()
                        .flatten()
                        .map(|s| s.height)
                        .max()
                        .unwrap_or(12),
                ),
            ],
            Control::Graph { size, .. } => *size,
            Control::Table {
                cells,
                columns,
                rows,
                spacing,
            } => {
                columns.fill(0);
                rows.fill(0);
                for &(x, y, n) in cells.iter() {
                    if self.nodes[n].visible() {
                        let s = self.padded_size(n);
                        columns[x] = columns[x].max(s[0]);
                        rows[y] = rows[y].max(s[1]);
                    }
                }
                [axis_size(columns, *spacing), axis_size(rows, *spacing)]
            }
            Control::Layout(items) => {
                let rects = self.relative_rects(items);
                bounds_size(&rects)
            }
            Control::Tabs {
                tabs,
                gap,
                header_height,
                ..
            } => {
                let mut w = 0;
                let mut h = 0;
                let mut body = [0, 0];
                for &(a, b) in tabs.iter() {
                    let s = self.padded_size(a);
                    w += s[0] + *gap;
                    h = h.max(s[1]);
                    let s = self.padded_size(b);
                    body = [body[0].max(s[0]), body[1].max(s[1])];
                }
                *header_height = h + 2 * *gap;
                [
                    w.max(body[0]) + 2 * *gap,
                    *header_height + body[1] + 2 * *gap,
                ]
            }
            Control::Clip { size, .. } | Control::Container { size, .. } => [
                if size[0] > 0 { size[0] } else { max[0] },
                if size[1] > 0 { size[1] } else { max[1] },
            ],
            Control::Spinner { parts, .. } => {
                let edit = parts[0].map_or([0, 0], |n| self.padded_size(n));
                let up = parts[1].map_or([0, 0], |n| self.padded_size(n));
                let down = parts[2].map_or([0, 0], |n| self.padded_size(n));
                [edit[0] + up[0].max(down[0]), edit[1].max(up[1] + down[1])]
            }
            Control::Document(d) => {
                self.layout_document(d);
                d.size
            }
            _ => max,
        };
        self.nodes[id].kind = Kind::Control(c);
        size
    }

    pub(super) fn relative_rects(&mut self, items: &[LayoutChild]) -> Vec<[i32; 4]> {
        let sizes: Vec<_> = items.iter().map(|i| self.padded_size(i.node)).collect();
        let mut rects = vec![[0; 4]; items.len()];
        let mut state = vec![0u8; items.len()];
        fn resolve(
            i: usize,
            items: &[LayoutChild],
            sizes: &[[i32; 2]],
            rects: &mut [[i32; 4]],
            state: &mut [u8],
        ) {
            if state[i] != 0 {
                return;
            }
            state[i] = 1;
            let spec = &items[i].spec;
            let mut pos = [0, 0];
            for axis in 0..2 {
                let anchor = items.iter().position(|x| {
                    x.spec.strings[0] == spec.strings[axis + 1]
                        && !spec.strings[axis + 1].is_empty()
                });
                let (start, end) = if let Some(j) = anchor {
                    resolve(j, items, sizes, rects, state);
                    (rects[j][axis], rects[j][axis + 2])
                } else {
                    (0, 0)
                };
                pos[axis] = spec.unknown_i32[axis]
                    + match spec.unknown_u32[axis] {
                        1 => start,
                        2 => end,
                        3 => start - sizes[i][axis],
                        4 => end - sizes[i][axis],
                        5 => (start + end - sizes[i][axis]) / 2,
                        _ => 0,
                    };
            }
            rects[i] = [pos[0], pos[1], pos[0] + sizes[i][0], pos[1] + sizes[i][1]];
            state[i] = 2;
        }
        for i in 0..items.len() {
            resolve(i, items, &sizes, &mut rects, &mut state);
        }
        let minx = rects.iter().map(|r| r[0]).min().unwrap_or(0);
        let miny = rects.iter().map(|r| r[1]).min().unwrap_or(0);
        for r in &mut rects {
            r[0] -= minx;
            r[2] -= minx;
            r[1] -= miny;
            r[3] -= miny;
        }
        rects
    }

    pub(crate) fn layout_control(&mut self, id: NodeId) {
        let Kind::Control(mut c) = self.nodes[id].kind.clone() else {
            return;
        };
        let [l, t, r, b] = self.nodes[id].rect;
        let layout =
            |ui: &mut Ui, n: NodeId, rc: [i32; 4]| ui.compute_layout(n, rc[0], rc[1], rc[2], rc[3]);
        match &mut c {
            Control::Table {
                cells,
                columns,
                rows,
                spacing,
            } => {
                for &(x, y, n) in cells.iter() {
                    let px = l + axis_offset(columns, x, *spacing);
                    let py = t + axis_offset(rows, y, *spacing);
                    layout(self, n, [px, py, px + columns[x], py + rows[y]]);
                }
            }
            Control::Layout(items) => {
                for (item, rc) in items.iter().zip(self.relative_rects(items)) {
                    layout(
                        self,
                        item.node,
                        [l + rc[0], t + rc[1], l + rc[2], t + rc[3]],
                    );
                }
            }
            Control::Tabs {
                decoration,
                tabs,
                gap,
                header_height,
                selected,
            } => {
                for (i, n) in decoration.iter().enumerate() {
                    if let Some(n) = n {
                        layout(
                            self,
                            *n,
                            if i == 0 {
                                [l, t, r, t + *header_height]
                            } else {
                                [l, t + *header_height, r, b]
                            },
                        );
                    }
                }
                let mut x = l + *gap;
                for (a, _) in tabs.iter() {
                    let s = self.padded_size(*a);
                    layout(self, *a, [x, t + *gap, x + s[0], t + *header_height]);
                    x += s[0] + *gap;
                }
                for (i, &(header, _)) in tabs.iter().enumerate() {
                    if let Kind::Button { state, .. } = &mut self.nodes[header].kind {
                        *state = if i == *selected {
                            3
                        } else if self.hot == Some(header) {
                            2
                        } else {
                            0
                        };
                    }
                }
                if let Some((_, n)) = tabs.get(*selected) {
                    layout(
                        self,
                        *n,
                        [l + *gap, t + *header_height + *gap, r - *gap, b - *gap],
                    );
                }
            }
            Control::Container {
                content,
                bars,
                spacing,
                offset,
                view,
                ..
            } => {
                let content_size = content.map_or([0, 0], |n| self.padded_size(n));
                let mut need = [false, false];
                for _ in 0..2 {
                    let w = r
                        - l
                        - if need[1] {
                            bars[1].map_or(0, |n| self.nodes[n].size[0]) + *spacing
                        } else {
                            0
                        };
                    let h = b
                        - t
                        - if need[0] {
                            bars[0].map_or(0, |n| self.nodes[n].size[1]) + *spacing
                        } else {
                            0
                        };
                    need = [
                        bars[0].is_some() && content_size[0] > w,
                        bars[1].is_some() && content_size[1] > h,
                    ];
                }
                let vr = r - if need[1] {
                    bars[1].map_or(0, |n| self.nodes[n].size[0]) + *spacing
                } else {
                    0
                };
                let vb = b - if need[0] {
                    bars[0].map_or(0, |n| self.nodes[n].size[1]) + *spacing
                } else {
                    0
                };
                *view = [l, t, vr.max(l), vb.max(t)];
                for axis in 0..2 {
                    let extent = (view[axis + 2] - view[axis]).max(0);
                    let max_scroll = (content_size[axis] - extent).max(0);
                    offset[axis] = offset[axis].clamp(0, max_scroll);
                    if let Some(bar) = bars[axis] {
                        if let Kind::Control(Control::Scroll { value, page, .. }) =
                            &mut self.nodes[bar].kind
                        {
                            *value = if max_scroll > 0 {
                                offset[axis] as f32 / max_scroll as f32
                            } else {
                                0.0
                            };
                            *page = if content_size[axis] > 0 {
                                (extent as f32 / content_size[axis] as f32).clamp(0.0, 1.0)
                            } else {
                                1.0
                            };
                        }
                        layout(
                            self,
                            bar,
                            if need[axis] {
                                if axis == 0 {
                                    [l, vb + *spacing, vr, b]
                                } else {
                                    [vr + *spacing, t, r, vb]
                                }
                            } else {
                                [0; 4]
                            },
                        );
                    }
                }
                if let Some(n) = content {
                    layout(
                        self,
                        *n,
                        [
                            l - offset[0],
                            t - offset[1],
                            l - offset[0] + content_size[0].max(vr - l),
                            t - offset[1] + content_size[1].max(vb - t),
                        ],
                    );
                }
            }
            Control::Clip {
                content, offset, ..
            } => {
                if let Some(n) = content {
                    let size = self.nodes[*n].size;
                    layout(
                        self,
                        *n,
                        [
                            l - offset[0],
                            t - offset[1],
                            r.max(l + size[0]) - offset[0],
                            b.max(t + size[1]) - offset[1],
                        ],
                    );
                }
            }
            Control::Combo { button, list, .. } => {
                layout(self, *button, [l, t, r, b]);
                self.nodes[*button].rect = [l, t, r, b];
                let height = match &self.nodes[*list].kind {
                    Kind::List(l) => {
                        (l.items.len().min(10) as i32 * l.row_height).max(l.row_height)
                    }
                    _ => 20,
                };
                let top = if b + height > self.screen[1] {
                    (t - height).max(0)
                } else {
                    b
                };
                layout(self, *list, [l, top, r, top + height]);
                self.nodes[*list].rect = [l, top, r, top + height];
            }
            Control::Slider(s) => {
                if let Some(n) = s.number {
                    let width = self.padded_size(n)[0];
                    let knob = s.sprites.get(3).and_then(|sp| *sp).map_or(0, |sp| sp.width);
                    let x = l
                        + s.inset
                        + ((r - l - 2 * s.inset - knob).max(0) as f32 * fraction(s.value, s.range))
                            as i32;
                    let tx = if x + knob + width < r {
                        x + knob + s.inset
                    } else {
                        x - width
                    };
                    layout(self, n, [tx, t + s.track_y, tx + width, b]);
                }
            }
            Control::Spinner { parts, .. } => {
                let width = parts[0].map_or(0, |n| self.padded_size(n)[0]);
                let mid = t + (b - t) / 2;
                if let Some(n) = parts[0] {
                    layout(self, n, [l, t, l + width, b]);
                }
                if let Some(n) = parts[1] {
                    layout(self, n, [l + width, t, r, mid]);
                }
                if let Some(n) = parts[2] {
                    layout(self, n, [l + width, mid, r, b]);
                }
            }
            Control::Edit(e) => {
                self.layout_text(&mut e.text);
            }
            Control::Document(d) => {
                d.width = (r - l).max(1);
                self.layout_document(d);
            }
            Control::Radio { children, selected } => {
                for (i, &n) in children.iter().enumerate() {
                    if let Kind::Control(Control::Check { checked, .. }) = &mut self.nodes[n].kind {
                        *checked = i == *selected;
                    }
                    layout(self, n, [l, t, r, b]);
                }
            }
            _ => {
                for n in c.active_children() {
                    layout(self, n, [l, t, r, b]);
                }
            }
        }
        self.nodes[id].kind = Kind::Control(c);
    }
}
