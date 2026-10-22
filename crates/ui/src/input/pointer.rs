//! The pointer: moving and pressing, dragging controls and scroll bars, the wheel, which element is hit, and routing an event to the element that takes it.

mod events;

use super::*;

impl Ui {
    /// Move the pointer to `(x, y)` over the tree at `root`.
    pub fn pointer_move(&mut self, x: i32, y: i32, root: NodeId) {
        self.input_root = Some(root);
        self.pointer = (x, y);
        // Open dropdown lists sit above the tree, so they take the pointer first.
        let popup = self
            .open_lists(root)
            .into_iter()
            .rev()
            .find_map(|l| self.hit(l, x, y, None));
        self.hot = popup.or_else(|| self.hit(root, x, y, None));
        if self.pointer_down {
            if let Some(id) = self.pressed {
                self.drag_control(id, x, y);
                // `ButtonElement::Update`: leaving a pressed button lets it go; coming back
                // before the release does not press it again.
                let button = matches!(
                    self.nodes[id].kind,
                    Kind::Button { .. }
                        | Kind::Control(Control::Check { .. } | Control::Combo { .. })
                ) || self.nodes[id].flags & unbanished_formats::ui::flags::MOUSE != 0;
                if button && self.hot != Some(id) {
                    self.pressed = None;
                }
            }
        }
        for n in &mut self.nodes {
            if let Kind::Control(Control::Slider(s)) = &mut n.kind {
                let k = s.knob(n.rect);
                s.over_knob = k[0] <= x && x <= k[2] && k[1] <= y && y <= k[3];
            }
        }
        self.update_states(root);
        for n in &mut self.nodes {
            if let Kind::List(l) = &mut n.kind {
                l.hovered = None;
            }
            if let Kind::Control(Control::Document(d)) = &mut n.kind {
                d.hovered_link = None;
            }
        }
        if let Some(h) = self.hot {
            let rect = self.nodes[h].rect;
            if let Kind::List(l) = &mut self.nodes[h].kind {
                let row = ((y - rect[1] + l.scroll) / l.row_height.max(1)).max(0) as usize;
                l.hovered = (row < l.items.len()).then_some(row);
            }
            if let Kind::Control(Control::Document(d)) = &mut self.nodes[h].kind {
                d.hovered_link = d
                    .links
                    .iter()
                    .position(|(_, r)| contains(*r, x - rect[0], y - rect[1]));
            }
        }
        self.update_tooltip(root);
    }

    /// Press or release the pointer's button. Returns the click when a button is released over
    /// the element it was pressed on.
    pub fn pointer_button(&mut self, down: bool, root: NodeId) -> Option<Event> {
        // A dropdown is modal with respect to pointer input; an outside click closes it.
        let open: Vec<_> = self
            .active_nodes(root)
            .into_iter()
            .filter(|&id| {
                matches!(
                    self.nodes[id].kind,
                    Kind::Control(Control::Combo { open: true, .. })
                )
            })
            .collect();
        if down {
            for id in open {
                let Kind::Control(Control::Combo { button, list, .. }) = self.nodes[id].kind else {
                    continue;
                };
                if self.hot != Some(id) && self.hot != Some(button) && self.hot != Some(list) {
                    if let Kind::Control(Control::Combo { open, .. }) = &mut self.nodes[id].kind {
                        *open = false;
                    }
                }
            }
        }
        self.pointer_down = down;
        let mut event = None;
        if down {
            self.pressed = self.hot;
            if let Some(f) = self.focus.filter(|&f| Some(f) != self.hot) {
                if matches!(self.nodes[f].kind, Kind::Control(Control::Edit(_))) {
                    self.commit_edit(f);
                }
            }
            self.focus = self.hot;
            // `ButtonElement::Update`: `Play(4)` on press (a list plays nothing; a slider or a
            // scroll bar, below).
            if self.hot.is_some_and(|id| {
                self.interactive(id)
                    && self.nodes[id].usable()
                    && !matches!(
                        self.nodes[id].kind,
                        Kind::List(_)
                            | Kind::Control(
                                Control::Slider(_) | Control::Scroll { .. } | Control::Edit(_)
                            )
                    )
            }) {
                self.sounds.push(4);
            }
            if let Some(id) = self.hot {
                let rect = self.nodes[id].rect;
                match &mut self.nodes[id].kind {
                    Kind::List(l) => {
                        if let Some(row) = l.hovered {
                            l.selected = Some(row);
                            event = Some(Event::Selected(id, row));
                        }
                    }
                    // `EditElement::Update`: a press puts the cursor before the first
                    // character whose right edge reaches the pointer (at the end past the
                    // text, or left of it), and shows the caret.
                    Kind::Control(Control::Edit(e)) => {
                        let target = self.pointer.0 - rect[0];
                        let len = e.text.text.chars().count();
                        e.cursor = (0..len)
                            .find(|&i| {
                                let prefix = e.text.text.chars().take(i + 1).collect::<String>();
                                let x = e.text.font.map_or(0, |f| {
                                    TextLayout::compute(&self.res, f, &prefix, None, 0).width
                                });
                                0 <= target && target <= x
                            })
                            .unwrap_or(len);
                        e.caret = true;
                        e.blink = 0.0;
                    }
                    Kind::Control(Control::Document(d)) => d.pressed_link = d.hovered_link,
                    // `ScrollElement::Update`: on a bar that scrolls, a press on the thumb
                    // starts a drag, one past it pages the view (`page / (1 - page)` of the
                    // range) that way; either plays 4.
                    Kind::Control(Control::Scroll {
                        sprites,
                        vertical,
                        value,
                        page,
                        inset,
                        drag,
                        ..
                    }) if *page < 1.0 => {
                        let (x, y) = self.pointer;
                        let thumb = scroll_thumb(rect, *vertical, *value, *page, *inset, sprites);
                        let step = *page / (1.0 - *page);
                        if thumb[0] <= x && x <= thumb[2] && thumb[1] <= y && y <= thumb[3] {
                            *drag = Some((*value, if *vertical { y } else { x }));
                            self.sounds.push(4);
                        } else if (*vertical && thumb[3] < y) || thumb[2] < x {
                            let v = *value + step;
                            self.set_value(id, v);
                            self.sounds.push(4);
                            self.pending.push(Event::Changed(id));
                            self.scroll_parent(id);
                        } else if (*vertical && y < thumb[1]) || x < thumb[0] {
                            let v = *value - step;
                            self.set_value(id, v);
                            self.sounds.push(4);
                            self.pending.push(Event::Changed(id));
                            self.scroll_parent(id);
                        }
                    }
                    // `SliderElement::Update`: a press on the knob starts a drag; one beside it
                    // on the slider moves the value a step towards the pointer (event `0x17`).
                    // Either plays 4.
                    Kind::Control(Control::Slider(sl)) => {
                        let (x, y) = self.pointer;
                        let knob = sl.knob(rect);
                        if knob[0] <= x && x <= knob[2] && knob[1] <= y && y <= knob[3] {
                            sl.drag = Some((sl.value, x));
                            self.sounds.push(4);
                        } else if knob[0] <= x && x <= knob[2] {
                        } else {
                            let step = if knob[2] < x { sl.step() } else { -sl.step() };
                            let value = sl.value + step;
                            self.set_value(id, value);
                            self.sounds.push(4);
                            event = Some(Event::Changed(id));
                        }
                    }
                    _ => {
                        self.drag_control(id, self.pointer.0, self.pointer.1);
                    }
                }
            }
        } else {
            // `SliderElement::Update` / `ScrollElement::Update`: the drag ends; within 32 px it
            // plays 5.
            if let Some(id) = self.pressed {
                let rect = self.nodes[id].rect;
                let (x, y) = self.pointer;
                let drag = match &mut self.nodes[id].kind {
                    Kind::Control(Control::Slider(sl)) => sl.drag.take(),
                    Kind::Control(Control::Scroll { drag, .. }) => drag.take(),
                    _ => None,
                };
                if let Some((start, _)) = drag {
                    let near = rect[0] - 32 <= x
                        && x <= rect[2] + 32
                        && rect[1] - 32 <= y
                        && y <= rect[3] + 32;
                    if near {
                        self.sounds.push(5);
                    } else {
                        // Deviation: the original keeps the drag flag after a release far
                        // away (until a release near it); the port ends the drag there.
                        self.set_value(id, start);
                        self.scroll_parent(id);
                    }
                }
            }
            if let Some(id) = self.pressed.filter(|p| Some(*p) == self.hot) {
                event = self.activate(id).or(event);
                // `Play(5)` with the click event (`0x12`).
                if event.is_some() {
                    self.sounds.push(5);
                }
            }
            self.pressed = None;
        }
        if let Some(e) = event {
            event = Some(self.route_event(root, e));
        }
        if !down {
            for n in &mut self.nodes {
                if let Kind::Control(Control::Document(d)) = &mut n.kind {
                    d.pressed_link = None;
                }
            }
        }
        self.update_states(root);
        event
    }

    /// `ContainerElement::Update`: the wheel over a container queues 7 frames of scrolling
    /// (up for a positive `delta`), which [`Ui::update_containers`] plays out.
    pub fn wheel(&mut self, delta: i32) -> bool {
        if delta == 0 {
            return false;
        }
        let (x, y) = self.pointer;
        // The innermost visible container under the pointer.
        let active = self
            .input_root
            .map(|r| self.active_nodes(r))
            .unwrap_or_default();
        let hovered = active
            .into_iter()
            .filter_map(|id| {
                let n = &self.nodes[id];
                matches!(n.kind, Kind::Control(Control::Container { .. }))
                    .then_some(n.rect)
                    .filter(|r| contains(*r, x, y))
                    .map(|r| (id, (r[2] - r[0]) * (r[3] - r[1])))
            })
            .min_by_key(|c| c.1);
        if let Some((id, _)) = hovered {
            if let Kind::Control(Control::Container { wheel, .. }) = &mut self.nodes[id].kind {
                *wheel += if 0 < delta { -7 } else { 7 };
            }
            return true;
        }
        // A list outside any container (the port's own popups) scrolls directly.
        if let Some(h) = self.hot {
            let height = self.nodes[h].rect[3] - self.nodes[h].rect[1];
            if let Kind::List(l) = &mut self.nodes[h].kind {
                let max = (l.items.len() as i32 * l.row_height - height).max(0);
                l.scroll = (l.scroll - delta).clamp(0, max);
                return true;
            }
        }
        false
    }

    /// `ContainerElement::Update`, each frame: one queued wheel frame moves the vertical bar a
    /// seventh of a third of the view (`0.33 / 7`), within the content.
    pub fn update_containers(&mut self) {
        for id in 0..self.nodes.len() {
            let Kind::Control(Control::Container {
                content,
                bars,
                view,
                wheel,
                ..
            }) = self.nodes[id].kind
            else {
                continue;
            };
            if wheel == 0 || bars[1].is_none() {
                continue;
            }
            let height = (view[3] - view[1]) as f32;
            let max = content.map_or(0.0, |n| self.nodes[n].size[1] as f32 - height);
            if max <= 0.0 {
                continue;
            }
            if let Kind::Control(Control::Container {
                offset,
                wheel,
                fine,
                ..
            }) = &mut self.nodes[id].kind
            {
                if *fine as i32 != offset[1] {
                    *fine = offset[1] as f32;
                }
                let mut step = (max / (max / height)) / 7.0 * 0.33;
                if *wheel < 0 {
                    step = -step;
                    *wheel += 1;
                } else {
                    *wheel -= 1;
                }
                *fine = (*fine + step).clamp(0.0, max);
                offset[1] = *fine as i32;
            }
        }
    }

    pub(crate) fn active_nodes(&self, root: NodeId) -> Vec<NodeId> {
        fn visit(ui: &Ui, id: NodeId, out: &mut Vec<NodeId>) {
            let n = &ui.nodes[id];
            if !n.visible() || !n.enabled {
                return;
            }
            // A locked element is passed by, its children kept.
            if n.usable() {
                out.push(id);
            }
            let children = match &n.kind {
                Kind::Control(c) => c.active_children(),
                Kind::Button { .. } => Vec::new(),
                _ => n.children.clone(),
            };
            for c in children {
                visit(ui, c, out);
            }
        }
        let mut out = Vec::new();
        visit(self, root, &mut out);
        out
    }

    pub(super) fn interactive(&self, id: NodeId) -> bool {
        if self.nodes[id].flags & unbanished_formats::ui::flags::MOUSE != 0 {
            return true;
        }
        match &self.nodes[id].kind {
            Kind::Button { .. } | Kind::List(_) => true,
            Kind::Control(
                Control::Slider(_)
                | Control::Scroll { .. }
                | Control::Check { .. }
                | Control::Combo { .. },
            ) => true,
            Kind::Control(Control::Edit(e)) => e.max_length > 0,
            Kind::Control(Control::Document(d)) => !d.links.is_empty(),
            _ => false,
        }
    }

    pub(super) fn hit(&self, id: NodeId, x: i32, y: i32, clip: Option<[i32; 4]>) -> Option<NodeId> {
        let n = &self.nodes[id];
        if !n.visible() || !n.enabled || clip.is_some_and(|r| !contains(r, x, y)) {
            return None;
        }
        let mut child_clip = clip;
        if let Kind::Control(Control::Document(d)) = &n.kind {
            return d
                .links
                .iter()
                .any(|(_, r)| contains(*r, x - n.rect[0], y - n.rect[1]))
                .then_some(id);
        }
        let children = match &n.kind {
            Kind::Control(Control::Container {
                content,
                bars,
                view,
                ..
            }) => {
                for c in bars.iter().flatten().rev() {
                    if let Some(h) = self.hit(*c, x, y, clip) {
                        return Some(h);
                    }
                }
                if let Some(c) = content {
                    return self.hit(*c, x, y, intersect(clip, *view));
                }
                return None;
            }
            Kind::Control(Control::Clip { .. }) => {
                child_clip = intersect(clip, n.rect);
                if let Kind::Control(c) = &n.kind {
                    c.active_children()
                } else {
                    Vec::new()
                }
            }
            Kind::Control(Control::Check { .. } | Control::Combo { .. }) => {
                if let Kind::Control(Control::Combo {
                    list, open: true, ..
                }) = &n.kind
                {
                    if let Some(h) = self.hit(*list, x, y, None) {
                        return Some(h);
                    }
                }
                return n.contains(x, y).then_some(id);
            }
            Kind::Control(c) => c.active_children(),
            _ => n.children.clone(),
        };
        for child in children.into_iter().rev() {
            if let Some(h) = self.hit(child, x, y, child_clip) {
                return Some(h);
            }
        }
        (self.interactive(id) && n.contains(x, y)).then_some(id)
    }
}
