//! Production: consumed and produced goods, counts and the production table.

use super::*;

/// `ConsumeProduceUI` (type `0x9c93d6c1`): bind `FUN_14005fd30`, refresh `FUN_14005fb50`,
/// events `FUN_14005fc90`. `comboProduct` (id 22000) lists the recipes by their labels and
/// shows the selected one; `progressRemaining` the share of the lifetime left.
pub struct ConsumeProduceUi {
    combo: Option<NodeId>,
    remaining: Option<(NodeId, Option<(NodeId, String)>)>,
    filled: bool,
}

/// A progress bar out of `max` with its text's `@0`, `@1` parameters: AttendanceUI
/// (`FUN_14005e6a0`, `FUN_14005e5b0`: attendees of the places) and CemeteryUI
/// (`FUN_14005f9f0`, `FUN_14005f8b0`: graves taken, of all).
pub struct CountUi {
    progress: Option<NodeId>,
    text: Option<(NodeId, String)>,
    cemetery: bool,
}

/// `ProductionUI` (type `0x3998642a`): bind `FUN_14007b270`, refresh `FUN_14007aef0` →
/// `FUN_14007af20` for this year and last. The `production` (or `production3`,
/// `production4`) element's two children are tables whose rows (from the second) hold a
/// `labelStorage` and a `numberCount`.
pub struct ProductionUi {
    rows: [Vec<(Option<NodeId>, Option<NodeId>)>; 2],
}

impl ConsumeProduceUi {
    pub fn bind(ui: &mut Ui, root: NodeId) -> ConsumeProduceUi {
        let remaining = ui.find(root, "progressRemaining").map(|p| {
            let content = match &ui.node(p).kind {
                Kind::Control(Control::Progress { content, .. }) => *content,
                _ => None,
            };
            (p, content.map(|c| (c, text_of(ui, c))))
        });
        ConsumeProduceUi {
            combo: ui.find(root, "comboProduct"),
            remaining,
            filled: false,
        }
    }

    pub fn refresh(&mut self, ui: &mut Ui, archive: &Archive, data: &EntityData) {
        let Some((_, reference, selector, produced, lifetime)) = &data.produce else {
            return;
        };
        if !self.filled {
            // The bind's entity half: the recipes' labels, the current one (a negative
            // selector shows the first: the port's windows draw nothing from the game's
            // Random, where `FUN_14011e940` would pick one by the weights).
            self.filled = true;
            if let Some(c) = self.combo {
                let file = material_file(reference);
                if let Some(d) = archive
                    .read_ref(&format!("{file}:consumeproduce"))
                    .ok()
                    .and_then(|b| banished_formats::game::consume_produce_description(&b).ok())
                {
                    let labels = d
                        .recipes
                        .iter()
                        .map(|r| ui.res.string(d.string_table.as_deref(), &r.label))
                        .collect();
                    // `FUN_14005fd30`: each item's sprite is its output's small one
                    // (RawMaterialDescription sheet `+0x20`, sprite `+0x38`).
                    let icons = d
                        .recipes
                        .iter()
                        .map(|r| {
                            let file = r.output.as_deref()?.split(':').next()?.replace('\\', "/");
                            let m = banished_formats::game::raw_material_description(
                                &archive.read_ref(&format!("{file}:rawmaterial")).ok()?,
                            )
                            .ok()?;
                            ui.res.sprite(m.sprite_sheet.as_deref(), &m.name_small)
                        })
                        .collect();
                    ui.set_choices_with_icons(c, labels, icons);
                    crate::menu::select_combo(ui, c, (*selector).max(0) as usize);
                }
            }
        }
        if let Some((p, text)) = &self.remaining {
            let fill = 1.0 - *produced as f32 / *lifetime as f32;
            ui.set_value(*p, fill);
            if let Some((t, template)) = text {
                let percent = (fill * 100.0 + 0.5) as i32;
                ui.set_text(*t, &template.replace("@0", &percent.to_string()));
            }
        }
    }

    /// `FUN_14005fc90` (event `0x14`, id 22000): the selector is the chosen recipe.
    pub fn select(&self, node: NodeId, index: usize, data: &EntityData) -> Option<WorkPlaceClick> {
        if Some(node) != self.combo {
            return None;
        }
        let entity = data.produce.as_ref()?.0;
        Some(WorkPlaceClick::Command(
            banished_game::work::professions::Command::SetRecipe {
                entity,
                recipe: index as i32,
            },
        ))
    }
}

impl CountUi {
    pub fn bind(ui: &mut Ui, element: NodeId, cemetery: bool) -> CountUi {
        let mut stack = vec![element];
        let mut progress = None;
        while let Some(n) = stack.pop() {
            if matches!(ui.node(n).kind, Kind::Control(Control::Progress { .. })) {
                progress = Some(n);
                break;
            }
            stack.extend(ui.children(n).into_iter().rev());
        }
        let text = progress.and_then(|p| match &ui.node(p).kind {
            Kind::Control(Control::Progress { content, .. }) => {
                content.map(|c| (c, text_of(ui, c)))
            }
            _ => None,
        });
        CountUi {
            progress,
            text,
            cemetery,
        }
    }

    pub fn refresh(&mut self, ui: &mut Ui, data: &EntityData) {
        let counts = if self.cemetery {
            data.cemetery
        } else {
            data.attendance
        };
        let Some((n, max)) = counts else { return };
        if let Some(p) = self.progress {
            ui.set_value(p, if 0 < max { n as f32 / max as f32 } else { 0.0 });
        }
        if let Some((t, template)) = &self.text {
            let text = template
                .replace("@0", &n.to_string())
                .replace("@1", &max.to_string());
            ui.set_text(*t, &text);
        }
    }
}

impl ProductionUi {
    pub fn bind(ui: &mut Ui, element: NodeId) -> ProductionUi {
        let table = ["production", "production3", "production4"]
            .iter()
            .find_map(|n| ui.find(element, n));
        let mut rows: [Vec<(Option<NodeId>, Option<NodeId>)>; 2] = [Vec::new(), Vec::new()];
        if let Some(t) = table {
            let years = ui.children(t);
            for (k, &y) in years.iter().take(2).enumerate() {
                for &row in ui.children(y).iter().skip(1) {
                    rows[k].push((ui.find(row, "labelStorage"), ui.find(row, "numberCount")));
                }
            }
        }
        ProductionUi { rows }
    }

    /// `FUN_14007af20`: each listed material's sprite, name and amount; the rows past the
    /// list hide.
    pub fn refresh(&mut self, ui: &mut Ui, archive: &Archive, data: &EntityData) {
        let Some(lists) = &data.tracker else { return };
        for (rows, list) in self.rows.iter().zip(lists) {
            for (i, &(label, number)) in rows.iter().enumerate() {
                let Some(&(m, amount)) = list.get(i) else {
                    for n in [label, number].into_iter().flatten() {
                        ui.set_visible(n, false);
                    }
                    continue;
                };
                let d = archive
                    .read_ref(&format!(
                        "{}:rawmaterial",
                        material_file(&data.materials[m].reference)
                    ))
                    .ok()
                    .and_then(|b| raw_material_description(&b).ok());
                if let (Some(l), Some(d)) = (label, d) {
                    if let Some(s) = ui.res.sprite(d.sprite_sheet.as_deref(), &d.name) {
                        if let Kind::Label { icon, .. } = &mut ui.node_mut(l).kind {
                            *icon = Some(s);
                        }
                    }
                    let name = ui.res.string(d.string_table.as_deref(), &d.name);
                    ui.set_text(l, &name);
                    ui.set_visible(l, true);
                }
                if let Some(n) = number {
                    ui.set_text(n, &amount.to_string());
                    ui.set_visible(n, true);
                }
            }
        }
    }
}
