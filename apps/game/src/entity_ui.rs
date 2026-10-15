//! EntityUI hosts opened by EditTool::Select (FUN_140102880).
use crate::building_ui::{Controller, EntityData};
use crate::{loops::Screen, selection::Entity, start::StartingTown};
use banished_formats::{
    game::{self, UiController},
    pkg::Archive,
};
use banished_ui::{Control, Kind, NodeId, Ui};

pub struct EntityWindow {
    pub entity: Entity,
    pub root: NodeId,
    pub controllers: Vec<(UiController, NodeId)>,
    /// The attached controllers, in UIDescription order.
    pub bound: Vec<Controller>,
    pub pages: Option<NodeId>,
    /// The template's WorkPlaceDescription names a profession.
    pub workplace_profession: bool,
    pub pinned: bool,
}

#[derive(Default)]
pub struct EntityWindows {
    // Not ported: offscreen host retirement, dragging and persisted pin settings.
    pub open: Vec<EntityWindow>,
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
        self.open.retain(|w| {
            if !w.pinned && w.entity != entity {
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
        // EntityUI refresh FUN_1400676e0 uses UIDescription +0x40 for completed entities.
        if let Some(pages) = ui.find(root, "pages") {
            if let Kind::Control(Control::Page { selected, .. }) = &mut ui.node_mut(pages).kind {
                *selected = desc.flags.max(0) as usize;
            }
        }
        let mut controllers = Vec::new();
        let mut bound = Vec::new();
        for c in desc.controllers {
            let Some(slot) = ui.find(root, &c.slot) else {
                log::warn!("{template}: missing slot {}", c.slot);
                continue;
            };
            let Some(element) = c.element.as_deref().and_then(|r| ui.instantiate(r)) else {
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
                bound.push(Controller::bind(ui, archive, c.type_id, element));
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
        let workplace_profession = archive
            .read_ref(&format!("{template}:workplace"))
            .ok()
            .and_then(|b| game::workplace_description(&b).ok())
            .is_some_and(|w| w.profession.is_some());
        let pages = ui.find(root, "pages");
        self.open.push(EntityWindow {
            entity,
            root,
            controllers,
            bound,
            pages,
            workplace_profession,
            pinned: false,
        });
        self.refresh(ui, archive, town);
        log::debug!(
            "entity host has {} controllers",
            self.open.last().unwrap().controllers.len()
        );
    }

    /// Each controller's slot `+0x38` refresh with the entity's current data (read-only).
    pub fn refresh(&mut self, ui: &mut Ui, archive: &Archive, town: &StartingTown) {
        for w in &mut self.open {
            let active_page = w
                .pages
                .and_then(|p| match &ui.node(p).kind {
                    Kind::Control(Control::Page { selected, .. }) => Some(*selected),
                    _ => None,
                })
                .unwrap_or(0);
            let (storage, status) = match w.entity {
                Entity::Placed(p) => (
                    town.storage_sites
                        .iter()
                        .find(|s| s.entity == p)
                        .and_then(|s| town.storages.get(s.storage)),
                    // Not ported: building StatusIconComponent bits.
                    0,
                ),
                Entity::Citizen(i) => town
                    .agents
                    .states
                    .get(i)
                    .map_or((None, 0), |s| (Some(&s.carried), s.status)),
            };
            let data = EntityData {
                storage,
                materials: &town.materials,
                status,
                workplace_profession: w.workplace_profession,
                active_page,
            };
            for c in &mut w.bound {
                c.refresh(ui, archive, &data);
            }
        }
    }

    pub fn close_containing(
        &mut self,
        ui: &mut Ui,
        screen: &mut Screen,
        node: NodeId,
    ) -> Option<Entity> {
        let i = self.open.iter().position(|w| ui.descendant(w.root, node))?;
        let w = self.open.remove(i);
        screen.remove_root(ui, w.root);
        Some(w.entity)
    }

    /// EntityUI/ToolUI pinning changes only presentation state (ToolUI +0x40).
    pub fn pin(&mut self, ui: &Ui, node: NodeId) -> bool {
        let Some(w) = self.open.iter_mut().find(|w| ui.descendant(w.root, node)) else {
            return false;
        };
        w.pinned = !w.pinned;
        true
    }

    /// FUN_140102270: place the unpinned host above the projected object's rectangle.
    pub fn position(&self, ui: &mut Ui, entity: Entity, bounds: [f32; 4]) {
        for w in self.open.iter().filter(|w| w.entity == entity && !w.pinned) {
            let size = ui.node(w.root).size;
            let n = ui.node_mut(w.root);
            n.alignment = 0;
            n.margins = [
                (((bounds[0] + bounds[2]) * 0.5) as i32 - size[0] / 2).max(0),
                0,
                (bounds[1] as i32 - size[1]).max(0),
                0,
            ];
        }
    }
}
