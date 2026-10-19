//! `SelectResourceUI` and its classes: CropUI (type `0x111542c0`, vtable `0x140165680`),
//! OrchardUI (`0x33908208`, `0x140163360`) and PastureUI (`0xae1a9e32`, `0x140165a20`). See
//! docs/BUILDING-WINDOWS.md.
//!
//! The host's `selectResource` combo (id 500, `FUN_14007e0b0`) lists the known crops, trees or
//! livestock (stock manager `+0x120`) whose SelectDescription `+0x40` is the class's kind
//! (slot `+0x68`: 0, 1, 2), each its name, and shows the current one (slot `+0x60`)
//! (`FUN_14007dd00`); it is refilled when the count changes (slot `+0x40`, `FUN_14007e200`)
//! and enabled with anything in it (`FUN_14007e180`). A choice (`FUN_14007e250`) calls slot
//! `+0x58`: the field's crop, the orchard's tree or the pasture's kind, which the simulation
//! applies (`Command::SetCrop`).
//!
//! CropUI and OrchardUI show the mean ripeness in `yield` (`FUN_1401208e0`, `FUN_140051fa0`);
//! PastureUI's `slider` (id `0x1f5`) the most animals to keep, up to the pasture's room
//! (`FUN_1400ba470`), its text the animals and the most (`FUN_1400ba320`); moved, the most
//! (`FUN_1400ba2f0`; the kind combo's `0x14` on id 500 → `FUN_14007dc80`, the chosen kind;
//! `Command::SetLimit`). The kind's list is enabled only while the pasture is
//! empty.

use banished_formats::pkg::Archive;
use banished_game::work::professions::Command;
use banished_ui::{Control, Kind, NodeId, Ui};

use crate::start::StartingTown;
use crate::ui::building_ui::{text_of, WorkPlaceClick};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resource {
    Crop,
    Orchard,
    Pasture,
}

impl Resource {
    /// Slot `+0x68` (`FUN_1400b7e90`, `FUN_14007a8f0`, `FUN_1400ba1c0`).
    fn kind(self) -> i32 {
        match self {
            Resource::Crop => 0,
            Resource::Orchard => 1,
            Resource::Pasture => 2,
        }
    }
}

pub struct SelectResourceUi {
    resource: Resource,
    combo: Option<NodeId>,
    /// The listed templates and the known list they came from.
    items: Vec<usize>,
    filled: Option<Vec<usize>>,
    /// `yield` and its text.
    progress: Option<(NodeId, Option<(NodeId, String)>)>,
    /// `slider` and its number's text.
    slider: Option<(NodeId, Option<(NodeId, String)>)>,
}

/// SelectDescription (`FUN_140055f90`): `+0x30`, `+0x38` the name's string table and key,
/// `+0x40` the kind.
/// SelectDescription: its icon (sheet `+0x20`, sprite `+0x28`), its name (table `+0x30`,
/// key `+0x38`) and kind (`+0x40`).
struct Select {
    sheet: Option<String>,
    sprite: String,
    table: Option<String>,
    name: String,
    kind: i32,
}

fn select_description(data: &[u8]) -> Option<Select> {
    use banished_formats::crs;
    crs::parse(data, |r: &mut crs::Reader, _| {
        r.block(|r, _| {
            let sheet = r.external_ref()?;
            let sprite = r.string()?;
            let table = r.external_ref()?;
            let name = r.string()?;
            let kind = r.i32()?;
            r.skip_rest();
            Ok(Select {
                sheet,
                sprite,
                table,
                name,
                kind,
            })
        })
    })
    .ok()
    .map(|(_, v)| v)
}

impl SelectResourceUi {
    pub fn bind(ui: &mut Ui, resource: Resource, element: NodeId, root: NodeId) -> Self {
        let with_text = |ui: &Ui, n: Option<NodeId>| {
            n.map(|p| {
                let content = match &ui.node(p).kind {
                    Kind::Control(Control::Progress { content, .. }) => *content,
                    Kind::Control(Control::Slider(s)) => s.number,
                    _ => None,
                };
                (p, content.map(|c| (c, text_of(ui, c))))
            })
        };
        let progress = match resource {
            Resource::Pasture => None,
            _ => with_text(ui, ui.find(element, "yield")),
        };
        let slider = match resource {
            Resource::Pasture => with_text(ui, ui.find(element, "slider")),
            _ => None,
        };
        SelectResourceUi {
            resource,
            combo: ui.find(root, "selectResource"),
            items: Vec::new(),
            filled: None,
            progress,
            slider,
        }
    }

    /// Slot `+0x60`: the current crop, tree or kind.
    fn current(&self, town: &StartingTown, e: usize) -> Option<usize> {
        let site = town.construction.sites.get(&e);
        match self.resource {
            Resource::Crop => site?.crop_field.as_ref()?.crop,
            Resource::Orchard => site?.orchard.as_ref()?.tree,
            Resource::Pasture => town.wildlife.pastures.get(&e)?.kind,
        }
    }

    pub fn refresh(&mut self, ui: &mut Ui, archive: &Archive, town: &StartingTown, e: usize) {
        let known = &town.construction.known;
        if self.filled.as_ref() != Some(known) {
            self.filled = Some(known.clone());
            let mut items = Vec::new();
            let mut labels = Vec::new();
            let mut icons = Vec::new();
            for &t in known {
                let file = town.templates[t]
                    .reference
                    .split(':')
                    .next()
                    .unwrap_or("")
                    .replace('\\', "/");
                let Some(d) = archive
                    .read_ref(&format!("{file}:select"))
                    .ok()
                    .and_then(|d| select_description(&d))
                else {
                    continue;
                };
                if d.kind == self.resource.kind() {
                    items.push(t);
                    labels.push(ui.res.string(d.table.as_deref(), &d.name));
                    icons.push(ui.res.sprite(d.sheet.as_deref(), &d.sprite));
                }
            }
            // `FUN_14007dd00`: `ComboElement::AddItem` with the icon and the name.
            if let Some(c) = self.combo {
                ui.set_choices_with_icons(c, labels, icons);
            }
            self.items = items;
            self.select_current(ui, town, e);
        }
        let empty = town
            .wildlife
            .pastures
            .get(&e)
            .is_none_or(|p| p.livestock.is_empty());
        if let Some(c) = self.combo {
            let on = !self.items.is_empty() && (self.resource != Resource::Pasture || empty);
            ui.node_mut(c).enabled = on;
        }
        if let Some((p, text)) = &self.progress {
            let site = town.construction.sites.get(&e);
            let fill = match self.resource {
                Resource::Crop => site.and_then(|s| s.crop_field.as_ref()).map_or(0.0, |f| {
                    banished_game::production::farm::ripeness(f, &town.placed)
                }),
                _ => site.and_then(|s| s.orchard.as_ref()).map_or(0.0, |o| {
                    banished_game::production::orchard::ripeness(
                        o,
                        &town.cells.objects,
                        &town.construction.fruits,
                    )
                }),
            };
            ui.set_value(*p, fill);
            if let Some((t, template)) = text {
                let percent = (fill * 100.0 + 0.5) as i32;
                ui.set_text(*t, &template.replace("@0", &percent.to_string()));
            }
        }
        if let (Some((s, text)), Some(p)) = (&self.slider, town.wildlife.pastures.get(&e)) {
            let room = town.wildlife.capacity(e);
            let focused = ui
                .focused()
                .is_some_and(|f| f == *s || ui.descendant(*s, f));
            if !focused {
                ui.set_slider_range(*s, [0.0, room as f32]);
                ui.set_value(*s, p.limit as f32);
            }
            if let Some((t, template)) = text {
                let text = template
                    .replace("@0", &p.livestock.len().to_string())
                    .replace("@1", &p.limit.to_string());
                ui.set_text(*t, &text);
            }
        }
    }

    /// `FUN_14007dd00`'s `SetCurrentItem`: the current one, when listed.
    fn select_current(&self, ui: &mut Ui, town: &StartingTown, e: usize) {
        let (Some(c), Some(cur)) = (self.combo, self.current(town, e)) else {
            return;
        };
        if let Some(i) = self.items.iter().position(|&t| t == cur) {
            crate::menu::select_combo(ui, c, i);
        }
    }

    /// The combo's choice (event `0x14`, id 500).
    pub fn select(&self, node: NodeId, index: usize, e: usize) -> Option<WorkPlaceClick> {
        if self.combo != Some(node) {
            return None;
        }
        let crop = *self.items.get(index)?;
        Some(WorkPlaceClick::Command(Command::SetCrop {
            entity: e,
            crop: Some(crop),
        }))
    }

    /// The slider moved (event `0x17`, id `0x1f5`).
    pub fn click(&self, ui: &Ui, node: NodeId, e: usize) -> Option<WorkPlaceClick> {
        let (s, _) = self.slider.as_ref()?;
        if node != *s && !ui.descendant(*s, node) {
            return None;
        }
        let limit = ui.value(*s)? as i32;
        Some(WorkPlaceClick::Command(Command::SetLimit {
            entity: e,
            limit,
        }))
    }
}
