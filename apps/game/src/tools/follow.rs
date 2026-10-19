//! The Follow dialog (`Dialog/Follow.rsc`, vtable `0x140161f40`), opened by a citizen window's
//! `buttonFollow` (CitizenUI event `0x12` id `0x228`, `FUN_140065080`). The citizen window
//! keeps it (`+0xe0`) and closes it when it closes itself.
//!
//! - **Bind** (`FUN_140069ca0`): `buttonClose` gets id 100; `textFollow`'s parameter 0 is the
//!   citizen's name; `textAction` is kept; the tool manager's `+0x38` is set (the camera then
//!   zooms with the wheel wherever the pointer is, `FUN_140104390`). Closing clears it
//!   (`FUN_140069c80`).
//! - **Layout** (`FUN_1400698d0`): `textFollow`'s parameter 1 is what the citizen is, as the
//!   citizen window shows it (a child, a student, or the profession), and `textAction` the
//!   activity string (`StringTable.rsc:citizenActions`).
//! - **Update** (`FUN_140069bb0`): the camera's target is the citizen's model position (x, y,
//!   0); Escape (action 8) closes it (`FUN_140069860`).
//! - **Event** (`FUN_140069b90`): a click on id 100 closes it.

use banished_ui::{NodeId, Ui};

use crate::loops::Screen;
use crate::start::StartingTown;
use crate::ui::utility::{set_parameters, text_of};

const DIALOG: &str = "Dialog/Follow.rsc:resource";

pub struct Follow {
    pub citizen: usize,
    /// The citizen window that opened it.
    pub owner: NodeId,
    root: NodeId,
    close: Option<NodeId>,
    text: Option<(NodeId, String)>,
    action: Option<NodeId>,
}

impl Follow {
    /// `FUN_140065080` → `FUN_140069ca0`.
    pub fn open(
        ui: &mut Ui,
        screen: &mut Screen,
        town: &StartingTown,
        citizen: usize,
        owner: NodeId,
    ) -> Option<Follow> {
        let index = screen.add_dialog(ui, DIALOG)?;
        let root = screen.dialogs[index].root;
        let text = ui.find(root, "textFollow").map(|id| (id, text_of(ui, id)));
        let f = Follow {
            citizen,
            owner,
            root,
            close: ui.find(root, "buttonClose"),
            text,
            action: ui.find(root, "textAction"),
        };
        f.layout(ui, town);
        Some(f)
    }

    /// `FUN_1400698d0`.
    pub fn layout(&self, ui: &mut Ui, town: &StartingTown) {
        let Some(info) = crate::ui::entity_ui::citizen_info(town, self.citizen) else {
            return;
        };
        if let Some(text) = &self.text {
            let what = info
                .profession
                .as_ref()
                .map(|(table, key)| ui.res.string(table.as_deref(), key))
                .unwrap_or_default();
            set_parameters(ui, text, &[info.name.clone(), what]);
        }
        if let Some(n) = self.action {
            let t = ui
                .res
                .string_at(r"Dialog\StringTable.rsc:citizenActions", info.activity);
            ui.set_text(n, &t);
        }
    }

    /// `FUN_140069bb0`'s camera target.
    pub fn target(&self, town: &StartingTown) -> Option<glam::Vec2> {
        let c = town.citizens.get(self.citizen).filter(|c| c.alive)?;
        Some(glam::Vec2::new(c.position[0], c.position[1]))
    }

    /// Whether `node` is the dialog's close button (`FUN_140069b90`, id 100).
    pub fn is_close(&self, node: NodeId) -> bool {
        self.close == Some(node)
    }

    /// `FUN_140069860`.
    pub fn close(self, ui: &mut Ui, screen: &mut Screen) {
        screen.remove_root(ui, self.root);
    }
}
