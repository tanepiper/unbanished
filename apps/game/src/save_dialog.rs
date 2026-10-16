//! The Save and Load dialog: `Dialog/SaveLoad.rsc` (vtable `0x140165d10`). Native builds.
//!
//! - **Opening** (constructor `FUN_1400bd740`, bind `FUN_1400bd1d0`): `+0xb8` is the mode (1
//!   Load, 0 Save), `+0xc0` the selection (0). Its buttons get ids Save 1, Load 2, Cancel 0
//!   (action 8, so Escape closes), Delete 3. It asks `SaveLoad` for the list (operation 1,
//!   argument −2 saving or −3 loading, at least 0.75 s; [`crate::save_list::list`]).
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
//! - The saves are read and written through [`crate::save_store`]: a load waits for the
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

use banished_ui::{Control, Kind, NodeId, Ui};

use crate::loops::Screen;
use crate::save_list::{self, SaveInfo};

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
    load: Option<crate::saved::SavedGame>,
    /// The file being read for a load.
    reading: Option<crate::save_store::Read>,
    listing: bool,
    deleting: bool,
    /// `+200`: the spinner's angle.
    angle: f32,
    /// Whether a game is running (`DAT_1401cbdd8`): loading asks first.
    in_game: bool,
    confirm: Option<(Confirm, NodeId)>,
    el: Elements,
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
    Load(crate::saved::SavedGame),
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
                match result
                    .and_then(|b| crate::saved::SavedGame::parse(&b).map_err(|e| e.to_string()))
                {
                    Ok(saved) => self.load = Some(saved),
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

    /// `FUN_1400bbd50`.
    fn refresh(&self, ui: &mut Ui) {
        let e = &self.el;
        let show = |ui: &mut Ui, n: Option<NodeId>, v: bool| {
            if let Some(n) = n {
                ui.set_visible(n, v);
            }
        };
        let usable = |ui: &mut Ui, n: Option<NodeId>, v: bool| {
            if let Some(n) = n {
                ui.node_mut(n).enabled = v;
            }
        };
        show(ui, e.label_load, self.loading);
        show(ui, e.label_save, !self.loading);
        show(ui, e.save, !self.loading);
        show(ui, e.load, self.loading);
        let idle = self.operation.is_none() && !self.listing && !self.deleting && !self.saved;
        if idle && self.load.is_none() {
            set_page(ui, e.page, 1);
            let readable = self
                .saves
                .get(self.selected.max(0) as usize)
                .filter(|_| self.selected >= 0)
                .is_some_and(|s| s.status == 2);
            usable(ui, e.load, readable);
            usable(ui, e.save, true);
            usable(ui, e.cancel, true);
            let file = self
                .saves
                .get(self.selected.max(0) as usize)
                .is_some_and(|s| s.file.is_some());
            usable(ui, e.delete, self.selected >= 0 && file);
            for (i, c) in self.cells.iter().enumerate() {
                let on = i as i32 == self.selected;
                show(ui, c.border, !on);
                show(ui, c.select, on);
            }
            return;
        }
        if let Some(n) = e.spinner {
            ui.node_mut(n).rotation = self.angle;
        }
        set_page(ui, e.page, 0);
        for n in [e.load, e.save, e.cancel, e.delete] {
            usable(ui, n, false);
        }
        let (wait, loading, saving) = match self.operation.map(|o| o.0) {
            Some(Operation::List) => (true, false, false),
            Some(Operation::Load) => (false, true, false),
            Some(Operation::Save) => (false, false, true),
            _ => return,
        };
        show(ui, e.wait, wait);
        show(ui, e.loading, loading);
        show(ui, e.saving, saving);
    }

    /// `FUN_1400bc1b0`: the table of saves.
    fn fill(&mut self, ui: &mut Ui) {
        self.cells.clear();
        let Some(table) = self.el.table else { return };
        let count = self.saves.len();
        let cells = (count.max(6) + 2) / 3 * 3;
        // `TableElement::SetSize(cells / 3 + 1, 3)`.
        if let Kind::Control(Control::Table {
            cells: c,
            rows,
            columns,
            ..
        }) = &mut ui.node_mut(table).kind
        {
            c.clear();
            rows.clear();
            rows.resize(cells / 3 + 1, 0);
            columns.clear();
            columns.resize(3, 0);
        }
        for i in 0..cells {
            let Some(cell) = ui.instantiate(CELL) else {
                continue;
            };
            ui.set_table_child(table, i, cell);
            let f = |ui: &Ui, n: &str| ui.find(cell, n);
            let (name, date, icon, border, select, corrupt) = (
                f(ui, "labelSaveName"),
                f(ui, "labelSaveDate"),
                f(ui, "imageSaveIcon"),
                f(ui, "borderSave"),
                f(ui, "selectSave"),
                f(ui, "imageCorrupt"),
            );
            let param = |ui: &mut Ui, n: Option<NodeId>, text: &str| {
                if let Some(n) = n {
                    let template = crate::building_ui::text_of(ui, n);
                    ui.set_text(n, &template.replace("@0", text));
                }
            };
            let Some(info) = self.saves.get(i) else {
                // An unused cell.
                for n in [corrupt, select].into_iter().flatten() {
                    ui.set_visible(n, false);
                }
                if let Some(n) = border {
                    ui.node_mut(n).color = 0x80ff_ffff;
                }
                if let Some(n) = icon {
                    ui.node_mut(n).color = 0x20ff_ffff;
                }
                param(ui, name, "");
                param(ui, date, "");
                continue;
            };
            if let Some(n) = icon {
                ui.node_mut(n).flags |= banished_formats::ui::flags::MOUSE;
            }
            match &info.file {
                // The new-save slot: the save sprite, the template's own ("SaveLoadEmpty").
                None => {
                    if let Some(n) = corrupt {
                        ui.set_visible(n, false);
                    }
                    param(ui, name, "");
                    param(ui, date, "");
                }
                Some(file) => {
                    let (w, h) = (info.width.max(0) as u32, info.height.max(0) as u32);
                    let rgba = info
                        .image
                        .chunks(4)
                        .flat_map(|p| [p[2], p[1], p[0], 255])
                        .collect();
                    let sprite =
                        ui.res
                            .image_sheet(&format!("save:{file}:{}", info.date), w, h, rgba);
                    if let Some(n) = icon {
                        if let Kind::Image { sprite: s, .. } = &mut ui.node_mut(n).kind {
                            *s = Some(sprite);
                        }
                    }
                    let readable = info.status == 2;
                    if readable {
                        if let Some(n) = corrupt {
                            ui.set_visible(n, false);
                        }
                    } else if let Some(n) = icon {
                        ui.node_mut(n).color = 0xff80_8080;
                    }
                    let when = if readable {
                        save_list::date_string(info.date)
                    } else {
                        // `FUN_14009e670`: the game's names table (`+0xd8`).
                        ui.res.string(Some(GLOBAL_NAMES), "Corrupt")
                    };
                    param(ui, date, &when);
                    param(ui, name, &info.name.clone());
                }
            }
            self.cells.push(Cell {
                border,
                select,
                icon,
            });
        }
    }

    /// `FUN_1400bbcb0` for a click, and the confirmations' answers.
    pub fn click(
        &mut self,
        ui: &mut Ui,
        screen: &mut Screen,
        audio: &mut crate::sound::Audio,
        archive: &banished_formats::pkg::Archive,
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

    fn ask(&mut self, ui: &mut Ui, screen: &mut Screen, name: &str, c: Confirm, params: &[&str]) {
        let Some(index) = screen.add_modal(ui, &format!("Dialog/SaveLoad.rsc:{name}")) else {
            return;
        };
        let root = screen.dialogs[index].root;
        // `FUN_14006da30`: the message's parameters.
        if !params.is_empty() {
            let texts: Vec<NodeId> = (0..ui.nodes.len())
                .filter(|&n| ui.descendant(root, n) && matches!(ui.node(n).kind, Kind::Text(_)))
                .collect();
            for n in texts {
                let mut t = crate::building_ui::text_of(ui, n);
                if !t.contains("@0") {
                    continue;
                }
                for (i, p) in params.iter().enumerate() {
                    t = t.replace(&format!("@{i}"), p);
                }
                ui.set_text(n, &t);
            }
        }
        self.confirm = Some((c, root));
    }

    /// Operation 4: into the selected file, or a new `Save%i`.
    fn save(&mut self) -> Outcome {
        let file = self
            .selected_file()
            .map(str::to_string)
            .unwrap_or_else(|| save_list::generate_name(&self.saves));
        self.saved = true;
        self.start(Operation::Save, SAVE_LOAD_TIME);
        Outcome::Save(file)
    }

    /// The mods from `next` on (`CheckMod`), each that is not installed as it was saved asking
    /// first, then operation 2.
    fn load(
        &mut self,
        ui: &mut Ui,
        screen: &mut Screen,
        mods: &crate::mods::Modification,
        next: usize,
    ) -> Outcome {
        let Some(info) = usize::try_from(self.selected)
            .ok()
            .and_then(|i| self.saves.get(i))
        else {
            return Outcome::Stay;
        };
        let found = info
            .mods
            .iter()
            .enumerate()
            .skip(next)
            .find_map(|(i, m)| Some((i, m, mods.check(m))).filter(|c| c.2 != 0));
        if let Some((i, m, check)) = found {
            let dialog = match check {
                2 => "modsOutOfDate",
                3 => "modsVersion",
                4 => "modsChanged",
                _ => "modsMissing",
            };
            // `"%i (%s)"` of the author's version and the game version it was made with.
            let version = format!(
                "{} ({})",
                m.values[2],
                crate::mod_ui::version_string(m.values[1])
            );
            let (name, file) = (m.name.clone(), m.file.clone());
            self.ask(
                ui,
                screen,
                dialog,
                Confirm::Load(i + 1),
                &[&name, &file, &version],
            );
            return Outcome::Stay;
        }
        let Some(file) = info.file.clone() else {
            return Outcome::Stay;
        };
        self.reading = Some(crate::save_store::read(&file));
        self.start(Operation::Load, SAVE_LOAD_TIME);
        Outcome::Stay
    }

    /// Operation 5.
    fn delete(&mut self) {
        if let Some(file) = self.selected_file() {
            crate::save_store::delete(file);
        }
        self.deleting = true;
        self.start(Operation::Delete, 0.0);
    }
}

fn set_page(ui: &mut Ui, page: Option<NodeId>, index: usize) {
    if let Some(n) = page {
        if let Kind::Control(Control::Page { selected, .. }) = &mut ui.node_mut(n).kind {
            *selected = index;
        }
    }
}
