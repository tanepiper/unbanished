//! Refreshing the open entity windows from the town.

use super::*;

impl EntityWindows {
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
        town_ctx: Option<&crate::ui::town_hall_ui::TownContext>,
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
}
