//! Layout: each element's size (`ComputeSize`), then its rectangle (`ComputeLayout`).

use super::*;

impl Ui {
    pub(super) fn padded_size(&mut self, id: NodeId) -> [i32; 2] {
        self.compute_size(id);
        let n = &self.nodes[id];
        let mut w = (n.size[0] + n.margins[0] + n.margins[1]).max(n.min_size[0]);
        let mut h = (n.size[1] + n.margins[2] + n.margins[3]).max(n.min_size[1]);
        if n.flags & flags::SCREEN_WIDTH != 0 {
            w = self.screen[0];
        }
        if n.flags & flags::SCREEN_HEIGHT != 0 {
            h = self.screen[1];
        }
        [w, h]
    }

    /// Start a layout pass: every node is measured again, once.
    pub(super) fn begin_size_pass(&mut self) {
        self.size_pass = self.size_pass.wrapping_add(1).max(1);
    }

    /// `ComputeSize` for each element class. Within one layout pass a node's size is computed
    /// once: a size depends only on the node and its children, which a pass does not change,
    /// and nested layouts ask for their children's sizes both to measure and to arrange.
    pub(super) fn compute_size(&mut self, id: NodeId) {
        if self.nodes[id].sized_pass == self.size_pass {
            return;
        }
        let children = self.nodes[id].children.clone();
        let size = match self.nodes[id].kind.clone() {
            Kind::Group => {
                let mut s = [0, 0];
                for c in children {
                    if !self.nodes[c].visible() {
                        continue;
                    }
                    let p = self.padded_size(c);
                    s = [s[0].max(p[0]), s[1].max(p[1])];
                }
                s
            }
            Kind::Ribbon {
                vertical,
                uniform,
                spacing,
                ..
            } => {
                let (mut max, mut sum, mut cells, mut non_empty) = ([0, 0], [0, 0], Vec::new(), 0);
                for &c in &children {
                    if !self.nodes[c].visible() {
                        cells.push(0);
                        continue;
                    }
                    let p = self.padded_size(c);
                    max = [max[0].max(p[0]), max[1].max(p[1])];
                    sum = [sum[0] + p[0], sum[1] + p[1]];
                    cells.push(if vertical { p[1] } else { p[0] });
                    if p[0] > 0 && p[1] > 0 {
                        non_empty += 1;
                    }
                }
                if uniform {
                    let cell = if vertical { max[1] } else { max[0] };
                    cells.iter_mut().for_each(|c| *c = cell);
                    sum = [
                        children.len() as i32 * max[0],
                        children.len() as i32 * max[1],
                    ];
                }
                let gaps = spacing * (non_empty - 1).max(0);
                if let Kind::Ribbon { cells: stored, .. } = &mut self.nodes[id].kind {
                    *stored = cells;
                }
                if vertical {
                    [max[0], sum[1] + gaps]
                } else {
                    [sum[0] + gaps, max[1]]
                }
            }
            Kind::Border { sprites } => {
                let w = |i: usize| sprites[i].map_or(0, |s| s.width);
                let h = |i: usize| sprites[i].map_or(0, |s| s.height);
                [
                    (w(0) + w(2)).max(w(6) + w(8)),
                    (h(0) + h(6)).max(h(2) + h(8)),
                ]
            }
            Kind::Lines { size, .. } => size,
            Kind::Image { sprite, size } => [
                if size[0] > 0 {
                    size[0]
                } else {
                    sprite.map_or(0, |s| s.width)
                },
                if size[1] > 0 {
                    size[1]
                } else {
                    sprite.map_or(0, |s| s.height)
                },
            ],
            Kind::AnimatedImage {
                sprites,
                size,
                frame,
                ..
            } => {
                let sprite = sprites.get(frame).copied().flatten();
                [
                    if size[0] > 0 {
                        size[0]
                    } else {
                        sprite.map_or(0, |s| s.width)
                    },
                    if size[1] > 0 {
                        size[1]
                    } else {
                        sprite.map_or(0, |s| s.height)
                    },
                ]
            }
            Kind::Button {
                states,
                content,
                state,
                ..
            } => {
                let mut s = content.map_or([0, 0], |c| self.padded_size(c));
                if let Some(st) = states[state].or(states[0]) {
                    let p = self.padded_size(st);
                    s = [s[0].max(p[0]), s[1].max(p[1])];
                }
                let n = &self.nodes[id];
                [s[0].max(n.min_size[0]), s[1].max(n.min_size[1])]
            }
            Kind::Text(mut t) => {
                self.layout_text(&mut t);
                let s = [t.layout.width, t.layout.height];
                self.nodes[id].kind = Kind::Text(t);
                s
            }
            Kind::Label {
                mut text,
                icon,
                icon_size,
                orientation,
                min_text_width,
                gap,
                ..
            } => {
                self.layout_text(&mut text);
                let (tw, th) = (
                    text.layout.width.max(if text.text.is_empty() {
                        0
                    } else {
                        min_text_width
                    }),
                    text.layout.height,
                );
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
                if let Kind::Label { text: t, .. } = &mut self.nodes[id].kind {
                    *t = text;
                }
                if orientation > 1 {
                    [iw.max(tw), gap + ih + th]
                } else {
                    [gap + iw + tw, ih.max(th)]
                }
            }
            Kind::Picture { .. } | Kind::Viewport | Kind::List(_) => {
                for c in children {
                    self.compute_size(c);
                }
                self.nodes[id].min_size
            }
            Kind::Unsupported(_) => [0, 0],
            Kind::Control(c) => self.measure_control(id, c),
        };
        self.nodes[id].size = [
            size[0].max(self.nodes[id].min_size[0]),
            size[1].max(self.nodes[id].min_size[1]),
        ];
        if let Some(b) = self.nodes[id].border {
            self.compute_size(b);
        }
        self.nodes[id].sized_pass = self.size_pass;
    }

    pub(super) fn layout_text(&self, t: &mut TextData) {
        let mut display = t.text.clone();
        if !t.wrap && t.ellipsis_width > 0 {
            if let Some(f) = t.font {
                if TextLayout::compute(&self.res, f, &display, None, 0).width > t.ellipsis_width {
                    while !display.is_empty()
                        && TextLayout::compute(&self.res, f, &format!("{display}..."), None, 0)
                            .width
                            > t.ellipsis_width
                    {
                        display.pop();
                    }
                    display.push_str("...");
                }
            }
        }
        t.layout = match t.font {
            Some(f) => TextLayout::compute(
                &self.res,
                f,
                &display,
                t.wrap.then_some(t.max_width),
                t.justify,
            ),
            None => TextLayout::default(),
        };
    }

    /// Generic placement shared by most `ComputeLayout` implementations.
    pub(super) fn place(&mut self, id: NodeId, l: i32, t: i32, r: i32, b: i32) {
        self.place_filling(id, l, t, r, b, 0);
    }

    /// [`Ui::place`] with `fill` (`FILL_WIDTH`, `FILL_HEIGHT`) added to the node's own flags.
    pub(super) fn place_filling(
        &mut self,
        id: NodeId,
        mut l: i32,
        mut t: i32,
        r: i32,
        b: i32,
        fill: u32,
    ) {
        let n = &mut self.nodes[id];
        let fill = n.flags | fill;
        let (mut w, mut h) = (n.size[0], n.size[1]);
        if r < w + l {
            l = r - w;
        }
        if b < h + t {
            t = b - h;
        }
        let [ml, mr, mt, mb] = n.margins;
        let (mut x, mut y) = (l + ml, t + mt);
        let (aw, ah) = ((r - mr) - x, (b - mb) - y);
        if fill & flags::FILL_WIDTH != 0 {
            w = aw;
        }
        if fill & flags::FILL_HEIGHT != 0 {
            h = ah;
        }
        match n.alignment % 3 {
            1 => x += (aw >> 1) - (w >> 1),
            2 => x = (r - mr) - w,
            _ => {}
        }
        match n.alignment / 3 {
            1 => y += (ah >> 1) - (h >> 1),
            2 => y = (b - mb) - h,
            _ => {}
        }
        n.rect = [x, y, x + w, y + h];
    }

    /// Size and lay out `root` inside the given rectangle.
    pub fn layout(&mut self, root: NodeId, rect: [i32; 4]) {
        self.begin_size_pass();
        self.compute_size(root);
        self.compute_layout(root, rect[0], rect[1], rect[2], rect[3]);
    }

    /// Lay out a dialog's root element: placed at its own size by its alignment within `rect`
    /// (`DialogInstance::ComputeLayout`), unlike ordinary groups which fill.
    ///
    /// Original: Runtime `0x180116620`.
    pub fn layout_dialog(&mut self, root: NodeId, rect: [i32; 4]) {
        self.begin_size_pass();
        self.compute_size(root);
        if !matches!(self.nodes[root].kind, Kind::Group) {
            self.compute_layout(root, rect[0], rect[1], rect[2], rect[3]);
            return;
        }
        self.place(root, rect[0], rect[1], rect[2], rect[3]);
        let rc = self.nodes[root].rect;
        if let Some(border) = self.nodes[root].border {
            self.compute_layout(border, rc[0], rc[1], rc[2], rc[3]);
        }
        for c in self.nodes[root].children.clone() {
            if self.nodes[c].visible() {
                self.compute_layout(c, rc[0], rc[1], rc[2], rc[3]);
            }
        }
    }

    /// `ComputeLayout` for each element class.
    pub(super) fn compute_layout(&mut self, id: NodeId, l: i32, t: i32, r: i32, b: i32) {
        let slot = [l, t, r, b];
        if let Kind::Control(c) = &self.nodes[id].kind {
            // `ScrollElement::ComputeLayout` (Runtime 0x18011d7b0): a bar always fills its own
            // axis (a vertical bar the height, a horizontal one the width); the flags only
            // decide the other axis.
            let fill = match c {
                Control::Scroll { vertical: true, .. } => flags::FILL_HEIGHT,
                Control::Scroll {
                    vertical: false, ..
                } => flags::FILL_WIDTH,
                _ => 0,
            };
            self.place_filling(id, l, t, r, b, fill);
            self.layout_control(id);
        } else {
            match self.nodes[id].kind.clone() {
                Kind::Group
                | Kind::Viewport
                | Kind::List(_)
                | Kind::Picture { .. }
                | Kind::Image { .. }
                | Kind::Lines { .. }
                | Kind::AnimatedImage { .. } => {
                    // GroupElement fills the space offered, less its margins.
                    let n = &mut self.nodes[id];
                    let [ml, mr, mt, mb] = n.margins;
                    if matches!(n.kind, Kind::Group)
                        || n.flags & (flags::FILL_WIDTH | flags::FILL_HEIGHT) != 0
                    {
                        n.rect = [l + ml, t + mt, r - mr, b - mb];
                        if !matches!(n.kind, Kind::Group) {
                            let full = n.rect;
                            self.place(id, l, t, r, b);
                            let n = &mut self.nodes[id];
                            if n.flags & flags::FILL_WIDTH != 0 {
                                (n.rect[0], n.rect[2]) = (full[0], full[2]);
                            }
                            if n.flags & flags::FILL_HEIGHT != 0 {
                                (n.rect[1], n.rect[3]) = (full[1], full[3]);
                            }
                        }
                    } else {
                        self.place(id, l, t, r, b);
                    }
                    self.reveal_selection(id);
                    let rc = self.nodes[id].rect;
                    for c in self.nodes[id].children.clone() {
                        if self.nodes[c].visible() {
                            self.compute_layout(c, rc[0], rc[1], rc[2], rc[3]);
                        }
                    }
                }
                Kind::Border { .. } => {
                    let n = &mut self.nodes[id];
                    let [ml, mr, mt, mb] = n.margins;
                    n.rect = [l + ml, t + mt, r - mr, b - mb];
                }
                Kind::Ribbon {
                    vertical,
                    spacing,
                    cells,
                    ..
                } => {
                    self.place(id, l, t, r, b);
                    let rc = self.nodes[id].rect;
                    let fill_last = self.nodes[id].flags
                        & if vertical {
                            flags::FILL_HEIGHT
                        } else {
                            flags::FILL_WIDTH
                        }
                        != 0;
                    let children = self.nodes[id].children.clone();
                    let mut offset = 0;
                    for (i, c) in children.iter().copied().enumerate() {
                        if !self.nodes[c].visible() {
                            continue;
                        }
                        let cell = cells.get(i).copied().unwrap_or(0);
                        let last = i + 1 == children.len();
                        let p = self.padded_size(c);
                        if vertical {
                            let bottom = if fill_last && last {
                                rc[3]
                            } else {
                                rc[1] + offset + cell
                            };
                            self.compute_layout(c, rc[0], rc[1] + offset, rc[2], bottom);
                        } else {
                            let right = if fill_last && last {
                                rc[2]
                            } else {
                                rc[0] + offset + cell
                            };
                            self.compute_layout(c, rc[0] + offset, rc[1], right, rc[3]);
                        }
                        offset += cell;
                        if p[0] > 0 && p[1] > 0 {
                            offset += spacing;
                        }
                    }
                }
                Kind::Button {
                    states, content, ..
                } => {
                    self.place(id, l, t, r, b);
                    let rc = self.nodes[id].rect;
                    for c in states.iter().flatten().chain(content.iter()) {
                        self.compute_layout(*c, rc[0], rc[1], rc[2], rc[3]);
                    }
                }
                Kind::Text(_) | Kind::Label { .. } | Kind::Unsupported(_) => {
                    self.place(id, l, t, r, b)
                }
                Kind::Control(_) => unreachable!(),
            }
        }
        // `Element::ComputeLayout` (0x180119160) and its subclasses hand the border the slot
        // the element was given, before the element's own margins: a padded text sits inside
        // its box rather than the box inside the text.
        if let Some(border) = self.nodes[id].border {
            self.compute_layout(border, slot[0], slot[1], slot[2], slot[3]);
        }
    }
}
