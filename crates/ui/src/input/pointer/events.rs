//! Dragging controls, scrolling a bar's parent, activating a control and routing events up the tree.

use super::*;

impl Ui {
    pub(in crate::input) fn drag_control(&mut self, id: NodeId, x: i32, y: i32) {
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

    pub(in crate::input) fn scroll_parent(&mut self, bar: NodeId) {
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

    pub(in crate::input) fn activate(&mut self, id: NodeId) -> Option<Event> {
        if !self.nodes[id].usable() || !self.nodes[id].visible() {
            return None;
        }
        if self.nodes[id].flags & unbanished_formats::ui::flags::MOUSE != 0 {
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

    pub(in crate::input) fn route_event(&mut self, root: NodeId, event: Event) -> Event {
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
}
