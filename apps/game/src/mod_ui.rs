//! The Mods dialog (`Dialog/Mod.rsc`, vtable at `FUN_140073630`'s object, 0xd8 bytes), opened
//! by the main menu's and the pause menu's Mod button (`MainMenuDialog` id 7, `FUN_1400b8370`).
//!
//! - **Open** (`FUN_1400727f0`): Cancel is element 100 (action 8, Escape), Ok 101. For each mod
//!   a `modLine` joins `tableMods`; its checkbox is `1000 + i`, Up `2000 + i`, Down
//!   `3000 + i`, Top `4000 + i` and the details button `5000 + i`. The first line's Up and Top
//!   and the last line's Down are disabled. Each line's icon is the mod's 48×48 image (a sheet
//!   of its own over `Material/UI/UIMaterial.rsc`). The Browse tab gets the Workshop browser
//!   (`Dialog/Mod.rsc:browser`). The build without Steam (the BanishedKit's
//!   `Application-x64-profile.exe`, as the GOG release) then hides the Browse tab's button and
//!   the Workshop progress (`buttonBrowse`, `groupProgress`, `SetFlagsOnOff(1, 0)` at
//!   `0x14005395a`): mods are the packages in the pack directory only. The port is that build.
//! - **Lines** (`FUN_140072560`, after every change): the name, author and description as the
//!   texts' parameter, the version as `"%i (%s)"` of the mod's version and the game version it
//!   was made with (`Configuration::GetVersionString(+0x18, true)`), the checkbox from
//!   whether it is enabled, and the conflict image shown when the mod and another enabled
//!   mod share a file.
//! - **Clicks** (`FUN_1400723e0`, event 0x12): a checkbox enables or disables its mod
//!   (`FUN_140072090`), Up and Down swap it with its neighbour, Top swaps it up to the first
//!   place (`FUN_140071dc0`); details opens `Dialog/ModDetails.rsc` (`FUN_140071bf0`). Cancel
//!   reads the list again (`RefreshList`) and closes. Ok (`FUN_140072110`) closes when nothing
//!   changed (`Modification` `+0`, set by Enable, Swap and DisableAll); otherwise it asks
//!   `Dialog/Reload.rsc:menu` (or `:game` over a game, `DAT_1401cbdd8`). Yes or Ok saves the
//!   list and reloads: from the menu `FUN_1400b9c60`, over a game the game is saved to memory
//!   (`Save::SavedState`) and loaded again with the new mods (`FUN_1400b9ec0`). Any other
//!   answer reads the list again.
//!
//! - **Details** (`Dialog/ModDetails.rsc`, `FUN_140070950`): Ok is element 0 and closes;
//!   Create (1) and Update (2) upload to the Workshop and show only for a mod the player made
//!   (`FUN_140071600`). The mod's icon, name, path, author, version and description; then the
//!   mods it conflicts with, `^li^c1` (enabled) or `^li^c0`, its name, ` ^n[`, its path and
//!   `]^n` (the conflict title and list hide when there are none); then its files in order,
//!   `^li^c0`, or `^li^c1` when an enabled conflicting mod also has it, `^li^c2` when one of
//!   those is before it in the list, each ending `^n`. Both lists open `^jl^f0^c0^lb` and
//!   close `^le`.
//!
//! Not ported: the Steam build's Workshop browser (`FUN_14006dee0`, `FUN_14006f100`) and
//! uploads, so Create and Update stay hidden.

use banished_ui::{Kind, NodeId, Ui};

use crate::loops::Screen;
use crate::mods::Modification;
use crate::settings::Persistence;

const DIALOG: &str = "Dialog/Mod.rsc:resource";
const LINE: &str = r"Dialog\Mod.rsc:modLine";
const DETAILS: &str = "Dialog/ModDetails.rsc:resource";

/// A text and its own string, whose `@0` the parameter replaces.
type Param = Option<(NodeId, String)>;

/// One `modLine` (`+0xb8`, 0x58 bytes each).
struct Line {
    name: Param,
    author: Param,
    version: Param,
    description: Param,
    /// The icon (`+0x40`, the line's own image).
    icon: Option<NodeId>,
    conflict: Option<NodeId>,
    check: Option<NodeId>,
    top: Option<NodeId>,
    up: Option<NodeId>,
    down: Option<NodeId>,
    details: Option<NodeId>,
}

pub struct ModDialog {
    pub root: NodeId,
    lines: Vec<Line>,
    ok: Option<NodeId>,
    cancel: Option<NodeId>,
    /// Whether a game is running (`DAT_1401cbdd8`).
    in_game: bool,
    /// The Reload question while it is open.
    confirm: Option<NodeId>,
    /// The details dialog while it is open.
    details: Option<NodeId>,
}

/// What the dialog asks of its owner.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    Stay,
    Closed,
    /// Save the list and reload the packages (`FUN_1400b9c60` or `FUN_1400b9ec0`).
    Reload,
}

/// `GetVersionString(version, true)`: `"%i.%i.%i"` of the top three bytes, then the low
/// byte's stage (`0xfb` R, `0xfc` P, `0xfd` A, `0xfe` B; the long names when the flag is
/// clear).
pub fn version_string(version: u32) -> String {
    let mut s = format!(
        "{}.{}.{}",
        version >> 24,
        (version >> 16) & 0xff,
        (version >> 8) & 0xff
    );
    s.push_str(match version & 0xff {
        0xfb => "R",
        0xfc => "P",
        0xfd => "A",
        0xfe => "B",
        _ => "",
    });
    s
}

/// `UI::TextElement::SetParameter(0, text)`.
fn parameter(ui: &mut Ui, p: &Param, text: &str) {
    if let Some((n, template)) = p {
        ui.set_text(*n, &template.replace("@0", text));
    }
}

impl ModDialog {
    /// `FUN_1400b8370` → `FUN_1400727f0`.
    pub fn open(
        ui: &mut Ui,
        screen: &mut Screen,
        mods: &Modification,
        in_game: bool,
    ) -> Option<ModDialog> {
        let index = screen.add_modal(ui, DIALOG)?;
        let root = screen.dialogs[index].root;
        let table = ui.find(root, "tableMods");
        let count = mods.mods.len();
        let mut lines = Vec::with_capacity(count);
        for i in 0..count {
            let Some(line) = ui.instantiate(LINE) else {
                continue;
            };
            if let Some(table) = table {
                ui.add_child(table, line);
            }
            let f = |ui: &Ui, n: &str| ui.find(line, n);
            let p = |ui: &Ui, n: &str| {
                let n = ui.find(line, n)?;
                Some((n, crate::building_ui::text_of(ui, n)))
            };
            let l = Line {
                name: p(ui, "textModName"),
                author: p(ui, "textModAuthor"),
                description: p(ui, "textModDescription"),
                version: p(ui, "textModVersion"),
                icon: f(ui, "imageIcon"),
                check: f(ui, "checkLoaded"),
                conflict: f(ui, "imageConflict"),
                top: f(ui, "buttonTop"),
                up: f(ui, "buttonUp"),
                down: f(ui, "buttonDown"),
                details: f(ui, "buttonDetails"),
            };
            let disable = |ui: &mut Ui, n: Option<NodeId>| {
                if let Some(n) = n {
                    ui.node_mut(n).enabled = false;
                }
            };
            if i == 0 {
                disable(ui, l.up);
                disable(ui, l.top);
            }
            if i + 1 == count {
                disable(ui, l.down);
            }
            lines.push(l);
        }
        // The build without Steam: no Browse tab and no Workshop progress.
        for n in ["buttonBrowse", "groupProgress"] {
            if let Some(n) = ui.find(root, n) {
                ui.set_visible(n, false);
            }
        }
        let d = ModDialog {
            root,
            lines,
            ok: ui.find(root, "buttonOk"),
            cancel: ui.find(root, "buttonCancel"),
            in_game,
            confirm: None,
            details: None,
        };
        d.refresh(ui, mods);
        Some(d)
    }

    /// `FUN_140072560`.
    pub fn refresh(&self, ui: &mut Ui, mods: &Modification) {
        for (i, (l, m)) in self.lines.iter().zip(&mods.mods).enumerate() {
            parameter(ui, &l.name, &m.name);
            parameter(ui, &l.author, &m.author);
            parameter(ui, &l.description, &m.description);
            let version = format!("{} ({})", m.user_version, version_string(m.game_version));
            parameter(ui, &l.version, &version);
            if let Some(c) = l.check {
                ui.set_checked(c, m.enabled);
            }
            // `ImageBuffer::CopyToTexture` of the mod's 48×48 icon.
            let (w, h, argb) = &m.icon;
            if let (Some(icon), true) = (l.icon, *w > 0 && *h > 0) {
                let rgba = argb
                    .chunks(4)
                    .flat_map(|p| [p[2], p[1], p[0], p[3]])
                    .collect();
                let key = format!("mod:{}:{:08x}", m.file, m.crc);
                let sprite = ui.res.image_sheet(&key, *w, *h, rgba);
                if let Kind::Image { sprite: s, .. } = &mut ui.node_mut(icon).kind {
                    *s = Some(sprite);
                }
            }
            if let Some(c) = l.conflict {
                ui.set_visible(c, mods.in_conflict(i));
            }
        }
    }

    /// `FUN_1400723e0`, and the Reload question's answer.
    pub fn click(
        &mut self,
        ui: &mut Ui,
        screen: &mut Screen,
        mods: &mut Modification,
        settings: &Persistence,
        node: NodeId,
    ) -> Outcome {
        let is = |n: Option<NodeId>| n.is_some_and(|b| ui.descendant(b, node));
        if let Some(root) = self.details {
            // `FUN_140070950`'s Ok (element 0).
            if ui
                .find(root, "buttonOk")
                .is_some_and(|b| ui.descendant(b, node))
            {
                screen.remove_root(ui, root);
                self.details = None;
            }
            return Outcome::Stay;
        }
        if let Some(root) = self.confirm {
            let on = |b: &str| ui.find(root, b).is_some_and(|b| ui.descendant(b, node));
            let yes = on("buttonYes") || on("buttonOk");
            if !(yes || on("buttonNo") || on("buttonCancel")) {
                return Outcome::Stay;
            }
            screen.remove_root(ui, root);
            self.confirm = None;
            screen.remove_root(ui, self.root);
            if yes {
                return Outcome::Reload;
            }
            mods.revert(settings);
            return Outcome::Closed;
        }
        if is(self.cancel) {
            return self.cancel(ui, screen, mods, settings);
        }
        if is(self.ok) {
            // `FUN_140072110`.
            if !mods.dirty {
                screen.remove_root(ui, self.root);
                return Outcome::Closed;
            }
            let reference = if self.in_game {
                "Dialog/Reload.rsc:game"
            } else {
                "Dialog/Reload.rsc:menu"
            };
            if let Some(index) = screen.add_modal(ui, reference) {
                self.confirm = Some(screen.dialogs[index].root);
            }
            return Outcome::Stay;
        }
        let Some(i) = self.lines.iter().position(|l| {
            [l.check, l.top, l.up, l.down, l.details]
                .into_iter()
                .any(|n| n.is_some_and(|b| ui.descendant(b, node)))
        }) else {
            return Outcome::Stay;
        };
        let l = &self.lines[i];
        if is(l.check) {
            // `FUN_140072090`: the checkbox's state, already toggled.
            let on = l.check.is_some_and(|c| ui.checked(c));
            mods.enable(i, on);
        } else if is(l.up) {
            mods.swap(i, i - 1);
        } else if is(l.down) {
            mods.swap(i, i + 1);
        } else if is(l.details) {
            self.details = open_details(ui, screen, mods, i);
            return Outcome::Stay;
        } else if is(l.top) {
            for j in (1..=i).rev() {
                mods.swap(j, j - 1);
            }
        }
        self.refresh(ui, mods);
        Outcome::Stay
    }

    /// Cancel (element 100, also Escape): `RefreshList`, then close.
    pub fn cancel(
        &mut self,
        ui: &mut Ui,
        screen: &mut Screen,
        mods: &mut Modification,
        settings: &Persistence,
    ) -> Outcome {
        if let Some(root) = self.details.take() {
            screen.remove_root(ui, root);
            return Outcome::Stay;
        }
        if let Some(root) = self.confirm.take() {
            screen.remove_root(ui, root);
        }
        mods.revert(settings);
        screen.remove_root(ui, self.root);
        Outcome::Closed
    }
}

/// `FUN_140071bf0` → `FUN_140070950`: the details of mod `i`.
fn open_details(ui: &mut Ui, screen: &mut Screen, mods: &Modification, i: usize) -> Option<NodeId> {
    let m = mods.mods.get(i)?;
    let index = screen.add_modal(ui, DETAILS)?;
    let root = screen.dialogs[index].root;
    let find = |ui: &Ui, n: &str| ui.find(root, n);
    for n in ["buttonCreate", "buttonUpdate"] {
        if let Some(n) = find(ui, n) {
            ui.set_visible(n, false);
        }
    }
    let param = |ui: &mut Ui, n: &str, text: &str| {
        if let Some(n) = find(ui, n) {
            let template = crate::building_ui::text_of(ui, n);
            ui.set_text(n, &template.replace("@0", text));
        }
    };
    param(ui, "textName", &m.name);
    param(ui, "textAuthor", &m.author);
    param(ui, "textPath", &m.path());
    let version = format!("{} ({})", m.user_version, version_string(m.game_version));
    param(ui, "textVersion", &version);
    param(ui, "textDescription", &m.description);
    let (w, h, argb) = &m.icon;
    if let (Some(icon), true) = (find(ui, "imageIcon"), *w > 0 && *h > 0) {
        let rgba = argb
            .chunks(4)
            .flat_map(|p| [p[2], p[1], p[0], p[3]])
            .collect();
        let sprite = ui
            .res
            .image_sheet(&format!("mod:{}:{:08x}", m.file, m.crc), *w, *h, rgba);
        if let Kind::Image { sprite: s, .. } = &mut ui.node_mut(icon).kind {
            *s = Some(sprite);
        }
    }
    let conflicting: Vec<(usize, &crate::mods::Mod)> = m
        .conflicts
        .iter()
        .filter_map(|f| mods.mods.iter().enumerate().find(|(_, o)| &o.file == f))
        .collect();
    if conflicting.is_empty() {
        for n in ["textConflictTitle", "textConflictList"] {
            if let Some(n) = find(ui, n) {
                ui.set_visible(n, false);
            }
        }
    } else if let Some(n) = find(ui, "textConflictList") {
        let mut text = String::from("^jl^f0^c0^lb");
        for (_, o) in &conflicting {
            let colour = if o.enabled { "^li^c1" } else { "^li^c0" };
            text += &format!("{colour}{} ^n[{}]^n", o.name, o.path());
        }
        text += "^le";
        ui.set_text(n, &text);
    }
    if let Some(n) = find(ui, "textResourceList") {
        let mut text = String::from("^jl^f0^c0^lb");
        for name in &m.names {
            let key = banished_formats::pkg::normalise(name);
            let mut shared = false;
            let mut earlier = false;
            for (j, o) in conflicting.iter().filter(|(_, o)| o.enabled) {
                shared |= o.files.contains(&key);
                earlier |= *j < i;
            }
            let colour = match (shared, earlier) {
                (false, _) => "^li^c0",
                (true, false) => "^li^c1",
                (true, true) => "^li^c2",
            };
            text += &format!("{colour}{name}^n");
        }
        text += "^le";
        ui.set_text(n, &text);
    }
    Some(root)
}

#[cfg(test)]
mod tests {
    use super::version_string;

    #[test]
    fn versions_read_as_the_original_writes_them() {
        assert_eq!(version_string(0x0100_04fe), "1.0.4B");
        assert_eq!(version_string(0x0100_07ff), "1.0.7");
        assert_eq!(version_string(0x0100_02fb), "1.0.2R");
    }
}
