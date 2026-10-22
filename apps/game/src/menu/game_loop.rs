//! The main menu as an engine loop: loading, updating and its events.

use super::*;

impl GameLoop for MenuLoop {
    fn name(&self) -> &'static str {
        "Game/Menu"
    }

    fn load(&mut self, ctx: &mut Ctx) {
        match ctx
            .archive
            .read("Game/Names/Cities_resource.crs")
            .and_then(|bytes| unbanished_formats::names::NameDatabase::read(&bytes))
        {
            Ok(database) => self.town_names = Some(database),
            Err(e) => log::error!("town names: {e}"),
        }
        self.scene_index = ctx.rng.rand_below(MENU_SCENES);
        if let Some(forced) = unbanished_formats::env::var("MENU_SCENE")
            .ok()
            .and_then(|v| v.parse().ok())
        {
            self.scene_index = forced;
        }
        // `FUN_14009fe00`: the menu music SoundEffect, loaded with the scene. It decodes on
        // another thread while the scene loads (native); nothing plays until `startup`.
        let archive: &unbanished_formats::pkg::Archive = ctx.archive;
        let (music, scene) = unbanished_formats::par::join(
            || unbanished_audio::EffectCache::prepare(archive, &[MENU_MUSIC]),
            || load_menu_scene(ctx, self.scene_index),
        );
        match scene {
            Ok(view) => self.scene = Some(view),
            Err(e) => log::error!("menu scene {}: {e}", self.scene_index),
        }
        ctx.audio.add_prepared(music);
        if ctx.audio.effect(ctx.archive, MENU_MUSIC).is_none() {
            log::error!("menu music: {MENU_MUSIC} did not load");
        }
    }

    fn startup(&mut self, ctx: &mut Ctx) {
        let mut screen = Screen::new(ctx.ui);
        if let Some(d) = screen.add_dialog(ctx.ui, MENU_DIALOG) {
            crate::ui::multiplayer::add_menu_button(ctx.ui, &screen, d);
        }
        if let Some(d) = screen.add_dialog(ctx.ui, BUILD_ID_DIALOG) {
            if let Some(notice) = screen.find(ctx.ui, d, "textBuildNotice") {
                set_parameters(ctx.ui, notice, &[VERSION, BUILD_ID]);
            }
        }
        self.screen = Some(screen);
        // `FUN_1400a0e00`: `Scene::PlayEffect(music, 2)`.
        ctx.audio.play(
            ctx.archive,
            MENU_MUSIC,
            crate::audio::sound::sound_type::MUSIC,
        );
    }

    /// `FUN_1400a0750`: `Audio::Scene::StopAll`.
    fn shutdown(&mut self, ctx: &mut Ctx) {
        ctx.audio.stop_all();
        // The menu has its own Scene in the original (`+0xb30`); the port shares one, so the
        // menu's pause does not outlive it.
        ctx.audio
            .pause(crate::audio::sound::sound_type::MUSIC, false);
    }

    fn update(&mut self, ctx: &mut Ctx, dt: f32) -> Next {
        if let Some(d) = self.mods_dialog.as_mut() {
            d.poll(ctx.ui, ctx.settings, ctx.mods);
        }
        // `FUN_1400a0bb0`: the music pauses while the application is paused (unfocused), and
        // otherwise `Scene::Update(dt, 0, random, origin, 0)`.
        if ctx.app_paused != self.app_paused {
            self.app_paused = ctx.app_paused;
            ctx.audio
                .pause(crate::audio::sound::sound_type::MUSIC, ctx.app_paused);
        }
        if !ctx.app_paused {
            ctx.audio.update(dt, 0.0, glam::Vec3::ZERO, 0.0);
        }
        if let Some(SceneView {
            camera: SceneCamera::Menu(camera),
            ..
        }) = &mut self.scene
        {
            camera.update(dt);
        }
        if let Some(options) = &mut self.options {
            options.update();
        }
        let mut bytes = None;
        if let (Some(d), Some(screen)) = (self.save_dialog.as_mut(), self.screen.as_mut()) {
            let outcome = d.update(ctx.ui, screen, dt);
            if matches!(outcome, crate::ui::save_dialog::Outcome::Load(_)) {
                bytes = d.bytes.take();
            }
            self.save_outcome(outcome);
        }
        if let (Some(mp), Some(screen)) = (self.multiplayer.as_mut(), self.screen.as_mut()) {
            // The host's save starts the game for every player.
            let outcome = match (self.load.take(), bytes) {
                (Some(_), Some(bytes)) => mp.host_start(ctx.ui, screen, bytes),
                _ => mp.update(ctx.ui, screen),
            };
            if let crate::ui::multiplayer::Outcome::Start(game) = outcome {
                self.multiplayer = None;
                let screen_ref = format!("Dialog/Loading.rsc:gameLoad{}", ctx.rng.rand_below(12));
                return Next::Switch(Box::new(DialogLoaderLoop::new(screen_ref, 0.5, game)));
            }
        }
        if let Some(saved) = self.load.take() {
            log::info!("loading {}", saved.metadata.name);
            let screen_ref = format!("Dialog/Loading.rsc:gameLoad{}", ctx.rng.rand_below(12));
            return Next::Switch(Box::new(DialogLoaderLoop::new(
                screen_ref,
                0.5,
                Box::new(crate::gameplay::GameplayLoop::from_save(saved)),
            )));
        }
        Next::Stay
    }

    /// `load PATH`: start the save at `PATH` (captures; players use the Load dialog).
    #[cfg(not(target_arch = "wasm32"))]
    fn script(&mut self, _ctx: &mut Ctx, command: &str) -> bool {
        let Some(path) = command.strip_prefix("load ") else {
            return false;
        };
        match std::fs::read(path.trim())
            .map_err(|e| unbanished_formats::Error::Format(e.to_string()))
            .and_then(|b| crate::saves::saved::SavedGame::parse(&b))
        {
            Ok(saved) => {
                self.load = Some(saved);
                true
            }
            Err(e) => {
                log::error!("{path}: {e}");
                false
            }
        }
    }

    fn raw(&mut self, ctx: &mut Ctx, input: RawInput) -> Next {
        if let (Some(d), Some(screen)) = (self.mods_dialog.as_mut(), self.screen.as_mut()) {
            if let RawInput::Escape = input {
                d.cancel(ctx.ui, screen, ctx.mods, &ctx.settings.persistence);
                self.mods_dialog = None;
            }
            return Next::Stay;
        }
        if let (Some(d), Some(screen)) = (self.save_dialog.as_mut(), self.screen.as_mut()) {
            if let RawInput::Escape = input {
                let outcome = d.escape(ctx.ui, screen);
                self.save_outcome(outcome);
            }
            return Next::Stay;
        }
        if let (Some(mp), Some(screen), RawInput::Escape) =
            (self.multiplayer.as_mut(), self.screen.as_mut(), &input)
        {
            mp.close(ctx.ui, screen);
            self.multiplayer = None;
            return Next::Stay;
        }
        if let (Some(options), Some(screen)) = (&mut self.options, &mut self.screen) {
            let outcome = match input {
                RawInput::Released(binding) => {
                    options.released(ctx, screen, binding);
                    crate::ui::options::Outcome::Stay
                }
                RawInput::Escape => options.escape(ctx, screen),
                _ => crate::ui::options::Outcome::Stay,
            };
            if outcome == crate::ui::options::Outcome::Closed {
                self.options = None;
            }
        }
        Next::Stay
    }

    fn view(&self) -> View<'_> {
        self.scene.as_ref().map_or(View::None, View::World)
    }

    fn event(&mut self, ctx: &mut Ctx, event: Event) -> Next {
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
                        self.mods_dialog = None;
                        *ctx.reload_mods = Some(crate::mods::Reload::Menu);
                    }
                }
            }
            return Next::Stay;
        }
        if let (Some(d), Some(screen)) = (self.save_dialog.as_mut(), self.screen.as_mut()) {
            if let Event::Clicked(id) = event {
                let outcome = d.click(ctx.ui, screen, ctx.audio, ctx.archive, ctx.mods, id);
                self.save_outcome(outcome);
            }
            return Next::Stay;
        }
        if let (Some(options), Some(screen)) = (&mut self.options, &mut self.screen) {
            if options.event(ctx, screen, event) == crate::ui::options::Outcome::Closed {
                self.options = None;
            }
            return Next::Stay;
        }
        if let (Some(mp), Some(screen)) = (self.multiplayer.as_mut(), self.screen.as_mut()) {
            use crate::ui::multiplayer::Outcome;
            if let Event::Clicked(id) = event {
                match mp.click(ctx.ui, screen, &mut ctx.settings.persistence, id) {
                    Outcome::Closed => self.multiplayer = None,
                    Outcome::ChooseSave => {
                        self.save_dialog = crate::ui::save_dialog::SaveLoadDialog::open(
                            ctx.ui, screen, true, false,
                        );
                        if let Some(d) = &mut self.save_dialog {
                            d.keep_bytes = true;
                        }
                    }
                    Outcome::Stay | Outcome::Start(_) => {}
                }
            }
            return Next::Stay;
        }
        if let (Event::Selected(..), Some(_)) = (event, self.new_game) {
            self.refresh_description(ctx);
            return Next::Stay;
        }
        let Event::Clicked(id) = event else {
            return Next::Stay;
        };
        if let Some(d) = &mut self.scenarios {
            use crate::ui::scenario_ui::ScenarioClick;
            match d.click(ctx.ui, id) {
                Some(ScenarioClick::Close) => {
                    let root = d.root;
                    if let Some(screen) = &mut self.screen {
                        screen.remove_root(ctx.ui, root);
                    }
                    self.scenarios = None;
                }
                // `FUN_14007d180`: the scenario's save or new game.
                Some(ScenarioClick::Start(i)) => {
                    let list = crate::gameplay::scenarios(ctx.archive);
                    if let Some((_, s)) = list.get(i) {
                        let game = crate::gameplay::scenario_game(
                            ctx.ui,
                            ctx.archive,
                            ctx.global.as_ref(),
                            s,
                            i,
                        );
                        let screen_ref =
                            format!("Dialog/Loading.rsc:gameLoad{}", ctx.rng.rand_below(12));
                        return Next::Switch(Box::new(DialogLoaderLoop::new(
                            screen_ref,
                            0.5,
                            Box::new(game),
                        )));
                    }
                }
                _ => {}
            }
            return Next::Stay;
        }
        if let Some(a) = &self.achievements {
            if a.closes(id) {
                let root = a.root;
                if let Some(screen) = &mut self.screen {
                    screen.remove_root(ctx.ui, root);
                }
                self.achievements = None;
            }
            return Next::Stay;
        }
        if let Some(d) = self.new_game {
            // NewGame IDs (FUN_140075ba0), handled by FUN_1400755e0: buttonNewName 0
            // (FUN_140075a80), buttonNewSeed 1 (FUN_140075a00), buttonOk 2 (FUN_140075750, starts
            // the game), buttonCancel 3 (closes). See docs/NEW-GAME-FLOW.md.
            if self.is(ctx.ui, d, "buttonNewSeed", id) {
                self.new_seed(ctx);
            } else if self.is(ctx.ui, d, "buttonOk", id) {
                return self.start_game(ctx);
            } else if self.is(ctx.ui, d, "buttonCancel", id) {
                self.close_new_game(ctx);
            } else if self.is(ctx.ui, d, "buttonNewName", id) {
                self.new_town_name(ctx);
            }
            return Next::Stay;
        }
        if self.is(ctx.ui, 0, "buttonNew", id) {
            self.open_new_game(ctx);
        } else if self.is(ctx.ui, 0, "buttonOptions", id) {
            // `FUN_1400b9020` id 4 → `FUN_1400b8ee0`.
            if let Some(screen) = &mut self.screen {
                self.options = crate::ui::options::OptionsDialog::open(ctx, screen);
            }
        } else if self.is(ctx.ui, 0, "buttonLoad", id) {
            if let Some(screen) = &mut self.screen {
                self.save_dialog =
                    crate::ui::save_dialog::SaveLoadDialog::open(ctx.ui, screen, true, false);
            }
        } else if self.is(ctx.ui, 0, "buttonMultiplayer", id) {
            if let Some(screen) = &mut self.screen {
                self.multiplayer = crate::ui::multiplayer::MultiplayerDialog::open(
                    ctx.ui,
                    screen,
                    &ctx.settings.persistence,
                    ctx.archive,
                );
            }
        } else if self.is(ctx.ui, 0, "buttonQuit", id) {
            return Next::Quit;
        } else if self.is(ctx.ui, 0, "buttonTutorial", id)
            || self.is(ctx.ui, 0, "buttonScenario", id)
        {
            // `FUN_1400b9020` ids 12 and 9: the tutorials, or the other scenarios.
            let tutorials = self.is(ctx.ui, 0, "buttonTutorial", id);
            let reference = if tutorials {
                "Dialog/Tutorial.rsc:resource"
            } else {
                "Dialog/Scenario.rsc:resource"
            };
            if let Some(screen) = &mut self.screen {
                let list = crate::gameplay::scenarios(ctx.archive);
                self.scenarios = crate::ui::scenario_ui::ScenarioDialog::open(
                    ctx.ui, screen, reference, tutorials, &list,
                );
            }
        } else if self.is(ctx.ui, 0, "buttonMod", id) {
            if let Some(screen) = &mut self.screen {
                self.mods_dialog = crate::ui::mod_ui::ModDialog::open(
                    ctx.ui,
                    screen,
                    ctx.mods,
                    false,
                    &ctx.settings.persistence,
                    ctx.pack_dir.map(std::path::Path::to_path_buf),
                );
            }
        } else if self.is(ctx.ui, 0, "buttonAchievement", id) {
            // `FUN_1400b9020` id 8: `Dialog/Achievement.rsc`.
            if let Some(screen) = &mut self.screen {
                let list = crate::achievements::load(ctx.archive);
                let awarded = crate::ui::achievement_ui::awarded(&list, &ctx.settings.persistence);
                self.achievements =
                    crate::ui::achievement_ui::AchievementDialog::open(ctx.ui, screen, &list);
                if let Some(a) = &self.achievements {
                    a.refresh(ctx.ui, &awarded);
                }
            }
        } else {
            log::info!("menu: {} is not implemented yet", ctx.ui.node(id).name);
        }
        Next::Stay
    }

    fn screen(&self) -> Option<&Screen> {
        self.screen.as_ref()
    }
}
