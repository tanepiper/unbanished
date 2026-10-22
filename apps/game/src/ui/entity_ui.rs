//! EntityUI hosts opened by EditTool::Select (FUN_140102880).
mod info;
mod refresh;
mod state;
mod windows;
pub(crate) use info::citizen_info;
use info::{build_info, destroy_info, immigration_info, produce_info, residents, workplace_info};
use state::{cemetery_counts, control_state, entity_page, tracker_lists};

use crate::ui::building_ui::{Controller, EntityData};
use crate::{loops::Screen, start::StartingTown, tools::selection::Entity};
use unbanished_formats::{
    game::{self, UiController},
    pkg::Archive,
};
use unbanished_ui::{Control, Kind, NodeId, Ui};

pub struct EntityWindow {
    pub entity: Entity,
    pub root: NodeId,
    pub controllers: Vec<(UiController, NodeId)>,
    /// The attached controllers, in UIDescription order.
    pub bound: Vec<Controller>,
    pub pages: Option<NodeId>,
    /// UIDescription `+0x40`: the page of a finished entity.
    pub page: i32,
    /// ToolUI `+0x40`: pinned (by `checkPin` or by dragging it), it stays where it is.
    pub pinned: bool,
    /// Where a pinned window was dragged to (the tool manager's `+0x60` map, `FUN_140108300`).
    pub at: Option<(i32, i32)>,
    /// The bound controllers' type ids, and the size of the entity's settings buffer
    /// (UIDescription `+0x20`; 0, none).
    pub ids: Vec<u32>,
    pub settings_size: usize,
}

impl EntityWindow {
    /// `DialogInstance::SaveSettings` (`crate::ui::ui_settings`): what the window keeps, if its
    /// entity has a buffer.
    fn settings(&self, ui: &Ui) -> Option<(usize, Vec<u8>)> {
        let Entity::Placed(e) = self.entity else {
            return None;
        };
        if self.settings_size == 0 {
            return None;
        }
        let controllers: Vec<(u32, Vec<u8>)> = self
            .ids
            .iter()
            .zip(&self.bound)
            .map(|(&id, c)| (id, c.settings(ui)))
            .collect();
        Some((
            e,
            crate::ui::ui_settings::write(self.settings_size, &controllers),
        ))
    }
}

#[derive(Default)]
pub struct EntityWindows {
    /// The open hosts. An unpinned one closes when its entity leaves the screen
    /// (`FUN_140102160`); dragging its title pins it (`ToolUI`). The pinned windows' positions
    /// (the tool manager's `+0x60` map) go with the save.
    pub open: Vec<EntityWindow>,
    /// Settings written as windows closed (`OnClosed`), for their entities' UIComponents.
    pub saved: Vec<(usize, Vec<u8>)>,
}

impl EntityWindows {
    /// FUN_140102880 reuses an existing entity dialog and closes the preceding unpinned one.
    pub fn select(
        &mut self,
        ui: &mut Ui,
        screen: &mut Screen,
        archive: &Archive,
        town: &StartingTown,
        entity: Entity,
    ) {
        let existing = self.open.iter().any(|w| w.entity == entity);
        let saved = &mut self.saved;
        self.open.retain(|w| {
            if !w.pinned && w.entity != entity {
                saved.extend(w.settings(ui));
                screen.remove_root(ui, w.root);
                false
            } else {
                true
            }
        });
        if existing {
            return;
        }
        let template = entity
            .template(town)
            .split(':')
            .next()
            .unwrap_or("")
            .replace('\\', "/");
        let Some(desc) = archive
            .read_ref(&format!("{template}:ui"))
            .ok()
            .and_then(|b| game::ui_description(&b).ok())
        else {
            log::info!("{template}: selected entity has no UIDescription");
            return;
        };
        let Some(dialog) = desc
            .dialog
            .as_deref()
            .and_then(|host| screen.add_dialog(ui, host))
        else {
            return;
        };
        let root = screen.dialogs[dialog].root;
        let mut controllers = Vec::new();
        let mut bound = Vec::new();
        let mut ids = Vec::new();
        for c in desc.controllers {
            let Some(slot) = ui.find(root, &c.slot) else {
                log::warn!("{template}: missing slot {}", c.slot);
                continue;
            };
            // `FUN_1401335d0` attaches every controller (`DialogInstance::AddController`);
            // one without an element (RadiusUI) works on the dialog itself.
            let Some(element) = c.element.as_deref().and_then(|r| ui.instantiate(r)) else {
                if c.type_id != 0 {
                    ids.push(c.type_id);
                    bound.push(Controller::bind(
                        ui,
                        archive,
                        c.type_id,
                        root,
                        root,
                        c.config.as_deref(),
                    ));
                    controllers.push((c, root));
                }
                continue;
            };
            if matches!(&ui.node(slot).kind, Kind::Control(Control::Tabs { .. })) {
                if let Some(header) = c.tab.as_deref().and_then(|r| ui.instantiate(r)) {
                    ui.add_tab(slot, header, element);
                } else {
                    log::warn!("{template}: tab {} has no label", c.slot);
                }
            } else {
                ui.add_child(slot, element);
            }
            if c.type_id != 0 {
                ids.push(c.type_id);
                bound.push(Controller::bind(
                    ui,
                    archive,
                    c.type_id,
                    element,
                    root,
                    c.config.as_deref(),
                ));
            }
            controllers.push((c, element));
        }
        let title = match entity {
            Entity::Citizen(i) => town.citizens[i].name.clone(),
            Entity::Placed(_) => archive
                .read_ref(&format!("{template}:toolbar"))
                .ok()
                .and_then(|b| game::toolbar_description(&b).ok())
                .map(|t| ui.res.string(t.string_table.as_deref(), &t.name))
                .unwrap_or_default(),
        };
        if let Some(label) = ui.find(root, "labelTitle") {
            ui.set_text(label, &title);
        }
        let pages = ui.find(root, "pages");
        // `OnOpened`: each saved controller's settings to the first of its type.
        if let Entity::Placed(e) = entity {
            let data = town
                .construction
                .sites
                .get(&e)
                .map(|s| s.ui_settings.as_slice());
            if let Some(saved) = data.and_then(crate::ui::ui_settings::read) {
                for (id, settings) in saved {
                    if let Some(i) = ids.iter().position(|&t| t == id) {
                        bound[i].apply_settings(ui, settings);
                    }
                }
            }
        }
        let settings_size = desc.value.max(0) as usize;
        self.open.push(EntityWindow {
            entity,
            root,
            controllers,
            bound,
            pages,
            page: desc.flags,
            pinned: false,
            at: None,
            ids,
            settings_size,
        });
        self.refresh(ui, archive, town);
        log::debug!(
            "entity host has {} controllers",
            self.open.last().unwrap().controllers.len()
        );
    }

    /// Route an element event to the controllers of the window containing `node`
    /// (`UI::DialogController` slot `+0x48`).
    pub fn click(
        &mut self,
        ui: &Ui,
        town: &StartingTown,
        node: NodeId,
    ) -> Option<crate::ui::building_ui::WorkPlaceClick> {
        let w = self.open.iter_mut().find(|w| ui.descendant(w.root, node))?;
        let Entity::Placed(p) = w.entity else {
            return None;
        };
        let data = EntityData {
            storage: None,
            materials: &town.materials,
            status: 0,
            workplace: workplace_info(town, p),
            build: build_info(town, p),
            active_page: 0,
            citizen: None,
            destroy: destroy_info(town, p),
            stock_limits: [0; 21],
            control: None,
            tracker: None,
            attendance: None,
            cemetery: None,
            produce: produce_info(town, p),
            immigration: None,
            town_name: "",
            residents: residents(town, p),
            town: Some(town),
            town_ctx: None,
            entity: Some(p),
        };
        w.bound.iter_mut().find_map(|c| match c {
            Controller::WorkPlace(c) => c.click(node, &data),
            Controller::Build(c) => c.click(node, &data),
            Controller::Destroy(c) => c.click(node, &data),
            Controller::ResourceLimit(c) => c.click(ui, node),
            Controller::Control(c) => c.click(ui, node, Some(p)),
            Controller::Upgrade(c) => c.click(ui, node, p),
            Controller::Immigration(c) => c.click(node, p),
            Controller::Residence(c) => c.click(ui, node, &data),
            Controller::TownHall(c) => c.click(ui, town, node),
            Controller::Trade(c) => c.click(ui, node, p),
            Controller::SelectResource(c) => c.click(ui, node, p),
            _ => None,
        })
    }

    /// The buildings whose window has a RadiusUI (`0x00585b87`): while open, its work radius
    /// shows (`FUN_14007cb20`; closing removes it, `FUN_14007cad0`).
    pub fn radius_windows(&self) -> Vec<usize> {
        self.open
            .iter()
            .filter(|w| {
                w.controllers
                    .iter()
                    .any(|(c, _)| c.type_id == crate::ui::building_ui::RADIUS_UI)
            })
            .filter_map(|w| match w.entity {
                Entity::Placed(p) => Some(p),
                Entity::Citizen(_) => None,
            })
            .collect()
    }

    /// The buildings whose window has a TradeUI.
    pub fn trade_windows(&self) -> Vec<usize> {
        self.open
            .iter()
            .filter(|w| w.bound.iter().any(|c| matches!(c, Controller::Trade(_))))
            .filter_map(|w| match w.entity {
                Entity::Placed(p) => Some(p),
                Entity::Citizen(_) => None,
            })
            .collect()
    }

    /// A combo's choice (event `0x14`) in the window containing `node`.
    pub fn select_item(
        &mut self,
        ui: &Ui,
        town: &StartingTown,
        node: NodeId,
        index: usize,
    ) -> Option<crate::ui::building_ui::WorkPlaceClick> {
        let w = self.open.iter_mut().find(|w| ui.descendant(w.root, node))?;
        let Entity::Placed(p) = w.entity else {
            return None;
        };
        let data = EntityData {
            storage: None,
            materials: &town.materials,
            status: 0,
            workplace: None,
            build: None,
            active_page: 0,
            citizen: None,
            destroy: None,
            stock_limits: [0; 21],
            control: None,
            tracker: None,
            attendance: None,
            cemetery: None,
            produce: produce_info(town, p),
            immigration: None,
            town_name: "",
            residents: residents(town, p),
            town: Some(town),
            town_ctx: None,
            entity: Some(p),
        };
        w.bound.iter().find_map(|c| match c {
            Controller::ConsumeProduce(c) => c.select(node, index, &data),
            Controller::Trade(c) => c.select(node, index, p),
            Controller::SelectResource(c) => c.select(node, index, p),
            _ => None,
        })
    }
}
