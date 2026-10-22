//! The window's events in the town: the pointer, the wheel and keys, for the interface, the tools and the camera.

use super::*;

impl GameplayLoop {
    /// The UI events of `GameLoop::event`, before the queued commands are applied.
    pub(in crate::gameplay) fn window_event(&mut self, ctx: &mut Ctx, event: Event) -> Next {
        if let (Some(d), Some(screen)) = (self.mods_dialog.as_mut(), self.screen.as_mut()) {
            if let Event::Selected(id, index) = event {
                d.selected(ctx.ui, &ctx.settings.persistence, ctx.mods, id, index);
            }
            if let Event::Clicked(id) | Event::Changed(id) = event {
                use crate::ui::mod_ui::Outcome;
                match d.click(ctx.ui, screen, ctx.mods, &ctx.settings.persistence, id) {
                    Outcome::Stay => {}
                    Outcome::Closed => self.mods_dialog = None,
                    Outcome::Reload => {
                        // `FUN_140072110` over a game: saved to memory, then loaded again.
                        self.mods_dialog = None;
                        self.store_window_settings(ctx.ui, true);
                        let saved = self.save_file(ctx.archive, ctx.ui, ctx.mods, None, None);
                        *ctx.reload_mods = Some(match saved {
                            Some(bytes) => crate::mods::Reload::Game(bytes),
                            None => crate::mods::Reload::Menu,
                        });
                    }
                }
            }
            return Next::Stay;
        }
        if let (Some(d), Some(screen)) = (self.save_dialog.as_mut(), self.screen.as_mut()) {
            if let Event::Clicked(id) = event {
                let outcome = d.click(ctx.ui, screen, ctx.audio, ctx.archive, ctx.mods, id);
                if let Some(next) = self.save_outcome(ctx, outcome) {
                    return next;
                }
            }
            return Next::Stay;
        }
        if let (Some(options), Some(screen)) = (&mut self.options, &mut self.screen) {
            if options.event(ctx, screen, event) == crate::ui::options::Outcome::Closed {
                self.options = None;
            }
            return Next::Stay;
        }
        // `FUN_1400e41b0`'s "pressed": a click, or a toggle (the toolbar's menu buttons are check
        // controls, which send Changed). Recording only clicks left a pointer press on Housing
        // unseen, so the first tutorial stopped at its first house (issue #11).
        if let (Event::Clicked(id) | Event::Changed(id), Some(t)) = (event, self.tutorial.as_mut())
        {
            t.pressed.push(id);
        }
        // TutorialUI's quit (`FUN_1400e8bb0`): back to the main menu.
        if let Event::Clicked(id) = event {
            if self.tutorial_quit(id) {
                return Self::return_to_menu();
            }
        }
        if self.utility.help_event(ctx.ui, event) {
            return Next::Stay;
        }
        // A combo in a building's window (event `0x14`).
        if let Event::Selected(id, index) = event {
            if self
                .entities
                .town_hall_select(ctx.ui, &self.sim.statistics, id)
            {
                return Next::Stay;
            }
            let chosen = self
                .sim
                .town
                .as_ref()
                .and_then(|town| self.entities.select_item(ctx.ui, town, id, index));
            if let (Some(WorkPlaceClick::Command(c)), Some(_)) = (chosen, &self.sim.town) {
                self.execute(ctx.archive, PlayerCommand::Town(c));
            }
            return Next::Stay;
        }
        let (Event::Clicked(id) | Event::Changed(id)) = event else {
            return Next::Stay;
        };
        // The Achievement and Goal dialogs over the pause menu (OK closes them).
        if let Some(d) = &self.menu_dialog {
            let root = match d {
                MenuDialog::Achievements(a) => a.closes(id).then_some(a.root),
                MenuDialog::Goal(g) => g.closes(id).then_some(g.root),
                MenuDialog::Map(m) => match m.click(ctx.ui, id) {
                    crate::view::map_params::MapParamsClick::Rename(name) => {
                        let root = m.root;
                        self.execute(ctx.archive, PlayerCommand::RenameTown { name });
                        Some(root)
                    }
                    crate::view::map_params::MapParamsClick::NewName(max_length) => {
                        let random = PlayerCommand::RandomTownName { max_length };
                        if let Outcome::Name(name) = self.execute(ctx.archive, random) {
                            if let Some(MenuDialog::Map(m)) = &self.menu_dialog {
                                m.set_name(ctx.ui, &name);
                            }
                        }
                        None
                    }
                    crate::view::map_params::MapParamsClick::Close => Some(m.root),
                    crate::view::map_params::MapParamsClick::None => None,
                },
            };
            if let (Some(root), Some(screen)) = (root, self.screen.as_mut()) {
                screen.remove_root(ctx.ui, root);
                self.menu_dialog = None;
            }
            return Next::Stay;
        }
        if let Some(menu) = &self.pause_menu {
            // `FUN_1400b9020`.
            match menu.click(ctx.ui, id) {
                crate::ui::pause_menu::PauseClick::Resume => self.close_pause_menu(ctx),
                crate::ui::pause_menu::PauseClick::Menu => {
                    self.close_pause_menu(ctx);
                    return Self::return_to_menu();
                }
                crate::ui::pause_menu::PauseClick::Quit => return Next::Quit,
                crate::ui::pause_menu::PauseClick::Options => {
                    if let Some(screen) = &mut self.screen {
                        self.options = crate::ui::options::OptionsDialog::open(ctx, screen);
                    }
                }
                crate::ui::pause_menu::PauseClick::Achievement => {
                    if let Some(screen) = &mut self.screen {
                        let awarded = self.sim.achievement_manager.awarded.clone();
                        if let Some(d) = crate::ui::achievement_ui::AchievementDialog::open(
                            ctx.ui,
                            screen,
                            &self.sim.achievements,
                        ) {
                            d.refresh(ctx.ui, &awarded);
                            self.menu_dialog = Some(MenuDialog::Achievements(d));
                        }
                    }
                }
                crate::ui::pause_menu::PauseClick::Goal => {
                    if let Some(screen) = &mut self.screen {
                        let goal = self.goal.as_ref().map(|g| (&g.achievement, &g.instance));
                        self.menu_dialog =
                            crate::ui::achievement_ui::GoalDialog::open(ctx.ui, screen, goal)
                                .map(MenuDialog::Goal);
                    }
                }
                crate::ui::pause_menu::PauseClick::Map => {
                    self.load_town_names(ctx.archive);
                    let flags = self
                        .sim
                        .town
                        .as_ref()
                        .map_or(0, |t| t.construction.game_flags);
                    if let Some(screen) = &mut self.screen {
                        self.menu_dialog = crate::view::map_params::MapParamsDialog::open(
                            ctx.ui,
                            screen,
                            &self.params,
                            flags,
                        )
                        .map(MenuDialog::Map);
                    }
                }
                crate::ui::pause_menu::PauseClick::Mod => {
                    if let Some(screen) = &mut self.screen {
                        self.mods_dialog = crate::ui::mod_ui::ModDialog::open(
                            ctx.ui,
                            screen,
                            ctx.mods,
                            true,
                            &ctx.settings.persistence,
                            ctx.pack_dir.map(std::path::Path::to_path_buf),
                        );
                    }
                }
                crate::ui::pause_menu::PauseClick::Load
                | crate::ui::pause_menu::PauseClick::Save => {
                    if let Some(screen) = &mut self.screen {
                        let loading = matches!(
                            menu.click(ctx.ui, id),
                            crate::ui::pause_menu::PauseClick::Load
                        );
                        self.save_dialog = crate::ui::save_dialog::SaveLoadDialog::open(
                            ctx.ui, screen, loading, true,
                        );
                    }
                }
                crate::ui::pause_menu::PauseClick::Other => {}
            }
            return Next::Stay;
        }
        // The Follow dialog's close button (`FUN_140069b90`, id 100).
        if self.follow.as_ref().is_some_and(|f| f.is_close(id)) {
            self.close_follow(ctx);
            return Next::Stay;
        }
        if let (Some(notify), Some(screen)) = (self.notify.as_mut(), self.screen.as_mut()) {
            if let Some(goto) = notify.click(ctx.ui, screen, &self.sim.events, id) {
                if let Some(cell) = goto {
                    self.go_to(cell);
                }
                return Next::Stay;
            }
        }
        let values = self.status_values();
        let (Some(screen), Some(toolbar)) = (self.screen.as_mut(), self.toolbar.as_mut()) else {
            return Next::Stay;
        };
        let Some(action) = toolbar.click(ctx.ui, screen, id) else {
            match self.utility.click(ctx.ui, id, &self.sim.events, &values) {
                Some(UtilityClick::ProfessionCount(profession, count)) => {
                    if self.sim.town.is_some() {
                        let set = unbanished_game::work::professions::Command::SetCount {
                            profession,
                            count,
                        };
                        self.execute(ctx.archive, PlayerCommand::Town(set));
                    }
                    return Next::Stay;
                }
                Some(UtilityClick::Notify(i, on)) => {
                    self.sim.events.notify[i] = on;
                    return Next::Stay;
                }
                Some(UtilityClick::StockLimit(slot, limit)) => {
                    self.sim.stock_warnings.limits[slot] = limit;
                    return Next::Stay;
                }
                Some(UtilityClick::GoTo(cell)) => {
                    self.go_to(cell);
                    return Next::Stay;
                }
                Some(UtilityClick::ShowCitizen(citizen)) => {
                    // `FUN_140108510`: centre the camera on the citizen and select it.
                    if let (Some(town), Some(view), Some(screen)) = (
                        self.sim.town.as_ref(),
                        self.view.as_mut(),
                        self.screen.as_mut(),
                    ) {
                        let entity = Entity::Citizen(citizen);
                        if let SceneCamera::Map(camera) = &mut view.camera {
                            camera.target = entity.position(town).truncate();
                        }
                        self.selection.selected = Some(entity);
                        self.entities
                            .select(ctx.ui, screen, ctx.archive, town, entity);
                    }
                    return Next::Stay;
                }
                Some(UtilityClick::Handled) => return Next::Stay,
                None => {}
            }
            if self.entities.toggle_page(ctx.ui, id) {
                return Next::Stay;
            }
            let clicked = self
                .sim
                .town
                .as_ref()
                .and_then(|town| self.entities.click(ctx.ui, town, id));
            match clicked {
                Some(WorkPlaceClick::Command(command)) => {
                    self.execute(ctx.archive, PlayerCommand::Town(command));
                    return Next::Stay;
                }
                // UpgradeUI (`FUN_14008c160`): the template is loaded for the rebuild.
                Some(WorkPlaceClick::Upgrade { entity, template }) => {
                    self.execute(ctx.archive, PlayerCommand::Upgrade { entity, template });
                    return Next::Stay;
                }
                // ResourceLimitUI (`FUN_14007ccb0`): the slot's limit.
                Some(WorkPlaceClick::StockLimit { slot, limit }) => {
                    self.sim.stock_warnings.limits[slot] = limit;
                    return Next::Stay;
                }
                Some(WorkPlaceClick::ShowBuilding(building)) => {
                    self.entities.pin_containing(ctx.ui, id);
                    if let (Some(town), Some(view)) = (self.sim.town.as_ref(), self.view.as_mut()) {
                        let entity = Entity::Placed(building);
                        if let SceneCamera::Map(camera) = &mut view.camera {
                            camera.target = entity.position(town).truncate();
                        }
                        self.selection.selected = Some(entity);
                        self.entities
                            .select(ctx.ui, screen, ctx.archive, town, entity);
                    }
                    return Next::Stay;
                }
                Some(WorkPlaceClick::ShowWorker(citizen)) => {
                    // `FUN_140108510`: pin the asking window, centre the camera on the entity,
                    // then select it (`FUN_140102880`).
                    self.entities.pin_containing(ctx.ui, id);
                    if let (Some(town), Some(view)) = (self.sim.town.as_ref(), self.view.as_mut()) {
                        let entity = Entity::Citizen(citizen);
                        if let SceneCamera::Map(camera) = &mut view.camera {
                            camera.target = entity.position(town).truncate();
                        }
                        self.selection.selected = Some(entity);
                        self.entities
                            .select(ctx.ui, screen, ctx.archive, town, entity);
                    }
                    return Next::Stay;
                }
                None => {}
            }
            // The host dialog's close button (`Dialog/StandardDialog.rsc`).
            if ctx.ui.node(id).name == "buttonClose" {
                self.utility.close_containing(ctx.ui, screen, id);
                if self.entities.close_containing(ctx.ui, screen, id) == self.selection.selected {
                    self.selection.selected = None;
                }
            } else if ctx.ui.node(id).name == "checkPin" {
                self.entities.pin(ctx.ui, id);
            } else if matches!(
                ctx.ui.node(id).name.as_str(),
                "buttonProfession" | "buttonHome"
            ) {
                // CitizenUI event `0x12`: ids `0x227` (`FUN_140065330`: the school, else the
                // workplace) and `0x226` (`FUN_140065270`: the home) open that building through
                // `FUN_140108510`: the asking window pinned, the camera on it, selected.
                let profession = ctx.ui.node(id).name == "buttonProfession";
                let target = match (self.entities.entity_containing(ctx.ui, id), &self.sim.town) {
                    (Some(Entity::Citizen(c)), Some(town)) => {
                        let s = &town.agents.states[c];
                        if profession {
                            s.school.or(s.workplace)
                        } else {
                            s.home
                        }
                    }
                    _ => None,
                };
                if let (Some(building), Some(town), Some(view)) =
                    (target, self.sim.town.as_ref(), self.view.as_mut())
                {
                    self.entities.pin_containing(ctx.ui, id);
                    let entity = Entity::Placed(building);
                    if let SceneCamera::Map(camera) = &mut view.camera {
                        camera.target = entity.position(town).truncate();
                    }
                    self.selection.selected = Some(entity);
                    self.entities
                        .select(ctx.ui, screen, ctx.archive, town, entity);
                }
            } else if ctx.ui.node(id).name == "buttonFollow" {
                // CitizenUI event `0x12` id `0x228` (`FUN_140065080`).
                if let (Some(Entity::Citizen(c)), Some(owner), Some(town)) = (
                    self.entities.entity_containing(ctx.ui, id),
                    self.entities.root_containing(ctx.ui, id),
                    self.sim.town.as_ref(),
                ) {
                    if let Some(old) = self.follow.take() {
                        old.close(ctx.ui, screen);
                    }
                    self.follow =
                        crate::tools::follow::Follow::open(ctx.ui, screen, town, c, owner);
                }
            } else if ctx.ui.node(id).name == "buttonCenter" {
                // EntityUI event `0x12` id `0x65` (`FUN_1400671d0`).
                if let (Some(entity), Some(town), Some(view)) = (
                    self.entities.entity_containing(ctx.ui, id),
                    self.sim.town.as_ref(),
                    self.view.as_mut(),
                ) {
                    if let SceneCamera::Map(camera) = &mut view.camera {
                        camera.target = center_target(town, entity);
                    }
                }
            }
            return Next::Stay;
        };
        self.run_action(ctx, action);
        self.update_hud(ctx.ui);
        Next::Stay
    }
}
