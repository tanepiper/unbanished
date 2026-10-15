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
    /// UIDescription `+0x40`: the page of a finished entity.
    pub page: i32,
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
                bound.push(Controller::bind(
                    ui,
                    archive,
                    c.type_id,
                    element,
                    root,
                    c.config.is_some(),
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
        self.open.push(EntityWindow {
            entity,
            root,
            controllers,
            bound,
            pages,
            page: desc.flags,
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
            if let Some(pages) = w.pages {
                let active = match &ui.node(pages).kind {
                    Kind::Control(Control::Page { selected, .. }) => *selected as i32,
                    _ => 0,
                };
                let page = entity_page(town, w.entity, w.page, active);
                if let Kind::Control(Control::Page { selected, .. }) = &mut ui.node_mut(pages).kind
                {
                    *selected = page.max(0) as usize;
                }
            }
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
            let citizen = match w.entity {
                Entity::Citizen(i) => citizen_info(town, i),
                Entity::Placed(_) => None,
            };
            let data = EntityData {
                storage,
                materials: &town.materials,
                status,
                workplace: match w.entity {
                    Entity::Placed(p) => workplace_info(town, p),
                    Entity::Citizen(_) => None,
                },
                build: match w.entity {
                    Entity::Placed(p) => build_info(town, p),
                    Entity::Citizen(_) => None,
                },
                active_page,
                citizen,
            };
            for c in &mut w.bound {
                c.refresh(ui, archive, &data);
            }
        }
    }

    /// Route an element event to the controllers of the window containing `node`
    /// (`UI::DialogController` slot `+0x48`).
    pub fn click(
        &mut self,
        ui: &Ui,
        town: &StartingTown,
        node: NodeId,
    ) -> Option<crate::building_ui::WorkPlaceClick> {
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
        };
        w.bound.iter_mut().find_map(|c| match c {
            Controller::WorkPlace(c) => c.click(node, &data),
            Controller::Build(c) => c.click(node, &data),
            _ => None,
        })
    }

    /// `FUN_140108510` pins the window that asked (EntityUI `+0x38 +0xd8`).
    pub fn pin_containing(&mut self, ui: &Ui, node: NodeId) {
        if let Some(w) = self.open.iter_mut().find(|w| ui.descendant(w.root, node)) {
            w.pinned = true;
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

/// The values CitizenUI (`FUN_140065440`) shows for citizen `i`.
fn citizen_info(town: &StartingTown, i: usize) -> Option<crate::building_ui::CitizenInfo> {
    let c = town.citizens.get(i)?;
    let s = town.agents.states.get(i)?;
    let ai = town.agents.ais.get(i)?;
    let rules = town.agents.rules();
    let child = c.age < town.rules.adult_age || c.school.is_some();
    // Inferred from the resource names: ProfessionList `+0x40` child, `+0x48` student; adults
    // show their profession (`+0x1f0`).
    let key = if !child {
        "laborer"
    } else if c.school.is_some() {
        "student"
    } else {
        "child"
    };
    let assigned = (!child)
        .then(|| town.construction.professions.list.get(s.profession))
        .flatten()
        .map(|p| (p.string_table.clone(), p.name.clone()));
    let profession = assigned.or_else(|| {
        town.profession(key)
            .map(|p| (p.string_table.clone(), p.name.clone()))
    });
    let tool = if s.tool_uses < 1 {
        0
    } else {
        i32::from(s.tool_status != rules.tool_grade) + 1
    };
    // Activity 9 (working) shows 16 ("picking up resources") when the job's work
    // (`FUN_1400fef30`, by id `+0x30`) is for another profession (slot `+0x50`).
    let mut activity = ai.current as i32;
    if activity == 9 && s.job >= 0 {
        if let Some(work) = town.construction.jobs.works.get(&s.job) {
            if work.profession != s.profession {
                activity = 16;
            }
        }
    }
    Some(crate::building_ui::CitizenInfo {
        name: c.name.clone(),
        male: c.male,
        age: c.age,
        educated: c.education >= town.rules.education,
        clothing: s.clothing,
        tool,
        profession,
        health: s.health,
        health_max: rules.health_max,
        happiness: s.happiness,
        happiness_max: rules.happiness,
        activity,
        has_work: s.work.is_some() || c.school.is_some(),
    })
}

/// What WorkPlaceUI (`FUN_14008c550`) reads for building `p`; `None` without a profession.
fn workplace_info(town: &StartingTown, p: usize) -> Option<crate::building_ui::WorkPlaceInfo> {
    let c = &town.construction;
    let wp = c.workforce.workplaces.get(&p)?;
    let profession = wp.profession?;
    let desc = c.professions.list.get(profession)?;
    let (mut holders, mut laborers) = (0, 0);
    for (citizen, state) in town.citizens.iter().zip(&town.agents.states) {
        if citizen.alive && citizen.is_adult(&town.rules) && citizen.school.is_none() {
            holders += i32::from(state.profession == profession);
            laborers += i32::from(state.profession == c.professions.laborer);
        }
    }
    let site = c.sites.get(&p);
    let template = site.and_then(|s| c.templates.get(s.template));
    // Every site keeps its BuildComponent until it is built, and is the builder's until then.
    let build_points = (profession == c.professions.builder && site.is_some())
        .then(|| town.templates[town.placed[p].template].build_points.len() as i32);
    Some(crate::building_ui::WorkPlaceInfo {
        entity: p,
        sprite: (desc.sprite_sheet.clone(), desc.name.clone()),
        workers: wp.workers.clone(),
        max: wp.max(),
        holders,
        laborers,
        adjustable: template.is_some_and(|t| t.adjustable),
        min_jobs: template.map_or(0, |t| t.min_jobs),
        max_jobs: template.map_or(0, |t| t.max_jobs),
        build_points,
    })
}

/// EntityUI refresh `FUN_1400676e0`: the host page for the entity's state. Page 2 while it
/// is cleared or built, 3 in states 3..=7 (damaged or being destroyed), else the
/// UIDescription's page (`+0x40`), keeping page 4 once the player opened it.
/// Not ported: DestroyComponent (no demolition yet).
fn entity_page(town: &StartingTown, entity: Entity, finished: i32, active: i32) -> i32 {
    use banished_game::construction::Controller as C;
    let Entity::Placed(p) = entity else {
        return finished;
    };
    let c = &town.construction;
    let Some(site) = c.sites.get(&p) else {
        return finished;
    };
    let controllers = &c.templates[site.template].controllers;
    let m = &site.machine;
    let state = if m.pending < 0 { m.state } else { m.pending } as i32;
    if !controllers.contains(&C::Build) {
        return if !controllers.contains(&C::ClearArea) || state != 0 {
            finished
        } else {
            2
        };
    }
    if (3..8).contains(&state) {
        return 3;
    }
    let t = &c.templates[site.template];
    // `FUN_14010e860`: BuildDescription `+0x30` work × stages <= progress.
    if t.work * site.stages > site.progress {
        return 2;
    }
    if state != 0 {
        if active == 4 {
            4
        } else {
            finished
        }
    } else {
        2
    }
}

/// What BuildUI (`FUN_1400b6f20`) reads for building `p`; `None` unless it is a site.
fn build_info(town: &StartingTown, p: usize) -> Option<crate::building_ui::BuildInfo> {
    use banished_game::construction::Controller as C;
    let c = &town.construction;
    let site = c.sites.get(&p)?;
    let t = &c.templates[site.template];
    let storage = town.storages.get(site.storage);
    let build = t.controllers.contains(&C::Build).then(|| {
        let materials = t
            .materials
            .iter()
            .map(|&(count, m)| {
                // The first entry of that material.
                let have = storage
                    .and_then(|s| s.items.iter().find(|i| i.material == m))
                    .map_or(0, |i| i.count - i.reserved);
                (m, count * site.stages, have)
            })
            .collect();
        crate::building_ui::BuildProgress {
            materials,
            work: t.work * site.stages,
            progress: site.progress,
        }
    });
    let has_clear = t.controllers.contains(&C::ClearArea);
    Some(crate::building_ui::BuildInfo {
        entity: p,
        build,
        clear_total: if has_clear { site.clear_total } else { 0 },
        clear_remaining: if has_clear {
            banished_game::town::clear_remaining(
                &town.cells.objects,
                town.cells.size as i32,
                &town.placed[p],
                p,
            )
        } else {
            0
        },
        enabled: c.workforce.workplaces.get(&p).map(|w| w.enabled),
    })
}
