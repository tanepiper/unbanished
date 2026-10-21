//! A build without the `multiplayer` feature: the main menu has no Multiplayer button, and the
//! Multiplayer window never opens. The menu's code stays as it is; this stands in for the
//! window's type, which has no value.

use unbanished_ui::{NodeId, Ui};

use crate::gameplay::GameplayLoop;
use crate::loops::Screen;
use crate::settings::Persistence;

/// What the window asks the menu to do (never asked here; the same as the real window's).
#[allow(dead_code)]
pub enum Outcome {
    Stay,
    Closed,
    ChooseSave,
    Start(Box<GameplayLoop>),
}

/// The Multiplayer window, which a build without multiplayer never has.
pub enum MultiplayerDialog {}

impl MultiplayerDialog {
    /// Never opens.
    pub fn open(
        _ui: &mut Ui,
        _screen: &mut Screen,
        _settings: &Persistence,
        _archive: &unbanished_formats::pkg::Archive,
    ) -> Option<Self> {
        None
    }

    pub fn click(
        &mut self,
        _ui: &mut Ui,
        _screen: &mut Screen,
        _settings: &mut Persistence,
        _node: NodeId,
    ) -> Outcome {
        match *self {}
    }

    pub fn update(&mut self, _ui: &mut Ui, _screen: &mut Screen) -> Outcome {
        match *self {}
    }

    pub fn host_start(&mut self, _ui: &mut Ui, _screen: &mut Screen, _bytes: Vec<u8>) -> Outcome {
        match *self {}
    }

    pub fn close(&mut self, _ui: &mut Ui, _screen: &mut Screen) {
        match *self {}
    }
}

/// No Multiplayer button.
pub fn add_menu_button(_ui: &mut Ui, _screen: &Screen, _dialog: usize) {}
