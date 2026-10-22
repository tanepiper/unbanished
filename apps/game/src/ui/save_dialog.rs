//! The Save and Load dialog: `Dialog/SaveLoad.rsc` (vtable `0x140165d10`). Native builds.
//!
//! - **Opening** (constructor `FUN_1400bd740`, bind `FUN_1400bd1d0`): `+0xb8` is the mode (1
//!   Load, 0 Save), `+0xc0` the selection (0). Its buttons get ids Save 1, Load 2, Cancel 0
//!   (action 8, so Escape closes), Delete 3. It asks `SaveLoad` for the list (operation 1,
//!   argument −2 saving or −3 loading, at least 0.75 s; [`crate::ui::save_list::list`]).
//! - **Each frame** (`FUN_1400bce80`): while an operation runs the spinner turns 180° a second.
//!   When none runs it finishes what was started: a new list fills the table (`+0xbb`), a save
//!   (`+0xb9`) closes the dialog, a load (`+0xba`) closes it and starts the game, a delete
//!   (`+0xbc`) lists again.
//! - **Refresh** (`FUN_1400bbd50`): `labelLoad`/`buttonLoad` show when loading,
//!   `labelSave`/`buttonSave` when saving. With nothing running `mainPage` shows the list (page
//!   1): Load is usable on a readable save (status 2), the selected save has its highlight
//!   (`selectSave`) instead of its border. While one runs it shows page 0 with the spinner and
//!   "Please wait" (listing), "Loading" or "Saving", and the buttons are not usable.
//! - **The table** (`FUN_1400bc1b0`): `max(count, 6) + 2` rounded down to threes cells of the
//!   `icon` template, three a row, plus a row. A save shows its preview (`imageSaveIcon`,
//!   which takes the mouse with id `4 + index`), its name and its date, or "Corrupt" and a
//!   dimmed preview (`0xff808080`) with `imageCorrupt` when it does not read. Unused cells
//!   show only a faint border (`0x80ffffff`) and icon (`0x20ffffff`).
//! - **Events** (`FUN_1400bbcb0`): event 2 on an icon selects it (UI sound 4 when it changes,
//!   `FUN_1400bbc60`); Cancel closes; Save (`FUN_1400bb4a0`) asks `confirmSave` over an existing
//!   file and saves (operation 4, at least 1.5 s) under the town's name, into a new
//!   `Save%i` for the new-save slot; Load (`FUN_1400bb6b0`) asks `confirmLoad` while a game
//!   runs, then for each mod the save names that `SaveLoad::CheckMod` finds missing (1),
//!   out of date (2), of another version (3) or changed (4) `modsMissing`, `modsOutOfDate`,
//!   `modsVersion` or `modsChanged` with its name, file and `"%i (%s)"` version, and loads (operation 2, at least 1.5 s); Delete (`FUN_1400bb320`) asks
//!   `confirmDelete` and deletes (operation 5).
//! - The saves are read and written through [`crate::saves::save_store`]: a load waits for the
//!   file's bytes as well as its shortest time (in the browser the read is asynchronous).
//! - The confirmations are message boxes (vtable `0x1401625c0`, `FUN_14006daa0`: Cancel 0,
//!   Ok 1, Yes 2, No 3); `ShowDialog` returns the button and only Yes goes on.
//!
//! Deviations: the original runs `ShowDialog` as a nested loop; the port keeps the step waiting
//! for the answer.
//!
//! The new-save slot shows the save sprite (`SaveLoad +0x60`, set from the global data's
//! `+0xb8`: `Dialog\SpriteSheet.rsc` "SaveLoadEmpty", `FUN_1400a0e00`), which is the icon
//! template's own sprite, so the slot keeps it.

mod files;
mod list;

use unbanished_ui::{Control, Kind, NodeId, Ui};

use crate::loops::Screen;
use crate::ui::save_list::{self, SaveInfo};

const DIALOG: &str = "Dialog/SaveLoad.rsc:resource";
const CELL: &str = r"Dialog\SaveLoad.rsc:icon";
/// The game's names table (`Game +0xd8`).
pub const GLOBAL_NAMES: &str = "Dialog/StringTable.rsc:globalNames";
/// `DAT_140178824`, `DAT_140178860`: an operation's shortest time.
const LIST_TIME: f32 = 0.75;
const SAVE_LOAD_TIME: f32 = 1.5;
/// `DAT_140178920`: the spinner's degrees a second.
const SPIN: f32 = 180.0;

/// `SaveLoad::Operation`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Operation {
    List = 1,
    Load = 2,
    Save = 4,
    Delete = 5,
}

/// A confirmation waiting for its answer, and what Yes does.
enum Confirm {
    Save,
    /// Loading: the next mod to ask about.
    Load(usize),
    Delete,
}

struct Cell {
    border: Option<NodeId>,
    select: Option<NodeId>,
    icon: Option<NodeId>,
}

pub struct SaveLoadDialog {
    pub root: NodeId,
    /// `+0xb8`.
    loading: bool,
    /// `+0xc0`.
    selected: i32,
    saves: Vec<SaveInfo>,
    cells: Vec<Cell>,
    /// The operation running and the time left of its shortest time.
    operation: Option<(Operation, f32)>,
    /// `+0xb9`, `+0xba`, `+0xbb`, `+0xbc`: a save, load, list or delete was started.
    saved: bool,
    load: Option<crate::saves::saved::SavedGame>,
    /// The file being read for a load.
    reading: Option<crate::saves::save_store::Read>,
    listing: bool,
    deleting: bool,
    /// `+200`: the spinner's angle.
    angle: f32,
    /// Whether a game is running (`DAT_1401cbdd8`): loading asks first.
    in_game: bool,
    confirm: Option<(Confirm, NodeId)>,
    el: Elements,
    /// Keep the file of the save loaded (a multiplayer host sends it to the players).
    pub keep_bytes: bool,
    pub bytes: Option<Vec<u8>>,
}

#[derive(Default)]
struct Elements {
    label_load: Option<NodeId>,
    label_save: Option<NodeId>,
    save: Option<NodeId>,
    load: Option<NodeId>,
    cancel: Option<NodeId>,
    delete: Option<NodeId>,
    page: Option<NodeId>,
    loading: Option<NodeId>,
    saving: Option<NodeId>,
    wait: Option<NodeId>,
    spinner: Option<NodeId>,
    table: Option<NodeId>,
}

/// What the dialog asks of its owner.
pub enum Outcome {
    Stay,
    Closed,
    /// Write the game as the save `name` (operation 4 started); it is named after the town
    /// (`SetSaveName` with the string at `+0x28` of the dialog's `+0xe8`, see
    /// `GameplayLoop::save_file`).
    Save(String),
    /// Start this game (the load finished; the dialog has closed).
    Load(crate::saves::saved::SavedGame),
}

impl SaveLoadDialog {
    /// `loading`: the Load dialog, else the Save dialog. `in_game`: over a running game.
    pub fn open(ui: &mut Ui, screen: &mut Screen, loading: bool, in_game: bool) -> Option<Self> {
        let index = screen.add_modal(ui, DIALOG)?;
        let root = screen.dialogs[index].root;
        let f = |n: &str| ui.find(root, n);
        let el = Elements {
            label_load: f("labelLoad"),
            label_save: f("labelSave"),
            save: f("buttonSave"),
            load: f("buttonLoad"),
            cancel: f("buttonCancel"),
            delete: f("buttonDelete"),
            page: f("mainPage"),
            loading: f("textLoading"),
            saving: f("textSaving"),
            wait: f("textPleaseWait"),
            spinner: f("loadingImage"),
            table: f("tableSaveList"),
        };
        let mut d = SaveLoadDialog {
            root,
            loading,
            selected: 0,
            saves: Vec::new(),
            cells: Vec::new(),
            operation: None,
            saved: false,
            load: None,
            reading: None,
            listing: false,
            deleting: false,
            angle: 0.0,
            in_game,
            confirm: None,
            el,
            keep_bytes: false,
            bytes: None,
        };
        d.start(Operation::List, LIST_TIME);
        d.listing = true;
        d.refresh(ui);
        Some(d)
    }

    /// Whether its save operation (4) runs: the autosave dialog shows its spinner too.
    pub fn saving(&self) -> bool {
        matches!(self.operation, Some((Operation::Save, _)))
    }

    fn start(&mut self, operation: Operation, time: f32) {
        self.operation = Some((operation, time));
    }

    fn close(&mut self, ui: &mut Ui, screen: &mut Screen) {
        if let Some((_, c)) = self.confirm.take() {
            screen.remove_root(ui, c);
        }
        screen.remove_root(ui, self.root);
    }

    /// `FUN_1400bce80`, then the refresh `FUN_1400bbd50`.
    pub fn update(&mut self, ui: &mut Ui, screen: &mut Screen, dt: f32) -> Outcome {
        if let Some(read) = &mut self.reading {
            if let Some(result) = read.poll() {
                self.reading = None;
                let keep = self.keep_bytes;
                match result.and_then(|b| {
                    let saved = crate::saves::saved::SavedGame::parse(&b);
                    saved
                        .map(|s| (s, keep.then_some(b)))
                        .map_err(|e| e.to_string())
                }) {
                    Ok((saved, bytes)) => {
                        self.load = Some(saved);
                        self.bytes = bytes;
                    }
                    Err(e) => {
                        log::error!("load: {e}");
                        self.operation = None;
                    }
                }
            }
        }
        if let Some((_, left)) = &mut self.operation {
            self.angle += dt * SPIN;
            *left -= dt;
            if *left <= 0.0 && self.reading.is_none() {
                self.operation = None;
            }
        }
        let mut outcome = Outcome::Stay;
        if self.operation.is_none() {
            if self.deleting {
                self.deleting = false;
                self.listing = true;
                self.start(Operation::List, LIST_TIME);
            } else if self.listing {
                self.saves = save_list::list(!self.loading);
                let count = self.saves.len() as i32;
                if count <= self.selected {
                    self.selected = count - 1;
                }
                self.fill(ui);
                self.listing = false;
            } else if self.saved {
                self.close(ui, screen);
                return Outcome::Closed;
            } else if let Some(saved) = self.load.take() {
                self.close(ui, screen);
                outcome = Outcome::Load(saved);
            }
        }
        if !matches!(outcome, Outcome::Load(_)) {
            self.refresh(ui);
        }
        outcome
    }

    /// `FUN_1400bbcb0` for a click, and the confirmations' answers.
    pub fn click(
        &mut self,
        ui: &mut Ui,
        screen: &mut Screen,
        audio: &mut crate::audio::sound::Audio,
        archive: &unbanished_formats::pkg::Archive,
        mods: &crate::mods::Modification,
        node: NodeId,
    ) -> Outcome {
        if let Some((confirm, root)) = self.confirm.take() {
            let yes = ui
                .find(root, "buttonYes")
                .is_some_and(|b| ui.descendant(b, node));
            let answered = ["buttonYes", "buttonNo", "buttonOk", "buttonCancel"]
                .iter()
                .any(|b| ui.find(root, b).is_some_and(|b| ui.descendant(b, node)));
            if !answered {
                self.confirm = Some((confirm, root));
                return Outcome::Stay;
            }
            screen.remove_root(ui, root);
            if !yes {
                return Outcome::Stay;
            }
            return match confirm {
                Confirm::Save => self.save(),
                Confirm::Load(next) => self.load(ui, screen, mods, next),
                Confirm::Delete => {
                    self.delete();
                    Outcome::Stay
                }
            };
        }
        if self.operation.is_some() {
            return Outcome::Stay;
        }
        let is = |n: Option<NodeId>| n.is_some_and(|b| ui.descendant(b, node));
        if let Some(i) = self.cells.iter().position(|c| c.icon == Some(node)) {
            // `FUN_1400bbc60`.
            if self.selected != i as i32 {
                audio.play_ui(archive, 4);
            }
            self.selected = i as i32;
        } else if is(self.el.cancel) {
            self.close(ui, screen);
            return Outcome::Closed;
        } else if is(self.el.save) {
            // `FUN_1400bb4a0`.
            if self.selected_file().is_some() {
                self.ask(ui, screen, "confirmSave", Confirm::Save, &[]);
            } else {
                return self.save();
            }
        } else if is(self.el.load) {
            // `FUN_1400bb6b0`.
            if self.in_game {
                self.ask(ui, screen, "confirmLoad", Confirm::Load(0), &[]);
            } else {
                return self.load(ui, screen, mods, 0);
            }
        } else if is(self.el.delete) {
            self.ask(ui, screen, "confirmDelete", Confirm::Delete, &[]);
        }
        Outcome::Stay
    }

    /// Escape: Cancel's action (8).
    pub fn escape(&mut self, ui: &mut Ui, screen: &mut Screen) -> Outcome {
        if let Some((_, root)) = self.confirm.take() {
            screen.remove_root(ui, root);
            return Outcome::Stay;
        }
        if self.operation.is_some() {
            return Outcome::Stay;
        }
        self.close(ui, screen);
        Outcome::Closed
    }

    fn selected_file(&self) -> Option<&str> {
        self.saves
            .get(usize::try_from(self.selected).ok()?)?
            .file
            .as_deref()
    }
}

fn set_page(ui: &mut Ui, page: Option<NodeId>, index: usize) {
    if let Some(n) = page {
        if let Kind::Control(Control::Page { selected, .. }) = &mut ui.node_mut(n).kind {
            *selected = index;
        }
    }
}
