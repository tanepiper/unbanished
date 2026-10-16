//! The pause menu's Map dialog (`Dialog/MapParams.rsc`, `MapParamsDialog`, vtable
//! `0x1401622c0`), opened by the Map button (id 11, `FUN_1400b85f0`) when no scenario is played.
//!
//! - Open (slot `+0x20`, `FUN_14006c170`): `editName` gets the town's name (`Game +0x28`),
//!   `numberSeed` the seed (`+0x30`), `textCondition`, `textTerrain` and `textClimate` the
//!   start condition's, terrain's and climate's names (`+0x40`, `+0x48`, `+0x50`),
//!   `textTerrainSize` entry `+0x34` of its string list (`terrainSize`) and `textDisaster` entry
//!   `Game +0xd0` bit 6 of its (`disaster`: 0 on, 1 off). `buttonNewName` is id 0, `buttonOk` 1
//!   and `buttonCancel` 2, with action 8 (Escape).
//! - Update (slot `+0x30`, `FUN_14006c0f0`): OK is enabled while the name is not empty.
//! - Click (slot `+0x40`, `FUN_14006bf50`): New Name (`FUN_14006c060`) draws a town name of up to
//!   `(max length / 3) × 2` characters on the game's Random (`Game +0xd8`, `FUN_14009ef90`); OK
//!   (`FUN_14006bff0`) renames the town and closes; Cancel closes.

use banished_formats::names::NameDatabase;
use banished_game::random::Random;
use banished_ui::{Control, Kind, NodeId, Ui};

use crate::loops::Screen;
use crate::world::WorldParams;

const DIALOG: &str = "Dialog/MapParams.rsc:resource";

/// What a click on the dialog asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MapParamsClick {
    /// OK: the town's new name.
    Rename(String),
    Close,
    /// Handled here (New Name), or not one of its buttons.
    None,
}

pub struct MapParamsDialog {
    pub root: NodeId,
    name: Option<NodeId>,
    new_name: Option<NodeId>,
    ok: Option<NodeId>,
    cancel: Option<NodeId>,
}

fn edit_text(ui: &Ui, edit: NodeId) -> String {
    match &ui.node(edit).kind {
        Kind::Control(Control::Edit(e)) => e.text.text.clone(),
        _ => String::new(),
    }
}

impl MapParamsDialog {
    /// `FUN_1400b85f0` → `FUN_14006c170`.
    pub fn open(
        ui: &mut Ui,
        screen: &mut Screen,
        params: &WorldParams,
        game_flags: u32,
    ) -> Option<MapParamsDialog> {
        let index = screen.add_modal(ui, DIALOG)?;
        let root = screen.dialogs[index].root;
        let find = |ui: &Ui, n: &str| ui.find(root, n);
        let name = find(ui, "editName");
        if let Some(e) = name {
            ui.set_text(e, &params.town_name);
            if let Kind::Control(Control::Edit(d)) = &mut ui.node_mut(e).kind {
                d.cursor = d.text.text.chars().count();
            }
        }
        if let Some(n) = find(ui, "numberSeed") {
            ui.set_text(n, &params.seed.to_string());
        }
        for (element, text) in [
            ("textCondition", &params.start_condition),
            ("textTerrain", &params.terrain),
            ("textClimate", &params.climate),
        ] {
            if let Some(n) = find(ui, element) {
                ui.set_text(n, text);
            }
        }
        for (element, list, index) in [
            ("textTerrainSize", "terrainSize", params.size as i32),
            ("textDisaster", "disaster", (game_flags >> 6 & 1) as i32),
        ] {
            if let Some(n) = find(ui, element) {
                let text = ui
                    .res
                    .string_at(&format!(r"Dialog\StringTable.rsc:{list}"), index);
                ui.set_text(n, &text);
            }
        }
        let d = MapParamsDialog {
            root,
            name,
            new_name: find(ui, "buttonNewName"),
            ok: find(ui, "buttonOk"),
            cancel: find(ui, "buttonCancel"),
        };
        d.update(ui);
        Some(d)
    }

    /// `FUN_14006c0f0`: OK only with a name.
    pub fn update(&self, ui: &mut Ui) {
        let named = self.name.is_some_and(|e| !edit_text(ui, e).is_empty());
        if let Some(ok) = self.ok {
            ui.node_mut(ok).enabled = named;
        }
    }

    /// `FUN_14006bf50`.
    pub fn click(
        &self,
        ui: &mut Ui,
        node: NodeId,
        names: Option<&NameDatabase>,
        rng: &mut Random,
    ) -> MapParamsClick {
        let is = |b: Option<NodeId>| b.is_some_and(|b| ui.descendant(b, node));
        if is(self.new_name) {
            // `FUN_14006c060`.
            if let (Some(e), Some(db)) = (self.name, names) {
                let max_length = match &ui.node(e).kind {
                    Kind::Control(Control::Edit(d)) => d.max_length as i32,
                    _ => 30,
                };
                let name = banished_game::names::town_name(db, rng, max_length / 3 * 2);
                ui.set_text(e, &name);
                self.update(ui);
            }
            MapParamsClick::None
        } else if is(self.ok) {
            MapParamsClick::Rename(self.name.map(|e| edit_text(ui, e)).unwrap_or_default())
        } else if is(self.cancel) {
            MapParamsClick::Close
        } else {
            self.update(ui);
            MapParamsClick::None
        }
    }
}
