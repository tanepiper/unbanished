//! A step's goals: whether each one is met, and the counts some of them wait for.

use super::*;

impl GameplayLoop {
    /// A goal (`TutorialGoal` slot `+0x40`).
    pub(super) fn goal_met(&self, ui: &Ui, archive: &Archive, g: &Goal) -> bool {
        match g {
            // `FUN_1400e41b0`: the element was pressed.
            Goal::Button(path) => {
                let Some(n) = self.tutorial_element(ui, path) else {
                    return false;
                };
                self.tutorial
                    .as_ref()
                    .is_some_and(|t| t.pressed.iter().any(|&p| p == n || ui.descendant(n, p)))
            }
            Goal::Count {
                kind,
                amount,
                reference,
                at_least,
                path,
            } => {
                let n = self.goal_count(ui, archive, *kind, reference, path);
                if *at_least {
                    *amount <= n
                } else {
                    n == *amount
                }
            }
            // `FUN_1400e43b0`: a window with the controller is open (on the template).
            Goal::Dialog { path, component } => {
                let parts: Vec<&str> = path.split(':').collect();
                let ["DialogInstance", class, ..] = parts.as_slice() else {
                    return false;
                };
                let id = class_id(class);
                let Some(town) = &self.sim.town else {
                    return false;
                };
                self.entities.open.iter().any(|w| {
                    w.controllers.iter().any(|(c, _)| c.type_id == id)
                        && (component.is_empty()
                            || file(w.entity.template(town)) == file(component))
                })
            }
            // `FUN_1400e4620`.
            Goal::Speed(s) => {
                !self.paused && super::super::SPEED_LEVELS[self.speed as usize] as i32 == *s
            }
        }
    }

    /// `FUN_1400e4760`'s counts.
    pub(super) fn goal_count(
        &self,
        ui: &Ui,
        _archive: &Archive,
        kind: i32,
        r: &str,
        path: &str,
    ) -> i32 {
        let Some(town) = &self.sim.town else { return 0 };
        let of_template =
            |e: usize| file(&town.templates[town.placed[e].template].reference) == file(r);
        let profession = || {
            town.construction
                .professions
                .list
                .iter()
                .position(|p| p.reference.eq_ignore_ascii_case(r))
        };
        let live = || {
            town.citizens
                .iter()
                .zip(&town.agents.states)
                .filter(|(c, _)| c.alive)
        };
        match kind {
            // The template's entities (`FUN_1400974e0`).
            0 => (0..town.placed.len())
                .filter(|&e| town.placed[e].alive && of_template(e))
                .count() as i32,
            // Working adults of the profession.
            1 => {
                let p = profession();
                live()
                    .filter(|(c, s)| {
                        c.is_adult(&town.rules) && c.school.is_none() && Some(s.profession) == p
                    })
                    .count() as i32
            }
            2 => live().count() as i32,
            // The profession's workplaces (`FUN_140096570`).
            3 => profession()
                .and_then(|p| town.construction.workforce.registry.get(&p))
                .map_or(0, |l| l.len() as i32),
            // The stock manager's storages (`+8`).
            4 => town.storage_sites.len() as i32,
            // Roads planned (map `+0xf0`).
            5 => (!town.construction.pending_roads.is_empty()) as i32,
            // Workers at the profession's workplaces taking them (`FUN_140135080`).
            6 => profession()
                .and_then(|p| town.construction.workforce.registry.get(&p))
                .map_or(0, |l| {
                    l.iter()
                        .filter_map(|e| town.construction.workforce.workplaces.get(e))
                        .filter(|w| w.enabled)
                        .map(|w| w.workers.len() as i32)
                        .sum()
                }),
            // What the trading posts keep of the material.
            7 => {
                let m = town
                    .materials
                    .iter()
                    .position(|m| file(&m.reference) == file(r));
                town.construction
                    .trading_posts
                    .iter()
                    .filter_map(|p| town.construction.sites.get(p)?.trade.as_ref())
                    .map(|t| m.and_then(|m| t.keep.get(&m).copied()).unwrap_or(0))
                    .sum()
            }
            // What the trading posts hold.
            8 => town
                .construction
                .trading_posts
                .iter()
                .filter_map(|p| town.storages.get(town.construction.sites.get(p)?.storage))
                .flat_map(|s| s.items.iter())
                .map(|i| i.count)
                .sum(),
            // The spinner's value, unless it is being typed in.
            9 => self
                .tutorial_element(ui, path)
                .and_then(|n| crate::ui::building_ui::spinner_value(ui, n))
                .unwrap_or(0),
            // The template's activated entities (state machine past 0).
            10 => town
                .construction
                .sites
                .iter()
                .filter(|(&e, s)| s.machine.state != 0 && of_template(e))
                .count() as i32,
            _ => 0,
        }
    }
}
