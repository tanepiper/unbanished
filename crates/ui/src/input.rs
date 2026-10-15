use crate::controls::{intersect, scroll_thumb};
use crate::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Tab,
    BackTab,
    Enter,
    Escape,
    Space,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
    Backspace,
    Delete,
    Character(char),
}

impl Ui {
    pub fn focused(&self) -> Option<NodeId> {
        self.focus
    }
    /// Whether the current pointer target is a control that consumes pointer input.
    pub fn pointer_over_interactive(&self) -> bool {
        self.hot.is_some_and(|id| self.interactive(id))
    }
    /// Whether key input should be reserved for an active text edit control.
    pub fn text_edit_focused(&self) -> bool {
        self.focus
            .is_some_and(|id| matches!(&self.nodes[id].kind, Kind::Control(Control::Edit(_))))
    }
    pub fn drain_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.pending)
    }
    pub fn set_shortcut(&mut self, id: NodeId, key: u32) {
        self.nodes[id].shortcut = Some(key);
    }
    pub fn clear_input(&mut self) {
        self.focus = None;
        self.hot = None;
        self.pressed = None;
        self.pointer_down = false;
        self.pending.clear();
        self.input_root = None;
        self.tooltip_instance = None;
    }

    pub fn pointer_move(&mut self, x: i32, y: i32, root: NodeId) {
        self.input_root = Some(root);
        self.pointer = (x, y);
        self.hot = self.hit(root, x, y, None);
        if self.pointer_down {
            if let Some(id) = self.pressed {
                self.drag_control(id, x, y);
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
            self.focus = self.hot;
            if let Some(id) = self.hot {
                let rect = self.nodes[id].rect;
                match &mut self.nodes[id].kind {
                    Kind::List(l) => {
                        if let Some(row) = l.hovered {
                            l.selected = Some(row);
                            event = Some(Event::Selected(id, row));
                        }
                    }
                    Kind::Control(Control::Edit(e)) => {
                        let target = self.pointer.0 - rect[0] - 3 + e.scroll;
                        let mut best = 0;
                        let mut distance = i32::MAX;
                        for i in 0..=e.text.text.chars().count() {
                            let prefix = e.text.text.chars().take(i).collect::<String>();
                            let x = e.text.font.map_or(0, |f| {
                                TextLayout::compute(&self.res, f, &prefix, None, 0).width
                            });
                            if (x - target).abs() < distance {
                                distance = (x - target).abs();
                                best = i;
                            }
                        }
                        e.cursor = best;
                    }
                    Kind::Control(Control::Document(d)) => d.pressed_link = d.hovered_link,
                    _ => {
                        self.drag_control(id, self.pointer.0, self.pointer.1);
                    }
                }
            }
        } else {
            if let Some(id) = self.pressed.filter(|p| Some(*p) == self.hot) {
                event = self.activate(id).or(event);
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
            Kind::Control(Control::Slider(s)) => {
                let knob = s.sprites.get(3).and_then(|s| *s).map_or(12, |s| s.width);
                let f = ((x - rect[0] - s.inset - knob / 2) as f32
                    / (rect[2] - rect[0] - 2 * s.inset - knob).max(1) as f32)
                    .clamp(0.0, 1.0);
                let raw = s.range[0] + (s.range[1] - s.range[0]) * f;
                next = Some(raw);
            }
            Kind::Control(Control::Scroll { vertical, page, .. }) => {
                let axis = if *vertical { 1 } else { 0 };
                let p = if *vertical { y } else { x };
                let thumb = scroll_thumb(rect, *vertical, 0.0, *page);
                let len = thumb[axis + 2] - thumb[axis];
                next = Some(
                    (p - rect[axis] - len / 2) as f32
                        / (rect[axis + 2] - rect[axis] - len).max(1) as f32,
                );
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
        if !self.nodes[id].enabled || !self.nodes[id].visible() {
            return None;
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
            Kind::Control(Control::Edit(e)) if e.max_length > 0 => Some(Event::Submitted(id)),
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
                            selected_text = Some((*button, *list, i));
                            *open = false;
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
                }
            }
            if let Some(routed) = routed {
                return routed;
            }
        }
        event
    }

    pub fn wheel(&mut self, delta: i32) -> bool {
        let (x, y) = self.pointer;
        // Pick the innermost visible container whose viewport contains the pointer.
        let active = self
            .input_root
            .map(|r| self.active_nodes(r))
            .unwrap_or_default();
        let mut candidates: Vec<_> = active
            .into_iter()
            .filter_map(|id| {
                let n = &self.nodes[id];
                if let Kind::Control(Control::Container { view, .. }) = &n.kind {
                    if contains(*view, x, y) {
                        return Some((id, (view[2] - view[0]) * (view[3] - view[1])));
                    }
                }
                None
            })
            .collect();
        if let Some(h) = self.hot {
            let height = self.nodes[h].rect[3] - self.nodes[h].rect[1];
            if let Kind::List(l) = &mut self.nodes[h].kind {
                let max = (l.items.len() as i32 * l.row_height - height).max(0);
                l.scroll = (l.scroll - delta).clamp(0, max);
                return true;
            }
        }
        candidates.sort_by_key(|c| c.1);
        for (id, _) in candidates {
            let Kind::Control(Control::Container { content, view, .. }) =
                self.nodes[id].kind.clone()
            else {
                continue;
            };
            let max = content.map_or(0, |n| (self.nodes[n].size[1] - (view[3] - view[1])).max(0));
            if max > 0 {
                if let Kind::Control(Control::Container { offset, .. }) = &mut self.nodes[id].kind {
                    offset[1] = (offset[1] - delta).clamp(0, max);
                }
                return true;
            }
        }
        false
    }

    fn active_nodes(&self, root: NodeId) -> Vec<NodeId> {
        fn visit(ui: &Ui, id: NodeId, out: &mut Vec<NodeId>) {
            let n = &ui.nodes[id];
            if !n.visible() || !n.enabled {
                return;
            }
            out.push(id);
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
                    Key::Enter => {
                        if let Some(range) = e.range {
                            let value = e
                                .text
                                .text
                                .parse::<f32>()
                                .unwrap_or(0.0)
                                .clamp(range[0].min(range[1]), range[0].max(range[1]))
                                .trunc();
                            e.text.text = crate::controls::format_number(value);
                            e.cursor = e.text.text.chars().count();
                        }
                        self.pending.push(Event::Submitted(id));
                        self.focus = None;
                        return true;
                    }
                    Key::Escape => {
                        self.focus = None;
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
                let step = if s.step != 0.0 {
                    s.step
                } else {
                    (s.range[1] - s.range[0]) / 100.0
                };
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
        for ch in text.chars().filter(|c| !c.is_control()) {
            if e.text.text.chars().count() >= e.max_length {
                break;
            }
            let mut candidate = e.text.text.clone();
            let at = byte_index(&candidate, e.cursor);
            candidate.insert(at, ch);
            if e.numeric
                && (!valid_number_prefix(&candidate)
                    || (e.range.is_some() && candidate.contains(['.', 'e', 'E'])))
            {
                continue;
            }
            e.text.text = candidate;
            e.cursor += 1;
            changed = true;
        }
        if changed {
            self.pending.push(Event::Changed(id));
        }
        true
    }

    fn update_states(&mut self, _root: NodeId) {
        for id in 0..self.nodes.len() {
            let hot = self.hot == Some(id) || self.focus == Some(id);
            let pressed = self.pressed == Some(id) && self.pointer_down;
            let enabled = self.nodes[id].enabled;
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

    pub fn hovered_tooltip(&self) -> Option<&str> {
        self.hot
            .map(|h| self.nodes[h].tooltip.as_str())
            .filter(|t| !t.is_empty())
    }

    fn tooltip_source(&self, root: NodeId) -> Option<NodeId> {
        self.active_nodes(root).into_iter().rev().find(|&id| {
            self.nodes[id].contains(self.pointer.0, self.pointer.1)
                && !self.nodes[id].tooltip.is_empty()
        })
    }

    fn update_tooltip(&mut self, root: NodeId) {
        let Some(source) = self.tooltip_source(root) else {
            return;
        };
        if self.tooltip_instance.is_some_and(|(s, _)| s == source) {
            return;
        }
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

    /// Size and place the open tooltip dialog next to the pointer (again after the application
    /// filled it).
    pub fn place_tooltip(&mut self) {
        let Some((_, root)) = self.tooltip_instance else {
            return;
        };
        self.compute_size(root);
        let [w, h] = self.nodes[root].size;
        let x = (self.pointer.0 + 16).min(self.screen[0] - w).max(0);
        let y = (self.pointer.1 + 20).min(self.screen[1] - h).max(0);
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
    pub(crate) fn draw_tooltip(&self, root: NodeId, out: &mut Vec<Quad>) {
        if self.pointer_down {
            return;
        }
        if let Some((source, tip)) = self.tooltip_instance {
            if self.tooltip_source(root) == Some(source) && tip < self.nodes.len() {
                self.draw_node(tip, 0xFFFFFFFF, None, out);
                return;
            }
        }
        let node = self.active_nodes(root).into_iter().rev().find(|&id| {
            self.nodes[id].contains(self.pointer.0, self.pointer.1)
                && !self.nodes[id].tooltip.is_empty()
        });
        let Some(id) = node else {
            return;
        };
        let Some(font) = self.res.fonts.iter().position(|_| true) else {
            return;
        };
        let layout = TextLayout::compute(&self.res, font, &self.nodes[id].tooltip, Some(320), 0);
        let w = (layout.width + 16).min(self.screen[0]);
        let h = (layout.height + 12).min(self.screen[1]);
        let x = (self.pointer.0 + 16).min(self.screen[0] - w).max(0);
        let y = (self.pointer.1 + 20).min(self.screen[1] - h).max(0);
        if let Some((sheet, sp)) = self
            .res
            .sheets
            .iter()
            .enumerate()
            .find_map(|(i, s)| s.sheet.sprite("White").map(|s| (i, s)))
        {
            let s = SpriteRef {
                sheet,
                width: sp.width,
                height: sp.height,
                uv: sp.uv,
            };
            Self::sprite_quad(out, &s, [x, y, x + w, y + h], 0xF030281F, None);
        }
        let text = TextData {
            font: Some(font),
            text: String::new(),
            max_width: 320,
            justify: 0,
            wrap: true,
            shadow: false,
            ellipsis_width: 0,
            layout,
        };
        self.draw_text(&text, [x + 8, y + 6], 0xFFFFE8C0, None, out);
    }
}
fn contains(r: [i32; 4], x: i32, y: i32) -> bool {
    x >= r[0] && x < r[2] && y >= r[1] && y < r[3]
}
fn byte_index(s: &str, cursor: usize) -> usize {
    s.char_indices().nth(cursor).map_or(s.len(), |p| p.0)
}
fn valid_number_prefix(s: &str) -> bool {
    s.is_empty()
        || matches!(s, "-" | "+" | "." | "-." | "+.")
        || s.parse::<f32>().is_ok_and(|v| v.is_finite())
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

/// CRC32 type ids of the game's tooltip controllers handled by the UI itself.
pub mod tooltip_controller {
    pub const HOT_KEY: u32 = 0x5dd8_3c93;
    pub const TITLE_DESC: u32 = 0x33e6_1003;
    pub const BUILD_REQUIREMENT: u32 = 0xc3fe_f549;
}
