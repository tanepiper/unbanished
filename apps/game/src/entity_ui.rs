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
    /// ToolUI `+0x40`: pinned (by `checkPin` or by dragging it), it stays where it is.
    pub pinned: bool,
    /// Where a pinned window was dragged to (the tool manager's `+0x60` map, `FUN_140108300`).
    pub at: Option<(i32, i32)>,
}

#[derive(Default)]
pub struct EntityWindows {
    /// The open hosts. An unpinned one closes when its entity leaves the screen
    /// (`FUN_140102160`); dragging its title pins it (`ToolUI`). The pinned windows' positions
    /// (the tool manager's `+0x60` map) go with the save.
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
            // `FUN_1401335d0` attaches every controller (`DialogInstance::AddController`);
            // one without an element (RadiusUI) works on the dialog itself.
            let Some(element) = c.element.as_deref().and_then(|r| ui.instantiate(r)) else {
                if c.type_id != 0 {
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
        self.open.push(EntityWindow {
            entity,
            root,
            controllers,
            bound,
            pages,
            page: desc.flags,
            pinned: false,
            at: None,
        });
        self.refresh(ui, archive, town);
        log::debug!(
            "entity host has {} controllers",
            self.open.last().unwrap().controllers.len()
        );
    }

    /// Each controller's slot `+0x38` refresh with the entity's current data (read-only).
    pub fn refresh(&mut self, ui: &mut Ui, archive: &Archive, town: &StartingTown) {
        self.refresh_with(ui, archive, town, &[0; 21], "", None);
    }

    /// [`Self::refresh`] with the stock manager's limits (ResourceLimitUI).
    pub fn refresh_with(
        &mut self,
        ui: &mut Ui,
        archive: &Archive,
        town: &StartingTown,
        stock_limits: &[i32; 21],
        town_name: &str,
        town_ctx: Option<&crate::town_hall_ui::TownContext>,
    ) {
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
                    // The building's StatusIconComponent bits: its workplace's and home's.
                    town.construction
                        .workforce
                        .workplaces
                        .get(&p)
                        .map_or(0, |w| w.status)
                        | town.construction.homes.get(&p).map_or(0, |h| h.status),
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
                destroy: match w.entity {
                    Entity::Placed(p) => destroy_info(town, p),
                    Entity::Citizen(_) => None,
                },
                stock_limits: *stock_limits,
                control: None,
                tracker: match w.entity {
                    Entity::Placed(p) => tracker_lists(town, p),
                    Entity::Citizen(_) => None,
                },
                attendance: match w.entity {
                    Entity::Placed(p) => town
                        .construction
                        .attendance
                        .get(&p)
                        .map(|a| (a.attendees.len() as i32, a.capacity)),
                    Entity::Citizen(_) => None,
                },
                cemetery: match w.entity {
                    Entity::Placed(p) => cemetery_counts(town, p),
                    Entity::Citizen(_) => None,
                },
                produce: match w.entity {
                    Entity::Placed(p) => produce_info(town, p),
                    Entity::Citizen(_) => None,
                },
                immigration: match w.entity {
                    Entity::Placed(p) => immigration_info(town, p),
                    Entity::Citizen(_) => None,
                },
                town_name,
                residents: match w.entity {
                    Entity::Placed(p) => residents(town, p),
                    Entity::Citizen(_) => None,
                },
                town: Some(town),
                town_ctx,
                entity: match w.entity {
                    Entity::Placed(p) => Some(p),
                    Entity::Citizen(_) => None,
                },
            };
            let entity = match w.entity {
                Entity::Placed(p) => Some(p),
                Entity::Citizen(_) => None,
            };
            let control =
                |kind: u32, index: i32| entity.map_or(0, |e| control_state(town, e, kind, index));
            let data = EntityData {
                control: Some(&control),
                ..data
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
                    .any(|(c, _)| c.type_id == crate::building_ui::RADIUS_UI)
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
    ) -> Option<crate::building_ui::WorkPlaceClick> {
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

    /// TownHallUI's combos (`FUN_140081650` event `0x14`) in the window containing `node`.
    pub fn town_hall_select(
        &mut self,
        ui: &mut Ui,
        statistics: &banished_game::tracker::Statistics,
        node: NodeId,
    ) -> bool {
        let Some(w) = self.open.iter_mut().find(|w| ui.descendant(w.root, node)) else {
            return false;
        };
        w.bound.iter_mut().any(|c| match c {
            Controller::TownHall(c) => c.select(ui, statistics, node),
            _ => false,
        })
    }

    /// PageToggleUI's button (`FUN_14007abe0`) in the window containing `node`.
    pub fn toggle_page(&mut self, ui: &mut Ui, node: NodeId) -> bool {
        let Some(w) = self.open.iter_mut().find(|w| ui.descendant(w.root, node)) else {
            return false;
        };
        w.bound.iter_mut().any(|c| match c {
            Controller::PageToggle(c) => c.click(ui, node),
            _ => false,
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

    /// The window under `(x, y)` (topmost first), and whether the point is on its
    /// `borderTitle`.
    pub fn window_at(&self, ui: &Ui, x: i32, y: i32) -> Option<(usize, bool)> {
        let inside = |n: NodeId| {
            let [x0, y0, x1, y1] = ui.node(n).rect;
            x0 <= x && x < x1 && y0 <= y && y < y1
        };
        (0..self.open.len()).rev().find_map(|i| {
            let w = &self.open[i];
            inside(w.root).then(|| {
                let title = ui.find(w.root, "borderTitle").is_some_and(inside);
                (i, title)
            })
        })
    }

    /// `FUN_140102160` when the selected entity is off the screen: its unpinned host closes.
    /// Returns whether it did (the selection goes with it).
    pub fn retire(&mut self, ui: &mut Ui, screen: &mut Screen, entity: Entity) -> bool {
        let Some(i) = self
            .open
            .iter()
            .position(|w| w.entity == entity && !w.pinned)
        else {
            return false;
        };
        let w = self.open.remove(i);
        screen.remove_root(ui, w.root);
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

/// DestroyUI's view of the building's DestroyComponent (`FUN_140066460`).
fn destroy_info(town: &StartingTown, p: usize) -> Option<crate::building_ui::DestroyInfo> {
    let site = town.construction.sites.get(&p)?;
    let d = site.destroy.as_ref()?;
    let state = if site.machine.pending < 0 {
        site.machine.state
    } else {
        site.machine.pending
    };
    let own = town.placed[p].template;
    let replace = town.construction.templates[site.template]
        .destroy
        .and_then(|t| t.replace);
    Some(crate::building_ui::DestroyInfo {
        entity: p,
        is_damaged: matches!(state, 6 | 7),
        state,
        damaged: d.damaged,
        count: d.count,
        target: d.target,
        rebuild: d.rebuild == Some(own),
        upgrade: d.rebuild != Some(own) && d.rebuild != replace,
    })
}

/// Slot `+0x68` of the `kind` component of `e` for control `index`: bit 0 enabled, bit 1 on.
fn control_state(town: &StartingTown, e: usize, kind: u32, index: i32) -> u8 {
    use banished_game::control::kind as k;
    let Some(site) = town.construction.sites.get(&e) else {
        return 0;
    };
    let state = if site.machine.pending < 0 {
        site.machine.state
    } else {
        site.machine.pending
    };
    let enabled = town
        .construction
        .workforce
        .workplaces
        .get(&e)
        .is_some_and(|w| w.enabled);
    let on = |b: bool| if b { 2 } else { 0 };
    match kind {
        // `FUN_140120270`: plant now in 2, harvest now in 10, farming; all need a crop.
        k::CROP_FIELD => {
            let Some(f) = &site.crop_field else { return 0 };
            let crop = f.crop.is_some();
            match index {
                0 => u8::from(state == 2 && crop),
                1 => u8::from(state == 0xa && crop),
                2 => on(f.enabled) | u8::from(crop),
                _ => 0,
            }
        }
        // `FUN_1400525f0`: plant (no work and room, or no trees), harvest (in 10, not this
        // year), tending, cut (standing trees, not in 0x14); all need a tree.
        k::ORCHARD => {
            let Some(o) = &site.orchard else { return 0 };
            let tree = o.tree.is_some();
            let standing = o.trees.iter().filter(|t| t.state == 1).count();
            let has_work =
                site.work_id >= 0 && town.construction.jobs.works.contains_key(&site.work_id);
            match index {
                0 => u8::from(
                    ((!has_work && standing < o.trees.len()) || o.trees.is_empty()) && tree,
                ),
                1 => u8::from(state == 0xa && o.year != town.construction.year && tree),
                2 => on(o.enabled) | u8::from(tree),
                3 => u8::from(0 < standing && state != 0x14),
                _ => 0,
            }
        }
        // `FUN_14012d2a0`: room in the other pastures of the kind; the work toggle.
        k::PASTURE => {
            let w = &town.wildlife;
            let Some(p) = w.pastures.get(&e) else {
                return 0;
            };
            let mut free = 0;
            for &o in &w.pasture_list {
                if o != e && w.pastures.get(&o).is_some_and(|x| x.kind == p.kind) {
                    free += w.capacity(o) - w.pastures[&o].livestock.len() as i32;
                }
            }
            let has = u8::from(p.kind.is_some());
            let count = p.livestock.len() as i32;
            match index {
                0 if 0 < count && count <= free => has,
                1 if 9 < count && 4 < free => has,
                2 => on(enabled) | has,
                _ => 0,
            }
        }
        // `FUN_14005caa0`, `FUN_140044870`: the workplace's switch.
        k::TRADE | k::ATTENDANCE => on(enabled) | 1,
        // `FUN_140124df0`: planting and cutting.
        k::FORESTER => match (index, &site.forester) {
            (1, Some(f)) => on(f.plant) | 1,
            (2, Some(f)) => on(f.cut) | 1,
            _ => 1,
        },
        // `FUN_1401309f0`: on unless idle (0xe); available in 0xd and 0xe unless switched off.
        _ => {
            let disabled = site.produce.as_ref().is_some_and(|p| p.disabled);
            on(state != 0xe) | u8::from(!disabled && (state == 0xd || state == 0xe))
        }
    }
}

/// ProductionUI's view of the TrackerComponent (`FUN_14007af20`): in a new year this year's
/// list is last year's.
fn tracker_lists(town: &StartingTown, p: usize) -> Option<[Vec<(usize, i32)>; 2]> {
    let t = town.construction.trackers.get(&p)?;
    if t.year != town.construction.year {
        return Some([Vec::new(), t.this_year.clone()]);
    }
    Some([t.this_year.clone(), t.last_year.clone()])
}

/// CemeteryUI's counts: graves whose cell holds a tombstone (`FUN_140047730`), and graves.
fn cemetery_counts(town: &StartingTown, p: usize) -> Option<(i32, i32)> {
    if !town.construction.cemeteries.contains(&p) {
        return None;
    }
    let graves = &town.construction.sites.get(&p)?.graves;
    let used = graves
        .iter()
        .filter(|&&c| {
            town.cells.objects[c as usize]
                .is_some_and(|o| town.construction.tombstones.contains_key(&o))
        })
        .count();
    Some((used as i32, graves.len() as i32))
}

/// ConsumeProduceUI's view: the template, the selector, productions made and the lifetime.
fn produce_info(town: &StartingTown, p: usize) -> Option<(usize, String, i32, i32, i32)> {
    let site = town.construction.sites.get(&p)?;
    let state = site.produce.as_ref()?;
    let lifetime = town.construction.templates[site.template]
        .produce
        .as_ref()
        .map_or(-1, |t| t.lifetime);
    Some((
        p,
        town.templates[town.placed[p].template].reference.clone(),
        state.selector,
        state.produced,
        lifetime,
    ))
}

/// ImmigrationUI's view: nomads waiting (state 0x11) and how many came (`+0x60`).
fn immigration_info(town: &StartingTown, p: usize) -> Option<(bool, i32)> {
    let site = town.construction.sites.get(&p)?;
    let im = site.immigration.as_ref()?;
    let m = &site.machine;
    let state = if m.pending < 0 { m.state } else { m.pending };
    Some((state == 0x11, im.arrived))
}

/// ResidenceUI's residents, family by family, each as CitizenUI reads it.
fn residents(
    town: &StartingTown,
    p: usize,
) -> Option<Vec<(usize, crate::building_ui::CitizenInfo)>> {
    let home = town.construction.homes.get(&p)?;
    Some(
        home.families
            .iter()
            .flatten()
            .filter_map(|&c| citizen_info(town, c).map(|info| (c, info)))
            .collect(),
    )
}
