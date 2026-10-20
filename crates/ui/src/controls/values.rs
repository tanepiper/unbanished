//! Reading and setting the controls' values: sliders, check boxes, spinners, combos, tables, graphs
//! and images.

use super::*;

impl Ui {
    /// Set the value of a slider, scroll bar or progress bar.
    pub fn set_value(&mut self, id: NodeId, value: f32) {
        if !value.is_finite() {
            return;
        }
        let mut label = None;
        match &mut self.nodes[id].kind {
            Kind::Control(Control::Slider(s)) => {
                s.value = value.clamp(s.range[0].min(s.range[1]), s.range[0].max(s.range[1]));
                label = s.number.map(|n| (n, format_number(s.value)));
            }
            Kind::Control(
                Control::Progress { value: v, .. } | Control::Scroll { value: v, .. },
            ) => *v = value.clamp(0.0, 1.0),
            Kind::Control(Control::Edit(e)) => {
                let value = if let Some(range) = e.range {
                    value
                        .clamp(range[0].min(range[1]), range[0].max(range[1]))
                        .trunc()
                } else {
                    value
                };
                e.text.text = format_number(value);
                e.cursor = e.text.text.chars().count();
            }
            _ => {}
        }
        if let Some((n, text)) = label {
            self.set_text(n, &text);
        }
    }

    /// The value of a slider, scroll bar or progress bar.
    pub fn value(&self, id: NodeId) -> Option<f32> {
        match &self.nodes[id].kind {
            Kind::Control(Control::Slider(s)) => Some(s.value),
            Kind::Control(Control::Progress { value, .. } | Control::Scroll { value, .. }) => {
                Some(*value)
            }
            Kind::Control(Control::Edit(e)) => e.text.text.parse().ok(),
            _ => None,
        }
    }

    /// Set the smallest and the largest value of a slider.
    pub fn set_slider_range(&mut self, id: NodeId, range: [f32; 2]) {
        if let Kind::Control(Control::Slider(slider)) = &mut self.nodes[id].kind {
            slider.range = range;
            slider.value = slider
                .value
                .clamp(range[0].min(range[1]), range[0].max(range[1]));
        }
    }

    /// `UI::TableElement::SetChild(child, index)`: put `child` in cell `index` (row-major).
    pub fn set_table_child(&mut self, table: NodeId, index: usize, child: NodeId) {
        if let Kind::Control(Control::Table {
            cells,
            columns,
            rows,
            ..
        }) = &mut self.nodes[table].kind
        {
            let cols = columns.len().max(1);
            let (x, y) = (index % cols, index / cols);
            if y >= rows.len() {
                rows.resize(y + 1, 0);
            }
            cells.retain(|c| (c.0, c.1) != (x, y));
            cells.push((x, y, child));
        }
    }

    /// `UI::ImageElement::SetSprite` on every image inside `id`.
    pub fn set_images(&mut self, id: NodeId, sprite: SpriteRef) {
        let mut stack = vec![id];
        while let Some(n) = stack.pop() {
            if let Kind::Image { sprite: s, .. } = &mut self.nodes[n].kind {
                *s = Some(sprite);
            }
            stack.extend(self.child_ids(n));
        }
    }

    /// The range of the spinner's number edit, its current value clamped to it.
    ///
    /// Original: `SpinnerElement::SetMinMax`, Runtime `0x1800da5b0`.
    pub fn set_spinner_range(&mut self, id: NodeId, min: i32, max: i32) {
        let edit = match &self.nodes[id].kind {
            Kind::Control(Control::Spinner { parts, .. }) => parts[0],
            _ => None,
        };
        let Some(edit) = edit else { return };
        if let Kind::Control(Control::Edit(e)) = &mut self.nodes[edit].kind {
            e.range = Some([min as f32, max as f32]);
        }
        let value = match &self.nodes[id].kind {
            Kind::Control(Control::Spinner { value, .. }) => *value,
            _ => return,
        };
        let clamped = value.clamp(min.min(max), min.max(max));
        if clamped != value {
            if let Kind::Control(Control::Spinner { value, .. }) = &mut self.nodes[id].kind {
                *value = clamped;
            }
            self.set_text(edit, &clamped.to_string());
        }
    }

    // +0xf8; +0xfc
    /// The range of a spinner's number edit.
    pub fn spinner_range(&self, id: NodeId) -> Option<(i32, i32)> {
        let Kind::Control(Control::Spinner { parts, .. }) = &self.nodes[id].kind else {
            return None;
        };
        match &self.nodes[parts[0]?].kind {
            Kind::Control(Control::Edit(e)) => e
                .range
                .map(|r| (r[0].min(r[1]) as i32, r[0].max(r[1]) as i32)),
            _ => None,
        }
    }

    /// Whether a check control is checked.
    pub fn checked(&self, id: NodeId) -> bool {
        matches!(
            self.nodes[id].kind,
            Kind::Control(Control::Check { checked: true, .. })
        )
    }

    /// Check or clear a check box.
    pub fn set_checked(&mut self, id: NodeId, value: bool) {
        if let Kind::Control(Control::Check { checked, .. }) = &mut self.nodes[id].kind {
            *checked = value;
        }
    }

    /// Set the choices of a combo box.
    pub fn set_choices(&mut self, id: NodeId, choices: Vec<String>) {
        self.set_choices_with_icons(id, choices, Vec::new());
    }

    /// `ComboElement::AddItem` for each item, its text and sprite.
    pub fn set_choices_with_icons(
        &mut self,
        id: NodeId,
        choices: Vec<String>,
        icons: Vec<Option<SpriteRef>>,
    ) {
        if let Kind::Control(Control::Combo { list, .. }) = self.nodes[id].kind {
            self.set_list_icons(list, choices, icons);
        }
    }

    /// `GraphElement::SetYRange`, `SetXDivisions`, `RemoveGraphs` then `AddGraph` and
    /// `AddGraphPoint` for each (colour, points).
    pub fn set_graph(
        &mut self,
        id: NodeId,
        y_range: f32,
        x_divisions: i32,
        series: Vec<(u32, Vec<i32>)>,
    ) {
        if let Kind::Control(Control::Graph {
            series: s,
            y_range: y,
            x_divisions: x,
            ..
        }) = &mut self.nodes[id].kind
        {
            *s = series;
            *y = y_range;
            *x = x_divisions;
        }
    }

    /// `LabelElement::SetSpriteColor`.
    pub fn set_label_sprite_color(&mut self, id: NodeId, color: u32) {
        if let Kind::Label { icon_color, .. } = &mut self.nodes[id].kind {
            *icon_color = color;
        }
    }
}
