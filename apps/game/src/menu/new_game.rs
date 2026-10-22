//! The new game dialog: its name, seed and description, and starting the game.

use super::*;

impl MenuLoop {
    /// `FUN_140075ba0`: fill the selectors from the global lists (listed entries only, labelled
    /// from their string tables), apply the default selections and show the condition text.
    pub(super) fn open_new_game(&mut self, ctx: &mut Ctx) {
        let Some(screen) = &mut self.screen else {
            return;
        };
        let Some(index) = screen.add_modal(ctx.ui, NEW_GAME_DIALOG) else {
            return;
        };
        self.new_game = Some(index);
        if let Some(global) = ctx.global.as_ref() {
            fn fill<T>(
                ui: &mut Ui,
                combo: Option<NodeId>,
                choices: &[crate::global::Choice<T>],
            ) -> Vec<usize> {
                let mut map = Vec::new();
                let mut labels = Vec::new();
                for (i, c) in choices.iter().enumerate() {
                    if c.listed {
                        map.push(i);
                        labels.push(ui.res.string(c.label.0.as_deref(), &c.label.1));
                    }
                }
                if let Some(combo) = combo {
                    ui.set_choices(combo, labels);
                }
                map
            }
            let find = |ui: &Ui, name| screen.find(ui, index, name);
            let combo = find(ctx.ui, "comboClimate");
            self.climate_items = fill(ctx.ui, combo, &global.climates);
            let combo = find(ctx.ui, "comboCondition");
            self.condition_items = fill(ctx.ui, combo, &global.start_conditions);
            let combo = find(ctx.ui, "comboTerrain");
            self.terrain_items = fill(ctx.ui, combo, &global.terrains);
        }
        for (name, default) in DEFAULT_SELECTION {
            if let Some(combo) = screen.find(ctx.ui, index, name) {
                select_combo(ctx.ui, combo, default);
            }
        }
        self.new_seed(ctx);
        self.new_town_name(ctx);
        self.refresh_description(ctx);
    }

    /// The Load dialog's answer: a finished load starts that game on the next update.
    pub(super) fn save_outcome(&mut self, outcome: crate::ui::save_dialog::Outcome) {
        use crate::ui::save_dialog::Outcome;
        match outcome {
            Outcome::Closed => self.save_dialog = None,
            Outcome::Load(saved) => {
                self.save_dialog = None;
                self.load = Some(saved);
            }
            Outcome::Stay | Outcome::Save(_) => {}
        }
    }

    pub(super) fn new_town_name(&mut self, ctx: &mut Ctx) {
        let (Some(screen), Some(d)) = (&self.screen, self.new_game) else {
            return;
        };
        let Some(edit) = screen.find(ctx.ui, d, "editName") else {
            return;
        };
        let Some(db) = &self.town_names else {
            log::warn!("town names unavailable: Game/Names/Cities.ndb did not load");
            return;
        };
        // `FUN_140075ba0`: `FUN_14009ef90(rng, (editName max length / 3) · 2)`. The dialog's
        // own clock-seeded stream (`+0x118`) is the engine default `Random()` here.
        let max_length = match &ctx.ui.node(edit).kind {
            unbanished_ui::Kind::Control(unbanished_ui::Control::Edit(e)) => e.max_length as i32,
            _ => 30,
        };
        let name = unbanished_game::people::names::town_name(db, ctx.rng, max_length / 3 * 2);
        ctx.ui.set_text(edit, &name);
    }

    /// Event `0x14` on `comboCondition` (`FUN_140075670`): show the selected condition's
    /// description.
    pub(super) fn refresh_description(&mut self, ctx: &mut Ctx) {
        let (Some(screen), Some(d), Some(global)) =
            (&self.screen, self.new_game, ctx.global.as_ref())
        else {
            return;
        };
        let selected = screen
            .find(ctx.ui, d, "comboCondition")
            .map_or(0, |c| combo_index(ctx.ui, c));
        let Some(condition) = self
            .condition_items
            .get(selected)
            .and_then(|&i| global.start_conditions.get(i))
        else {
            return;
        };
        let text = ctx
            .ui
            .res
            .string(condition.data.list.as_deref(), &condition.data.description);
        if let Some(desc) = screen.find(ctx.ui, d, "textDescription") {
            ctx.ui.set_text(desc, &text);
        }
    }

    pub(super) fn new_seed(&mut self, ctx: &mut Ctx) {
        let (Some(screen), Some(d)) = (&self.screen, self.new_game) else {
            return;
        };
        if let Some(edit) = screen.find(ctx.ui, d, "editSeed") {
            // The original draws within the edit box's numeric range; that range is not yet decoded.
            let seed = ctx.rng.rand() & 0x7fff_ffff;
            ctx.ui.set_text(edit, &seed.to_string());
        }
    }

    pub(super) fn close_new_game(&mut self, ctx: &mut Ctx) {
        if let (Some(screen), Some(d)) = (&mut self.screen, self.new_game.take()) {
            screen.remove_dialog(ctx.ui, d);
        }
    }

    /// `FUN_140075750`: read the selections and start loading the game.
    pub(super) fn start_game(&mut self, ctx: &mut Ctx) -> Next {
        let (Some(screen), Some(d)) = (&self.screen, self.new_game) else {
            return Next::Stay;
        };
        let terrain_index = screen
            .find(ctx.ui, d, "comboTerrain")
            .map_or(0, |c| combo_index(ctx.ui, c));
        let size_index = screen
            .find(ctx.ui, d, "comboTerrainSize")
            .map_or(0, |c| combo_index(ctx.ui, c));
        let climate_index = screen
            .find(ctx.ui, d, "comboClimate")
            .map_or(0, |c| combo_index(ctx.ui, c));
        let condition_index = screen
            .find(ctx.ui, d, "comboCondition")
            .map_or(0, |c| combo_index(ctx.ui, c));
        let terrain = self
            .terrain_items
            .get(terrain_index)
            .and_then(|&i| ctx.global.as_ref()?.terrains.get(i));
        if let Some(t) = terrain {
            log::info!("terrain profile {}", t.reference);
        }
        let terrain = terrain.map_or_else(|| "Valleys".to_string(), |t| t.name.clone());
        let town_name = screen
            .find(ctx.ui, d, "editName")
            .and_then(|e| edit_text(ctx.ui, e))
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| "New Settlement".to_string());
        let climate_choice = self
            .climate_items
            .get(climate_index)
            .and_then(|&i| ctx.global.as_ref()?.climates.get(i));
        let climate_ref = climate_choice
            .map(|choice| choice.reference.clone())
            .unwrap_or_else(|| WorldParams::default().climate_ref);
        let climate = climate_choice
            .map(|choice| {
                ctx.ui
                    .res
                    .string(choice.label.0.as_deref(), &choice.label.1)
            })
            .unwrap_or_else(|| "Fair".to_string());
        let condition_choice = self
            .condition_items
            .get(condition_index)
            .and_then(|&i| ctx.global.as_ref()?.start_conditions.get(i));
        let start_condition_ref = condition_choice
            .map(|choice| choice.reference.clone())
            .unwrap_or_default();
        let start_condition = condition_choice
            .map(|choice| {
                ctx.ui
                    .res
                    .string(choice.data.list.as_deref(), &choice.data.name)
            })
            .unwrap_or_else(|| "Medium".to_string());
        let seed = screen
            .find(ctx.ui, d, "editSeed")
            .and_then(|e| edit_text(ctx.ui, e))
            .and_then(|t| t.trim().parse::<i64>().ok())
            .unwrap_or(0) as u32;
        // Scripted runs: `UNBANISHED_SEED` fixes the world seed.
        let seed = unbanished_formats::env::var("SEED")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(seed);
        // `FUN_140075750`: disasters are on with the combo's first item.
        let disasters = screen
            .find(ctx.ui, d, "comboDisaster")
            .map_or(0, |c| combo_index(ctx.ui, c))
            == 0;
        let params = WorldParams {
            disasters,
            town_name,
            terrain,
            climate,
            climate_ref,
            start_condition,
            start_condition_ref,
            size: size_index,
            seed,
            scenario: None,
        };
        log::info!(
            "new game: {} / {} / {} terrain {} size {} seed {}",
            params.town_name,
            params.climate,
            params.start_condition,
            params.terrain,
            params.size,
            params.seed
        );
        // `FUN_14009e600`: one of twelve loading screens at random.
        let screen_ref = format!("Dialog/Loading.rsc:gameLoad{}", ctx.rng.rand_below(12));
        Next::Switch(Box::new(DialogLoaderLoop::new(
            screen_ref,
            0.5,
            Box::new(crate::gameplay::GameplayLoop::new(params)),
        )))
    }
}
