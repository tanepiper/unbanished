use crate::controls::{intersect, scroll_thumb};
use crate::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// A key the interface reacts to.
pub enum Key {
    /// Tab: the next control.
    Tab,
    /// Shift and Tab: the previous control.
    BackTab,
    /// Enter.
    Enter,
    /// Escape.
    Escape,
    /// The space bar.
    Space,
    /// The left arrow.
    Left,
    /// The right arrow.
    Right,
    /// The up arrow.
    Up,
    /// The down arrow.
    Down,
    /// Home.
    Home,
    /// End.
    End,
    /// Page Up.
    PageUp,
    /// Page Down.
    PageDown,
    /// Backspace.
    Backspace,
    /// Delete.
    Delete,
    /// A character key.
    Character(char),
}

impl Ui {
    /// The element that has the keyboard focus.
    pub fn focused(&self) -> Option<NodeId> {
        self.focus
    }
    /// The pointer in UI units, as last moved.
    pub fn pointer(&self) -> (i32, i32) {
        self.pointer
    }
    /// Whether the current pointer target is a control that consumes pointer input.
    pub fn pointer_over_interactive(&self) -> bool {
        self.hot.is_some_and(|id| self.interactive(id))
    }
    /// `DialogInstance::GetMouseCursorIndex`: the cursor index of `id`, unless one of its
    /// visible children holds `(x, y)`, then that child's, the last such child winning.
    pub fn cursor_index(&self, id: NodeId, x: i32, y: i32) -> i32 {
        let n = &self.nodes[id];
        let mut cursor = n.cursor;
        for &c in &n.children {
            let child = &self.nodes[c];
            if child.visible() && child.contains(x, y) {
                cursor = self.cursor_index(c, x, y);
            }
        }
        cursor
    }

    /// Whether key input should be reserved for an active text edit control.
    pub fn text_edit_focused(&self) -> bool {
        self.focus
            .is_some_and(|id| matches!(&self.nodes[id].kind, Kind::Control(Control::Edit(_))))
    }
    /// The events since the last call, in order. (A click is returned by
    /// [`pointer_button`](Self::pointer_button) instead.)
    pub fn drain_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.pending)
    }
    /// Set the key that clicks an element: an upper-case character code, or 13 (Enter) or 27
    /// (Escape).
    pub fn set_shortcut(&mut self, id: NodeId, key: u32) {
        self.nodes[id].shortcut = Some(key);
    }
    /// Forget the pointer, the focus and the pending events, for example when the window loses the
    /// focus.
    pub fn clear_input(&mut self) {
        self.focus = None;
        self.hot = None;
        self.pressed = None;
        self.pointer_down = false;
        self.pending.clear();
        self.input_root = None;
        self.tooltip_instance = None;
    }

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
                ) || self.nodes[id].flags & banished_formats::ui::flags::MOUSE != 0;
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

    fn drag_control(&mut self, id: NodeId, x: i32, y: i32) {
        let rect = self.nodes[id].rect;
        let mut next = None;
        match &self.nodes[id].kind {
            // `SliderElement::Update`: a drag from the knob follows the pointer while it stays
            // within 32 px of the slider and puts the value back while it is further away
            // (without an event).
            Kind::Control(Control::Slider(s)) => {
                let Some((start, _)) = s.drag else {
                    return;
                };
                let near = rect[0] - 32 <= x
                    && x <= rect[2] + 32
                    && rect[1] - 32 <= y
                    && y <= rect[3] + 32;
                if !near {
                    self.set_value(id, start);
                    return;
                }
                next = s.dragged(rect, x);
            }
            // `ScrollElement::Update`: a drag from the thumb moves the value by the pointer's
            // travel over the bar's length less the thumb's share; more than 32 px away the
            // value goes back.
            Kind::Control(Control::Scroll {
                vertical,
                page,
                drag,
                ..
            }) => {
                let Some((start, from)) = *drag else {
                    return;
                };
                let near = rect[0] - 32 <= x
                    && x <= rect[2] + 32
                    && rect[1] - 32 <= y
                    && y <= rect[3] + 32;
                if !near {
                    next = Some(start);
                } else {
                    let axis = if *vertical { 1 } else { 0 };
                    let p = if *vertical { y } else { x };
                    let length = (rect[axis + 2] - rect[axis]) as f32;
                    next = Some(start - (from - p) as f32 / (length - length * *page));
                }
            }
            _ => {}
        }
        if let Some(v) = next {
            let before = self.value(id);
            self.set_value(id, v);
            if before != self.value(id) {
                self.pending.push(Event::Changed(id));
                self.scroll_parent(id);
            }
        }
    }

    fn scroll_parent(&mut self, bar: NodeId) {
        let value = self.value(bar).unwrap_or(0.0);
        let parent=(0..self.nodes.len()).find(|&id|matches!(&self.nodes[id].kind,Kind::Control(Control::Container{bars,..}) if bars.contains(&Some(bar))));
        if let Some(parent) = parent {
            let Kind::Control(Control::Container {
                bars,
                content,
                view,
                ..
            }) = self.nodes[parent].kind.clone()
            else {
                return;
            };
            let axis = if bars[0] == Some(bar) { 0 } else { 1 };
            let size = content.map_or(0, |n| self.nodes[n].size[axis]);
            let max = (size - (view[axis + 2] - view[axis])).max(0);
            if let Kind::Control(Control::Container { offset, .. }) = &mut self.nodes[parent].kind {
                offset[axis] = (value * max as f32) as i32;
            }
        }
    }

    fn activate(&mut self, id: NodeId) -> Option<Event> {
        if !self.nodes[id].usable() || !self.nodes[id].visible() {
            return None;
        }
        if self.nodes[id].flags & banished_formats::ui::flags::MOUSE != 0 {
            return Some(Event::Clicked(id));
        }
        match &mut self.nodes[id].kind {
            Kind::Button { .. } => Some(Event::Clicked(id)),
            Kind::Control(Control::Check { checked, .. }) => {
                *checked = !*checked;
                Some(Event::Changed(id))
            }
            Kind::Control(Control::Combo { open, .. }) => {
                *open = !*open;
                Some(Event::Clicked(id))
            }
            Kind::Control(Control::Document(d)) => d
                .hovered_link
                .filter(|&i| {
                    d.pressed_link
                        .and_then(|p| d.links.get(p))
                        .is_some_and(|p| p.0 == d.links[i].0)
                })
                .map(|i| Event::Link(id, i)),
            _ => None,
        }
    }

    fn route_event(&mut self, root: NodeId, event: Event) -> Event {
        let id = event_node(event);
        for parent in self.active_nodes(root) {
            let mut selected_text = None;
            let spinner_current = match &self.nodes[parent].kind {
                Kind::Control(Control::Spinner { parts, .. }) => {
                    parts[0].and_then(|n| self.value(n))
                }
                _ => None,
            };
            // `NumberEditElement::SetValue` clamps to the edit's range (`SpinnerElement::SetMinMax`).
            let spinner_range = match &self.nodes[parent].kind {
                Kind::Control(Control::Spinner { parts, .. }) => parts[0]
                    .and_then(|n| match &self.nodes[n].kind {
                        Kind::Control(Control::Edit(e)) => e.range,
                        _ => None,
                    })
                    .map(|r| (r[0].min(r[1]) as i32, r[0].max(r[1]) as i32)),
                _ => None,
            };
            let routed = match &mut self.nodes[parent].kind {
                Kind::Control(Control::Tabs { tabs, selected, .. }) => {
                    if let Some(i) = tabs.iter().position(|&(a, _)| a == id) {
                        *selected = i;
                        Some(Event::Selected(parent, i))
                    } else {
                        None
                    }
                }
                Kind::Control(Control::Radio { children, selected }) => {
                    if let Some(i) = children.iter().position(|&n| n == id) {
                        *selected = i;
                        Some(Event::Selected(parent, i))
                    } else {
                        None
                    }
                }
                Kind::Control(Control::Check {
                    button, checked, ..
                }) if *button == id => {
                    *checked = !*checked;
                    Some(Event::Changed(parent))
                }
                Kind::Control(Control::Combo { button, list, open }) => {
                    if *button == id {
                        *open = !*open;
                        Some(Event::Clicked(parent))
                    } else if let Event::Selected(n, i) = event {
                        if n == *list {
                            // `ComboDialog::OnElementEvent`: the choice plays 5 and closes.
                            selected_text = Some((*button, *list, i));
                            *open = false;
                            self.sounds.push(5);
                            Some(Event::Selected(parent, i))
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                }
                Kind::Control(Control::Spinner { parts, value, step }) => {
                    let delta = if parts[1] == Some(id) {
                        *step
                    } else if parts[2] == Some(id) {
                        -*step
                    } else {
                        0
                    };
                    if delta != 0 {
                        *value =
                            (spinner_current.unwrap_or(*value as f32) as i32).saturating_add(delta);
                        if let Some((lo, hi)) = spinner_range {
                            *value = (*value).clamp(lo, hi);
                        }
                        selected_text = parts[0].map(|n| (n, usize::MAX, *value as usize));
                        Some(Event::Changed(parent))
                    } else if parts[0] == Some(id) {
                        *value = spinner_current.unwrap_or(*value as f32) as i32;
                        Some(Event::Changed(parent))
                    } else {
                        None
                    }
                }
                _ => None,
            };
            if let Some((button, list, i)) = selected_text {
                let text = if list == usize::MAX {
                    self.set_value(button, i as i32 as f32);
                    (i as i32).to_string()
                } else {
                    match &self.nodes[list].kind {
                        Kind::List(l) => l.items.get(i).cloned().unwrap_or_default(),
                        _ => String::new(),
                    }
                };
                if list != usize::MAX {
                    self.set_text(button, &text);
                    let icon = match &self.nodes[list].kind {
                        Kind::List(l) => l.icons.get(i).copied().flatten(),
                        _ => None,
                    };
                    if let Some(icon) = icon {
                        self.set_icon(button, icon);
                    }
                }
            }
            if let Some(routed) = routed {
                return routed;
            }
        }
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
    fn interactive(&self, id: NodeId) -> bool {
        if self.nodes[id].flags & banished_formats::ui::flags::MOUSE != 0 {
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
    fn hit(&self, id: NodeId, x: i32, y: i32, clip: Option<[i32; 4]>) -> Option<NodeId> {
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

    /// Press a key. Returns whether the interface used it.
    pub fn key(&mut self, root: NodeId, key: Key) -> bool {
        let active = self.active_nodes(root);
        if self.focus.is_some_and(|id| !active.contains(&id)) {
            self.focus = None;
        }
        if matches!(key, Key::Tab | Key::BackTab) {
            let focusable: Vec<_> = active
                .iter()
                .copied()
                .filter(|&id| self.interactive(id))
                .collect();
            if focusable.is_empty() {
                return false;
            }
            let current = self
                .focus
                .and_then(|id| focusable.iter().position(|&n| n == id));
            let i = match (current, key) {
                (Some(i), Key::BackTab) => (i + focusable.len() - 1) % focusable.len(),
                (Some(i), _) => (i + 1) % focusable.len(),
                (None, Key::BackTab) => focusable.len() - 1,
                _ => 0,
            };
            self.focus = Some(focusable[i]);
            return true;
        }
        if let Some(id) = self.focus {
            // A combo's popup retains focus while arrows browse; Enter confirms and Escape
            // closes it. These keys must not fall through to the asset browser's navigation.
            let combo = active
                .iter()
                .find_map(|&parent| match self.nodes[parent].kind {
                    Kind::Control(Control::Combo { list, .. }) if parent == id || list == id => {
                        Some((parent, list))
                    }
                    _ => None,
                });
            if let Some((parent, list)) = combo {
                if matches!(key, Key::Up | Key::Down | Key::Home | Key::End) {
                    if let Kind::Control(Control::Combo { open, .. }) = &mut self.nodes[parent].kind
                    {
                        *open = true;
                    }
                    if let Kind::List(l) = &mut self.nodes[list].kind {
                        if !l.items.is_empty() {
                            let last = l.items.len() - 1;
                            let i = match key {
                                Key::Home => 0,
                                Key::End => last,
                                Key::Up => l.selected.unwrap_or(0).saturating_sub(1),
                                _ => l.selected.map_or(0, |i| (i + 1).min(last)),
                            };
                            l.selected = Some(i);
                            l.reveal = true;
                        }
                    }
                    self.focus = Some(list);
                    return true;
                }
                if key == Key::Escape {
                    if let Kind::Control(Control::Combo { open, .. }) = &mut self.nodes[parent].kind
                    {
                        *open = false;
                    }
                    self.focus = Some(parent);
                    return true;
                }
                if id == list && matches!(key, Key::Enter | Key::Space) {
                    if let Kind::List(l) = &self.nodes[list].kind {
                        if let Some(i) = l.selected {
                            let event = self.route_event(root, Event::Selected(list, i));
                            self.pending.push(event);
                        }
                    }
                    self.focus = Some(parent);
                    return true;
                }
            }
            if let Kind::Control(Control::Edit(e)) = &mut self.nodes[id].kind {
                let len = e.text.text.chars().count();
                e.cursor = e.cursor.min(len);
                let mut changed = false;
                match key {
                    Key::Left => e.cursor = e.cursor.saturating_sub(1),
                    Key::Right => e.cursor = (e.cursor + 1).min(len),
                    Key::Home => e.cursor = 0,
                    Key::End => e.cursor = len,
                    Key::Backspace if e.cursor > 0 => {
                        let at = byte_index(&e.text.text, e.cursor);
                        let prev = byte_index(&e.text.text, e.cursor - 1);
                        e.text.text.replace_range(prev..at, "");
                        e.cursor -= 1;
                        changed = true;
                    }
                    Key::Delete if e.cursor < len => {
                        let at = byte_index(&e.text.text, e.cursor);
                        let next = byte_index(&e.text.text, e.cursor + 1);
                        e.text.text.replace_range(at..next, "");
                        changed = true;
                    }
                    // `EditElement::Update`: Enter or Escape gives up the focus.
                    Key::Enter | Key::Escape => {
                        self.commit_edit(id);
                        return true;
                    }
                    Key::Character(_) | Key::Space => return false,
                    _ => return true,
                }
                if changed {
                    self.pending.push(Event::Changed(id));
                }
                return true;
            }
            if let Kind::Control(Control::Slider(s)) = &self.nodes[id].kind {
                let step = s.step();
                let value = match key {
                    Key::Left | Key::Down => s.value - step,
                    Key::Right | Key::Up => s.value + step,
                    Key::Home => s.range[0],
                    Key::End => s.range[1],
                    _ => f32::NAN,
                };
                if value.is_finite() {
                    self.set_value(id, value);
                    self.pending.push(Event::Changed(id));
                    return true;
                }
            }
            if let Kind::List(l) = &mut self.nodes[id].kind {
                let last = l.items.len().saturating_sub(1);
                let current = l.selected.unwrap_or(0);
                let selected = match key {
                    Key::Up => Some(current.saturating_sub(1)),
                    Key::Down => Some(l.selected.map_or(0, |i| (i + 1).min(last))),
                    Key::Home => Some(0),
                    Key::End => Some(last),
                    Key::PageUp => Some(current.saturating_sub(10)),
                    Key::PageDown => Some((current + 10).min(last)),
                    _ => None,
                };
                if let Some(i) = selected.filter(|_| !l.items.is_empty()) {
                    l.selected = Some(i);
                    l.reveal = true;
                    let e = self.route_event(root, Event::Selected(id, i));
                    self.pending.push(e);
                    return true;
                }
            }
            if matches!(key, Key::Enter | Key::Space) {
                if let Some(event) = self.activate(id) {
                    let event = self.route_event(root, event);
                    self.pending.push(event);
                    return true;
                }
            }
        }
        if key == Key::Escape {
            let mut closed = false;
            for id in &active {
                if let Kind::Control(Control::Combo { open, .. }) = &mut self.nodes[*id].kind {
                    closed |= *open;
                    *open = false;
                }
            }
            if closed {
                return true;
            }
        }
        let code = match key {
            Key::Enter => 13,
            Key::Escape => 27,
            Key::Space => 32,
            Key::Character(c) => c.to_ascii_uppercase() as u32,
            _ => 0,
        };
        if code != 0 {
            if let Some(id) = active
                .into_iter()
                .find(|&id| self.nodes[id].shortcut == Some(code))
            {
                if let Some(e) = self.activate(id) {
                    let e = self.route_event(root, e);
                    self.pending.push(e);
                    return true;
                }
            }
        }
        false
    }

    /// Type text into the focused edit box. Returns whether it was used.
    pub fn text_input(&mut self, text: &str) -> bool {
        let Some(id) = self.focus else {
            return false;
        };
        let Kind::Control(Control::Edit(e)) = &mut self.nodes[id].kind else {
            return false;
        };
        if e.max_length == 0 {
            return false;
        }
        let mut changed = false;
        // `EditElement::SelectCharacter`: the keys of a US keyboard give printable ASCII but
        // `:` and `;`; each goes in while the text is shorter than its most and the edit
        // takes it.
        for ch in text.chars() {
            let typable = (' '..='~').contains(&ch) && ch != ':' && ch != ';';
            if !typable
                || e.text.text.chars().count() >= e.max_length
                || (!e.allowed.is_empty() && !e.allowed.contains(ch))
            {
                continue;
            }
            let at = byte_index(&e.text.text, e.cursor);
            e.text.text.insert(at, ch);
            e.cursor += 1;
            changed = true;
        }
        if changed {
            self.pending.push(Event::Changed(id));
        }
        true
    }

    /// `EditElement::Update` losing the focus (Enter, Escape or a press elsewhere): a
    /// NumberEdit's text becomes its number within its range (`NumberEditElement::
    /// FinalizeText`), and event `0x15` goes.
    pub(crate) fn commit_edit(&mut self, id: NodeId) {
        if let Kind::Control(Control::Edit(e)) = &mut self.nodes[id].kind {
            if let Some(range) = e.range {
                let value = to_integer(&e.text.text)
                    .clamp(range[0].min(range[1]) as i32, range[0].max(range[1]) as i32);
                e.text.text = value.to_string();
                e.cursor = e.cursor.min(e.text.text.chars().count());
            }
            self.pending.push(Event::Submitted(id));
        }
        if self.focus == Some(id) {
            self.focus = None;
        }
    }

    /// The caret's blink: every half second while an edit has the focus.
    pub(crate) fn update_caret(&mut self, dt: f32) {
        let Some(id) = self.focus else { return };
        if let Kind::Control(Control::Edit(e)) = &mut self.nodes[id].kind {
            e.blink += dt;
            if 0.5 < e.blink {
                e.caret = !e.caret;
                e.blink = 0.0;
            }
        }
    }

    fn update_states(&mut self, _root: NodeId) {
        for id in 0..self.nodes.len() {
            let hot = self.hot == Some(id) || self.focus == Some(id);
            let pressed = self.pressed == Some(id) && self.pointer_down;
            let enabled = self.nodes[id].usable();
            if let Kind::Button { state, .. } = &mut self.nodes[id].kind {
                *state = if !enabled {
                    1
                } else if hot && pressed {
                    3
                } else if hot {
                    2
                } else {
                    0
                };
            }
            let button = match &self.nodes[id].kind {
                Kind::Control(Control::Check { button, .. } | Control::Combo { button, .. }) => {
                    Some(*button)
                }
                _ => None,
            };
            if let Some(button) = button {
                if let Kind::Button { state, .. } = &mut self.nodes[button].kind {
                    *state = if !enabled {
                        1
                    } else if hot && pressed {
                        3
                    } else if hot {
                        2
                    } else {
                        0
                    };
                }
            }
        }
    }

    /// The tooltip text of the element under the pointer.
    pub fn hovered_tooltip(&self) -> Option<&str> {
        self.hot
            .map(|h| self.nodes[h].tooltip.as_str())
            .filter(|t| !t.is_empty())
    }

    /// The element whose tooltip shows: the topmost element under the pointer (an open
    /// dropdown list first, else the last drawn) or the nearest of its ancestors with tooltip
    /// text. Elements covered by a later dialog do not show theirs.
    fn tooltip_source(&self, root: NodeId) -> Option<NodeId> {
        let (x, y) = self.pointer;
        let under = |id: NodeId| {
            let n = &self.nodes[id];
            n.contains(x, y) && n.rect[2] > n.rect[0] && n.rect[3] > n.rect[1]
        };
        let popup = self.open_lists(root).into_iter().rev().find(|&l| under(l));
        let nodes: Vec<NodeId> = match popup {
            Some(l) => self.active_nodes(l),
            None => self.active_nodes(root),
        };
        let under: Vec<NodeId> = nodes.into_iter().filter(|&id| under(id)).collect();
        let top = *under.last()?;
        under
            .into_iter()
            .rev()
            .find(|&id| !self.nodes[id].tooltip.is_empty() && self.descendant(id, top))
    }

    fn update_tooltip(&mut self, root: NodeId) {
        let Some(source) = self.tooltip_source(root) else {
            return;
        };
        // `UserInterface::UpdateToolTip` (0x180123300): the open tooltip follows the pointer.
        if self.tooltip_instance.is_some_and(|(s, _)| s == source) {
            self.place_tooltip();
            return;
        }
        // `Element::IsToolTipValid` (0x180036140): a tooltip needs the element's own tooltip
        // dialog as well as its text.
        let Some(reference) = self.nodes[source].tooltip_dialog.clone() else {
            return;
        };
        if let Some(dialog) = self.instantiate_dialog(&reference) {
            let text = self.nodes[source].tooltip.clone();
            for name in ["textToolTip", "text", "textDescription"] {
                if let Some(id) = self.find(dialog.root, name) {
                    self.set_parameters(id, &[(0, &text)]);
                    break;
                }
            }
            let controllers: Vec<u32> = dialog.bindings.iter().map(|b| b.controller).collect();
            // `HotKeyToolTipUI` (`FUN_14006b2e0`): with a key binding, its name is parameter 1
            // of `textToolTip` and parameter 0 of `textHotKey`.
            if controllers.contains(&tooltip_controller::HOT_KEY) {
                if let Some(key) = self.nodes[source].hotkey.clone() {
                    if let Some(id) = self.find(dialog.root, "textToolTip") {
                        self.set_parameters(id, &[(1, &key)]);
                    }
                    if let Some(id) = self.find(dialog.root, "textHotKey") {
                        self.set_parameters(id, &[(0, &key)]);
                    }
                }
            }
            // `TitleDescToolTipUI` (`FUN_140080c00`): the element's title and description.
            if controllers.contains(&tooltip_controller::TITLE_DESC) {
                if let Some((title, desc)) = self.nodes[source].title_desc.clone() {
                    for (name, value) in [("textTitle", title), ("textDesc", desc)] {
                        if let Some(id) = self.find(dialog.root, name) {
                            self.set_parameters(id, &[(0, &value)]);
                        }
                    }
                }
            }
            self.tooltip_instance = Some((source, dialog.root));
            self.tooltip_opened = Some((source, dialog.root, controllers));
            self.place_tooltip();
        }
    }

    /// Size and place the open tooltip dialog on the pointer (again after the application
    /// filled it).
    pub fn place_tooltip(&mut self) {
        let Some((_, root)) = self.tooltip_instance else {
            return;
        };
        self.compute_size(root);
        let [w, h] = self.nodes[root].size;
        // `BeginToolTip` / `UpdateToolTip`: `SetUpperLeft(x, y − height, false)`, the tooltip
        // standing on the pointer.
        let (x, y) = (self.pointer.0, self.pointer.1 - h);
        self.layout_dialog(root, [x, y, x + w, y + h]);
    }

    /// The tooltip dialog opened since the last call: source element, dialog root and the
    /// dialog's controller ids. Application controllers fill it, then call `place_tooltip`.
    pub fn new_tooltip(&mut self) -> Option<(NodeId, NodeId, Vec<u32>)> {
        self.tooltip_opened.take()
    }

    /// `UI::TextElement::SetParameter` on a text or label whose text still holds its template.
    pub fn set_parameters(&mut self, id: NodeId, values: &[(usize, &str)]) {
        let current = match &self.nodes[id].kind {
            Kind::Text(t) | Kind::Label { text: t, .. } => t.text.clone(),
            _ => return,
        };
        let mut out = current;
        for (i, v) in values {
            out = out.replace(&format!("@{i}"), v);
        }
        self.set_text(id, &out);
    }
    /// The open tooltip dialog, while its element is still the tooltip source under the
    /// pointer within the input root (a modal dialog hides the tooltips of what it covers).
    pub(crate) fn draw_tooltip(&self, root: NodeId, out: &mut Vec<Quad>) {
        if self.pointer_down {
            return;
        }
        let Some((source, tip)) = self.tooltip_instance else {
            return;
        };
        let input = self.input_root.unwrap_or(root);
        if tip < self.nodes.len() && self.tooltip_source(input) == Some(source) {
            self.draw_node(tip, 0xFFFFFFFF, None, out);
        }
    }
}

fn contains(r: [i32; 4], x: i32, y: i32) -> bool {
    x >= r[0] && x < r[2] && y >= r[1] && y < r[3]
}
fn byte_index(s: &str, cursor: usize) -> usize {
    s.char_indices().nth(cursor).map_or(s.len(), |p| p.0)
}
/// `System::String::ToInteger`: an optional sign and the digits after it, 0 without.
fn to_integer(s: &str) -> i32 {
    let s = s.trim_start();
    let (sign, digits) = match s.strip_prefix('-') {
        Some(rest) => (-1i64, rest),
        None => (1, s.strip_prefix('+').unwrap_or(s)),
    };
    let mut v: i64 = 0;
    for c in digits.chars().take_while(|c| c.is_ascii_digit()) {
        v = (v * 10 + c as i64 - '0' as i64).min(i32::MAX as i64 + 1);
    }
    (sign * v).clamp(i32::MIN as i64, i32::MAX as i64) as i32
}

pub(crate) fn event_node(e: Event) -> NodeId {
    match e {
        Event::Clicked(n)
        | Event::Selected(n, _)
        | Event::Changed(n)
        | Event::Submitted(n)
        | Event::Link(n, _) => n,
    }
}

/// The type ids (CRC-32 of the class name) of the game's tooltip controllers that the interface
/// handles itself.
pub mod tooltip_controller {
    /// `HotKeyToolTipUI`: a tooltip that shows the element's key.
    pub const HOT_KEY: u32 = 0x5dd8_3c93;
    /// `TitleDescToolTipUI`: a tooltip with a title and a description.
    pub const TITLE_DESC: u32 = 0x33e6_1003;
    /// The tooltip that lists what a building needs.
    pub const BUILD_REQUIREMENT: u32 = 0xc3fe_f549;
}
