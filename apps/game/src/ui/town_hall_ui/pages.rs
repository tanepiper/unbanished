//! The overview, professions, production and agriculture pages.

use super::*;

impl TownHallUi {
    /// `FUN_140082500`.
    pub(super) fn overview(&mut self, ui: &mut Ui, town: &StartingTown) {
        let life = &town.agents.life;
        let (mut adults, mut students, mut children, mut families) = (0, 0, 0, 0);
        let (mut educated, mut clothed) = (0.0f32, 0.0f32);
        let (mut health, mut happiness) = (0.0f32, 0.0f32);
        let living: Vec<usize> = (0..town.citizens.len())
            .filter(|&c| town.citizens[c].alive)
            .collect();
        for &c in &living {
            let (citizen, s) = (&town.citizens[c], &town.agents.states[c]);
            if citizen.age < life.adult_age() || s.school.is_some() {
                if s.school.is_none() {
                    children += 1;
                } else {
                    students += 1;
                }
            } else {
                adults += 1;
                if life.i(0x118) <= citizen.education {
                    educated += 1.0;
                }
                // A family: an adult alone, or a couple counted by its man (`+0x250`, `+0x38`).
                match citizen.spouse {
                    None => families += 1,
                    Some(_) if citizen.male => families += 1,
                    _ => {}
                }
            }
            happiness += s.happiness;
            health += s.health;
            if s.clothing != 0 {
                clothed += 1.0;
            }
        }
        let n = living.len() as i32;
        let inverse = if 0 < n { 1.0 / n as f32 } else { 0.0 };
        if 0 < adults {
            educated = (educated / adults as f32) * 100.0;
        }
        set_number(ui, self.year, town.construction.year as i32);
        if let Some(m) = self.month {
            let month = ((town.construction.calendar / 3600.0) * 12.0) as i32;
            let month = if 11 < month { 0 } else { month };
            if let Some(table) = ui.node(m).string_table.clone() {
                let text = ui.res.string_at(&table, month);
                ui.set_text(m, &text);
            }
        }
        set_number(ui, self.homes, town.construction.homes.len() as i32);
        set_number(ui, self.citizens, n);
        set_number(ui, self.adults, adults);
        set_number(ui, self.students, students);
        set_number(ui, self.children, children);
        set_number(ui, self.families, families);
        if let Some((node, t)) = &self.clothed {
            let v = (clothed * inverse * 100.0 + 0.5) as i32;
            ui.set_text(*node, &t.replace("@0", &v.to_string()));
        }
        if let Some((node, t)) = &self.educated {
            let v = (educated + 0.5) as i32;
            ui.set_text(*node, &t.replace("@0", &v.to_string()));
        }
        if n < 1 {
            crate::ui::utility::StatusBar::set_meter(ui, self.hearts, 0.0, 1.0);
            crate::ui::utility::StatusBar::set_meter(ui, self.stars, 0.0, 1.0);
        } else {
            crate::ui::utility::StatusBar::set_meter(
                ui,
                self.hearts,
                health * inverse,
                life.f(0x70),
            );
            crate::ui::utility::StatusBar::set_meter(
                ui,
                self.stars,
                happiness * inverse,
                life.f(0xa4),
            );
        }
    }

    /// `FUN_1400833c0`: each profession's workers, jobs and workplaces.
    pub(super) fn professions(&mut self, ui: &mut Ui, town: &StartingTown) {
        let (counts, places) = town.profession_counts();
        let registry = &town.construction.workforce.registry;
        let count = |p: usize| counts.get(p).copied().unwrap_or(0);
        for (i, row) in self.professions.iter().enumerate() {
            if let Some(s) = row.spinner {
                let v = count(i);
                if !ui.focused().is_some_and(|f| ui.descendant(s, f)) {
                    set_spinner(ui, s, v);
                }
                // `NumberEditElement::SetMinMax`, as the Professions window.
                let (lo, hi) = if i == 0 {
                    (count(0), count(0))
                } else {
                    (0, count(i) + count(0))
                };
                ui.set_spinner_range(s, lo, hi);
            }
            set_number(ui, row.jobs, places.get(i).copied().unwrap_or(0));
            set_number(
                ui,
                row.locations,
                registry.get(&i).map_or(0, |w| w.len() as i32),
            );
            if let Some(b) = row.goto {
                ui.node_mut(b).enabled = registry.get(&i).is_some_and(|w| !w.is_empty());
            }
        }
    }

    /// `FUN_140082180`: per slot its count, this year's use and production (the samples so
    /// far plus what is not sampled yet) and the previous period's.
    pub(super) fn production(&mut self, ui: &mut Ui, town: &StartingTown, ctx: &TownContext) {
        let years = [1, 2, 5, 10, 25, 50, 100][combo_index(ui, self.period).min(6)];
        for (node, template) in self.prev_headers.iter().flatten() {
            ui.set_text(*node, &template.replace("@0", &years.to_string()));
        }
        let year = town.construction.year as i32;
        let start = year * SAMPLES_PER_YEAR - SAMPLES_PER_YEAR;
        let slice = ((town.construction.calendar / 3600.0) * SAMPLES_PER_YEAR as f32) as i32;
        let slice = if 0x2f < slice { 0 } else { slice };
        let now = start + slice;
        let (prev_from, prev_to) = (
            (year - years - 1) * SAMPLES_PER_YEAR,
            year * SAMPLES_PER_YEAR - SAMPLES_PER_YEAR - 1,
        );
        let counters = &town.construction.counters;
        for (k, row) in self.limits.iter().enumerate().take(21) {
            let s = ctx.statistics;
            let (used, made) = (9 + 3 * k, 10 + 3 * k);
            set_number(ui, row[0], ctx.stock_counts[k]);
            set_number(ui, row[1], sum(s, used, start, now) + counters.used[k]);
            set_number(ui, row[2], sum(s, made, start, now) + counters.made[k]);
            set_number(ui, row[3], sum(s, used, prev_from, prev_to));
            set_number(ui, row[4], sum(s, made, prev_from, prev_to));
        }
    }

    /// `FUN_1400816f0`: the known crops, trees and livestock, each its SelectDescription's
    /// icon and name; the rest hidden.
    pub(super) fn agriculture(&mut self, ui: &mut Ui, archive: &Archive, town: &StartingTown) {
        let mut known = Vec::new();
        for &t in &town.construction.known {
            let file = town.templates[t]
                .reference
                .split(':')
                .next()
                .unwrap_or("")
                .replace('\\', "/");
            if let Some(d) = archive
                .read_ref(&format!("{file}:select"))
                .ok()
                .and_then(|d| select_description(&d))
            {
                known.push(d);
            }
        }
        for (i, &cell) in self.agri.iter().enumerate() {
            match known.get(i) {
                Some((sheet, sprite, table, name)) => {
                    let s = ui.res.sprite(sheet.as_deref(), sprite);
                    if let Kind::Label { icon, .. } = &mut ui.node_mut(cell).kind {
                        *icon = s;
                    }
                    let text = ui.res.string(table.as_deref(), name);
                    ui.node_mut(cell).tooltip = text;
                    ui.set_visible(cell, true);
                }
                None => ui.set_visible(cell, false),
            }
        }
    }
}
