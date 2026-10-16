//! The in-game pause menu: `Dialog/Pause.rsc` as a `MainMenuDialog` bound to the game.
//!
//! - `FUN_1401071b0` opens it: toolbar Options (kind 3, fixed action 8 = Escape), or the
//!   controller `FUN_140107b70` when the application is paused (window unfocused) and the game
//!   option `+0x18` (default on) is set.
//! - `FUN_1400b91a0` (`MainMenuDialog` open, slot `+0x20`): with a game (`+0xb8`) it pauses the
//!   game scene's effects, ambience and music (`Scene::Pause` types 4, 3, 2). `buttonResume`
//!   gets action 8, so Escape resumes.
//! - `FUN_1400b9130` (close, slot `+0x28`): resumes types 4, 3, 2.
//! - `FUN_1400b9020` (click, event 0x12): element id 1 Resume closes the dialog, 5 Menu returns
//!   to the main menu (`FUN_1400b9c60`), 6 Quit quits.
//! - The dialog's flags (`Dialog +0x40` = 0x13) have bit 0 set, so `UserInterface::PausesGame`
//!   holds while it is open: `FUN_14009b850` skips the simulation and the controller.
//!
//! Options (4) opens `crate::options::OptionsDialog`, Achievement (8) the achievements and
//! Goal (10) the game's goal. With a scenario (`Game +0x15c`) the Goal button shows and the Map
//! button goes, else the reverse; game flag bit 10 (a tutorial) disables Save and Mod. Map (11)
//! opens `crate::map_params::MapParamsDialog`. Not ported: Load (2), Save (3) and Mod (7).

use banished_ui::{NodeId, Ui};

use crate::loops::Screen;
use crate::sound::{sound_type, Audio};

const DIALOG: &str = "Dialog/Pause.rsc:resource";

/// What a click on the menu asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PauseClick {
    Resume,
    Menu,
    Quit,
    /// Options (id 4): `FUN_1400b8ee0` opens the Options dialog over the menu.
    Options,
    Achievement,
    Goal,
    /// Map (id 11, `FUN_1400b85f0`).
    Map,
    /// A button whose action is not ported, or a click on the dialog's background.
    Other,
}

pub struct PauseMenu {
    root: NodeId,
}

impl PauseMenu {
    /// `FUN_1401071b0` → `FUN_1400b91a0`.
    pub fn open(
        ui: &mut Ui,
        screen: &mut Screen,
        audio: &mut Audio,
        scenario: bool,
        game_flags: u32,
    ) -> Option<PauseMenu> {
        let index = screen.add_modal(ui, DIALOG)?;
        let root = screen.dialogs[index].root;
        let hide = if scenario { "buttonMap" } else { "buttonGoal" };
        if let Some(b) = ui.find(root, hide) {
            ui.set_visible(b, false);
        }
        if game_flags & 0x400 != 0 {
            for b in ["buttonSave", "buttonMod"] {
                if let Some(b) = ui.find(root, b) {
                    ui.node_mut(b).enabled = false;
                }
            }
        }
        for kind in [sound_type::EFFECT, sound_type::AMBIENT, sound_type::MUSIC] {
            audio.pause(kind, true);
        }
        Some(PauseMenu { root })
    }

    /// `UserInterface::CloseDialog` → `FUN_1400b9130`.
    pub fn close(self, ui: &mut Ui, screen: &mut Screen, audio: &mut Audio) {
        screen.remove_root(ui, self.root);
        for kind in [sound_type::EFFECT, sound_type::AMBIENT, sound_type::MUSIC] {
            audio.pause(kind, false);
        }
    }

    pub fn contains(&self, ui: &Ui, node: NodeId) -> bool {
        ui.descendant(self.root, node)
    }

    /// `FUN_1400b9020`: the clicked button's element id.
    pub fn click(&self, ui: &Ui, node: NodeId) -> PauseClick {
        let is = |name: &str| {
            ui.find(self.root, name)
                .is_some_and(|b| ui.descendant(b, node))
        };
        if is("buttonResume") {
            PauseClick::Resume
        } else if is("buttonMenu") {
            PauseClick::Menu
        } else if is("buttonQuit") {
            PauseClick::Quit
        } else if is("buttonOptions") {
            PauseClick::Options
        } else if is("buttonAchievement") {
            PauseClick::Achievement
        } else if is("buttonGoal") {
            PauseClick::Goal
        } else if is("buttonMap") {
            PauseClick::Map
        } else {
            PauseClick::Other
        }
    }
}
