//! The town's game as an engine loop: loading, updating, events and the probe's view.

use super::*;

impl GameLoop for GameplayLoop {
    fn name(&self) -> &'static str {
        "Game/Game"
    }

    fn load(&mut self, ctx: &mut Ctx) {
        self.load_world(ctx)
    }

    fn startup(&mut self, ctx: &mut Ctx) {
        self.build_hud(
            ctx.ui,
            ctx.archive,
            ctx.registry,
            &ctx.settings.keymap,
            ctx.settings.game.autosave_interval,
        );
        let persistence = &ctx.settings.persistence;
        // Deviation: in a multiplayer game every player counts the achievements from none won,
        // so every player's game (and its digest) is the same whatever each has won before; one
        // won is still kept in the player's own settings.
        let multiplayer = self.in_lockstep();
        self.sim
            .load_achievements(ctx.archive, self.saved.as_deref(), |title| {
                let mut v = false;
                persistence.get_bool("Achievement", title, &mut v);
                v && !multiplayer
            });
        // Every computer loads the same hooks at the same point (multiplayer compares them).
        crate::platform::rule_hooks::install(&mut self.sim);
        self.start_scenario(ctx);
        self.start_online(ctx);
        self.start_recording();
    }

    fn update(&mut self, ctx: &mut Ctx, dt: f32) -> Next {
        self.update_frame(ctx, dt)
    }

    /// The original's window handlers act inside the UI event, paused or not: WorkPlaceUI's
    /// (`FUN_14008ccb0`) calls `FUN_1400925d0`/`FUN_1400918a0`, `FUN_140096650` and
    /// `FUN_14008cae0` → `FUN_140134fd0` there and then, as BuildUI's pause (`FUN_1400b7370`)
    /// and ProfessionUI's spinner (`FUN_14007b890`) do. The port queues the commands; applying
    /// them after the event is the order the next iteration would apply them in.
    fn event(&mut self, ctx: &mut Ctx, event: Event) -> Next {
        let next = self.window_event(ctx, event);
        if self
            .sim
            .town
            .as_ref()
            .is_some_and(|t| !t.construction.commands.is_empty())
        {
            let flush = PlayerCommand::ApplyQueued {
                with_calendar: true,
            };
            self.execute(ctx.archive, flush);
        }
        next
    }

    fn raw(&mut self, ctx: &mut Ctx, input: RawInput) -> Next {
        self.raw_input(ctx, input)
    }

    fn screen(&self) -> Option<&Screen> {
        self.screen.as_ref()
    }

    /// Capture scripts: `event KIND [CELL]` posts an event at severity 3; `open TEMPLATE`
    /// toggles a utility window.
    fn script(&mut self, ctx: &mut Ctx, command: &str) -> bool {
        self.run_script(ctx, command)
    }

    /// `FUN_14009b850`: the UI's cursor (`UserInterface::Update` → `GetMouseCursorIndex`)
    /// while it holds the pointer (`+0xd9`) or pauses the game; otherwise the tool's (slot
    /// `+0x60` through `FUN_140107b70`): EditTool 1 (Hand, `FUN_14010ad30`), PlacementTool and
    /// every DragTool 5 (Place, `FUN_140103cd0`, `FUN_140101ea0`), PathTool 4 (Query,
    /// `FUN_1400e3de0`).
    fn cursor(&self, ui: &Ui, (x, y): (i32, i32)) -> usize {
        if ui.pointer_over_interactive() || self.pause_menu.is_some() {
            return self
                .screen
                .as_ref()
                .map_or(0, |s| s.cursor_at(ui, x, y).max(0) as usize);
        }
        if self.placement.is_some() || self.removal.is_some() {
            5
        } else if self.path_tool.is_some() {
            4
        } else {
            1
        }
    }

    fn take_screenshot(&mut self) -> Option<bool> {
        self.screenshot.take()
    }

    fn wants_save_preview(&self) -> bool {
        self.pending_save
            .as_ref()
            .is_some_and(|(_, _, i)| i.is_none())
    }

    fn save_preview(&mut self, image: Vec<u8>, minimap: Option<(u32, Vec<u32>)>) {
        if let Some((_, _, slot)) = &mut self.pending_save {
            *slot = Some(image);
            self.save_minimap = minimap;
        }
    }

    fn view(&self) -> View<'_> {
        self.view.as_ref().map_or(View::None, View::World)
    }

    #[cfg(feature = "probe")]
    fn probe_world(
        &self,
        what: &str,
        filter: Option<&str>,
    ) -> Option<Result<serde_json::Value, String>> {
        self.world_query(what, filter)
    }

    #[cfg(all(feature = "cinematic", not(target_arch = "wasm32")))]
    fn town(&self) -> Option<&StartingTown> {
        self.sim.town.as_ref()
    }
}
