//! The events the town posts to the event log after each frame's iterations: the citizens'
//! births, deaths and coming of age, and the sites' events.

use unbanished_formats::pkg::Archive;

use super::Simulation;
use crate::strings::Strings;

impl Simulation {
    /// The citizens' births, deaths and coming of age posted to the event log with `@2` the
    /// citizen's name, `@3` its role (profession, child or student) and `@4` the disease's or
    /// accident's name.
    ///
    /// Original: `FUN_140118fc0`.
    pub fn log_life_events(&mut self, strings: &mut dyn Strings, auto_slow: bool) {
        let (month, year) = (self.month(), self.calendar.year as i32);
        let Some(town) = &mut self.town else {
            return;
        };
        let events = std::mem::take(&mut town.agents.events);
        if events.is_empty() {
            return;
        }
        for e in events {
            let Some(c) = town.citizens.get(e.citizen) else {
                continue;
            };
            let state = &town.agents.states[e.citizen];
            let child = c.age < town.rules.adult_age;
            let key = if c.school.is_some() {
                Some("student")
            } else if child {
                Some("child")
            } else {
                None
            };
            let role = match key {
                Some(k) => town.profession(k).map(|p| (p.string_table, p.name)),
                None => town
                    .construction
                    .professions
                    .list
                    .get(state.profession)
                    .map(|p| (p.string_table.clone(), p.name.clone())),
            }
            .map(|(t, k)| strings.string(t.as_deref(), &k))
            .unwrap_or_default();
            let extra = e
                .extra
                .map(|(t, k)| strings.string(t.as_deref(), &k))
                .unwrap_or_default();
            self.events.add(
                e.kind,
                e.severity,
                -1,
                month,
                year,
                [c.name.clone(), role, extra],
                auto_slow,
            );
        }
    }

    /// The sites' events, posted with the entity: `@2` its toolbar name, at its map-rectangle
    /// centre cell.
    ///
    /// Original: `FUN_14009a500`, `FUN_14009a2d0`.
    pub fn log_site_events(
        &mut self,
        archive: &Archive,
        strings: &mut dyn Strings,
        auto_slow: bool,
    ) {
        let (month, year) = (self.month(), self.calendar.year as i32);
        let Some(town) = &mut self.town else {
            return;
        };
        let events = std::mem::take(&mut town.construction.site_events);
        for (kind, severity, e) in events {
            let Some(p) = town.placed.get(e) else {
                continue;
            };
            let reference = &town.templates[p.template].reference;
            let file = reference.split(':').next().unwrap_or("");
            let name = archive
                .read_ref(&format!("{file}:toolbar"))
                .ok()
                .and_then(|d| unbanished_formats::game::toolbar_description(&d).ok())
                .map(|t| strings.string(t.string_table.as_deref(), &t.name))
                .unwrap_or_default();
            let n = town.cells.size as i32;
            let cell = (p.cell.1 + (p.footprint.1 >> 1)) * n + p.cell.0 + (p.footprint.0 >> 1);
            self.events.add(
                kind,
                severity,
                cell,
                month,
                year,
                [name, String::new(), String::new()],
                auto_slow,
            );
        }
    }
}
