//! The HUD: the toolbar, the status values it shows, and the tooltip.

use super::*;

impl GameplayLoop {
    /// `FUN_14009d0d0` installs the toolbar with the other in-game dialogs.
    pub(super) fn build_hud(
        &mut self,
        ui: &mut Ui,
        archive: &unbanished_formats::pkg::Archive,
        registry: &unbanished_formats::registry::Registry,
        keymap: &crate::keymap::KeyMap,
        #[cfg_attr(target_arch = "wasm32", allow(unused_variables))] autosave_interval: i32,
    ) {
        let mut screen = Screen::new(ui);
        let mut toolbar = Toolbar::load(archive, registry);
        toolbar.install(ui, &mut screen, keymap);
        self.toolbar = Some(toolbar);
        self.notify = crate::ui::event_notify::EventNotify::open(ui, &mut screen, &self.sim.events);
        {
            self.autosave =
                crate::saves::autosave::AutoSave::open(ui, &mut screen, autosave_interval);
        }
        self.screen = Some(screen);
        self.update_hud(ui);
    }

    /// The values the Status Bar's FUN_14007eda0 reads from the starting town and environment.
    pub(super) fn status_values(&self) -> StatusValues {
        let summary = self.sim.town.as_ref().map_or_else(Default::default, |t| {
            unbanished_game::people::status::aggregate(
                t.citizens
                    .iter()
                    .zip(&t.agents.states)
                    .filter(|(c, _)| c.alive)
                    .map(|(c, s)| unbanished_game::people::status::CitizenValue {
                        age: c.age,
                        adult_age: t.rules.adult_age,
                        school: c.school.is_some(),
                        health: s.health,
                        happiness: s.happiness,
                        status: s.status,
                    }),
            )
        });
        StatusValues {
            events: Some(self.sim.events.clone()),
            map_size: self.sim.town.as_ref().map_or(0, |t| t.cells.size as u32),
            frustum: self.frustum(),
            town_name: self.params.town_name.clone(),
            year: self.sim.calendar.year,
            month: self.month(),
            temperature: self.sim.environment.as_ref().map_or(0.0, |e| e.temperature),
            weather: self.sim.environment.as_ref().map_or(3, |e| e.state),
            adults: summary.adults,
            students: summary.students,
            children: summary.children,
            status: summary.counts,
            health: summary.health,
            happiness: summary.happiness,
            health_max: self
                .sim
                .town
                .as_ref()
                .map_or(1.0, |t| t.agents.rules().health_max),
            happiness_max: self
                .sim
                .town
                .as_ref()
                .map_or(1.0, |t| t.agents.rules().happiness),
            // Adults per profession (`FUN_14007ba70`, citizen `+0x1f0`) and workplace places.
            professions: self
                .sim
                .town
                .as_ref()
                .map_or(vec![self.population().0], |t| {
                    t.profession_counts().0.iter().map(|&n| n as u32).collect()
                }),
            profession_capacity: self
                .sim
                .town
                .as_ref()
                .map_or(Vec::new(), |t| t.profession_counts().1),
            // `FUN_14008f3a0`, recomputed from the town storages.
            stock_limits: self.sim.stock_warnings.limits,
            stock: self.sim.town.as_ref().map_or([0; 21], |t| {
                unbanished_game::work::stock::stock_table(&t.registered_storages(), &t.materials)
            }),
            show_celsius: self.show_celsius,
            citizens: self.sim.town.as_ref().map_or(Vec::new(), |t| {
                t.citizens
                    .iter()
                    .zip(&t.agents.states)
                    .enumerate()
                    .filter(|(_, (c, _))| c.alive)
                    .map(|(i, (c, s))| {
                        (
                            i,
                            s.profession,
                            c.is_adult(&t.rules) && c.school.is_none(),
                            s.status,
                        )
                    })
                    .collect()
            }),
            game_flags: self
                .sim
                .town
                .as_ref()
                .map_or(0, |t| t.construction.game_flags),
            ..StatusValues::default()
        }
    }

    pub(super) fn update_hud(&self, ui: &mut Ui) {
        self.refresh_toolbar(ui);
        self.utility.update(ui, &self.status_values());
    }

    /// `FUN_1400be210` with the town's sites by template (`FUN_1400be4a0`).
    pub(super) fn refresh_toolbar(&self, ui: &mut Ui) {
        let Some(toolbar) = &self.toolbar else { return };
        let count = |template: &str| -> i32 {
            let Some(town) = &self.sim.town else { return 0 };
            let key = crate::start::normalise(template).to_ascii_lowercase();
            town.construction
                .sites
                .iter()
                .filter(|(&e, s)| {
                    let p = &town.placed[e];
                    !s.destroyed
                        && p.alive
                        && town.templates[p.template].reference.to_ascii_lowercase() == key
                })
                .count() as i32
        };
        toolbar.refresh(ui, SPEED_LEVELS[self.speed as usize], self.paused, &count);
    }

    pub(super) fn population(&self) -> (u32, u32, u32) {
        self.sim.town.as_ref().map_or((0, 0, 0), |t| {
            unbanished_game::people::citizens::population(&t.citizens, &t.rules)
        })
    }

    /// Application tooltip controllers: `BuildRequirementUI` on the toolbar's build tooltip
    /// (`Dialog/ToolTipToolBar.rsc`), filled from the button's template.
    pub(super) fn fill_tooltip(&self, ctx: &mut Ctx) {
        let Some((source, root, controllers)) = ctx.ui.new_tooltip() else {
            return;
        };
        if !controllers.contains(&unbanished_ui::tooltip_controller::BUILD_REQUIREMENT) {
            return;
        }
        let Some((template, description)) =
            self.toolbar.as_ref().and_then(|t| t.build_entry(source))
        else {
            return;
        };
        let file = template.split(':').next().unwrap_or("").replace('\\', "/");
        let build = ctx
            .archive
            .read_ref(&format!("{file}:build"))
            .ok()
            .and_then(|d| unbanished_formats::game::build_description(&d).ok());
        crate::ui::create_bar::fill_requirements(
            ctx.ui,
            ctx.archive,
            root,
            description,
            build.as_ref(),
        );
        ctx.ui.place_tooltip();
    }
}
