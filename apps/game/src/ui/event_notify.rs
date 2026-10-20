//! The event notification ribbon (`Dialog/EventNotify.rsc`, `EventNotifyDialog`, vtable
//! `0x140161e40`) and the event popup (`Dialog/EventPopup.rsc:resource`, `EventPopupDialog`,
//! vtable `0x140161ec0`). See docs/EVENT-LOG.md.
//!
//! The ribbon reads the event log only; its clicks return a cell for the camera.

use unbanished_game::events::{severity_color, EventLog};
use unbanished_ui::{NodeId, Ui};

use crate::loops::Screen;

pub const DIALOG: &str = "Dialog/EventNotify.rsc:resource";
const POPUP: &str = "Dialog/EventPopup.rsc:resource";
const SPRITES: &str = r"Dialog\SpriteSheet.rsc:resource";
/// `DAT_1401788cc`: a notification stays 30 s.
const LIFETIME: f32 = 30.0;

/// `FUN_140068590`'s switch: the notify flag (`log +0x10 + group`) that covers an event type.
/// Groups follow the Event Log's checkboxes: old age, deaths, disasters, storage, town, other.
pub fn notify_group(kind: i32) -> usize {
    match kind {
        2 => 0,
        0 | 1 | 3..=7 => 1,
        0xb | 0xc | 0x1e | 0x1f => 2,
        0x11..=0x18 => 3,
        0xd | 0xe => 4,
        _ => 5,
    }
}

/// The ribbon text: the entry text after "season year: " (`SubString` past the first ':' and
/// one more character), or the whole text without a colon.
pub fn short_text(text: &str) -> &str {
    match text.find(':') {
        Some(i) => text.get(i + 2..).unwrap_or(""),
        None => text,
    }
}

/// One shown notification (0x30 bytes at `+0xd8`).
#[derive(Debug, Clone, PartialEq)]
pub struct Note {
    /// Seconds left.
    pub time: f32,
    /// Index into the log.
    pub entry: usize,
    /// `EventIcon{category}`.
    pub category: i32,
    pub text: String,
    pub color: u32,
}

/// The ribbon's state, separate from its elements so it can be tested without a UI.
#[derive(Debug, Clone, Default)]
pub struct NotifyState {
    pub notes: Vec<Note>,
    /// `+0xc4`: the newest entry already seen (-1 before any).
    pub last_seen: i64,
    /// `+0xd0`: slots in the ribbon (`notifyContent` children).
    pub capacity: usize,
}

impl NotifyState {
    pub fn new(log: &EventLog, capacity: usize) -> NotifyState {
        // OnOpened (`FUN_140069030`) sets `+0xc4` to the entry count; the update starts at
        // `+0xc4 + 1` and then stores `count - 1`. An entry added before the first update is
        // therefore skipped (as in the original); later ones are not.
        NotifyState {
            notes: Vec::new(),
            last_seen: log.entries.len() as i64,
            capacity,
        }
    }

    /// `FUN_140068590`: age the notifications, then add one per new entry with severity > 0
    /// whose group is enabled and whose category is set. Returns the entries that open a
    /// popup (icon set) and whether anything was added (the alert sound).
    pub fn update(
        &mut self,
        log: &EventLog,
        mut text: impl FnMut(usize) -> String,
        dt: f32,
    ) -> (Vec<usize>, bool) {
        self.notes.retain_mut(|n| {
            n.time -= dt;
            n.time >= 1e-6
        });
        let mut popups = Vec::new();
        let mut added = false;
        let last = log.entries.len() as i64 - 1;
        if self.last_seen < last {
            for i in (self.last_seen + 1).max(0)..=last {
                let e = &log.entries[i as usize];
                if e.severity <= 0 || !log.notify[notify_group(e.kind)] || e.category < 0 {
                    continue;
                }
                if e.icon >= 0 {
                    popups.push(i as usize);
                }
                let full = text(i as usize);
                self.notes.push(Note {
                    time: LIFETIME,
                    entry: i as usize,
                    category: e.category,
                    text: short_text(&full).to_string(),
                    color: severity_color(e.severity),
                });
                added = true;
            }
            while self.notes.len() > self.capacity {
                self.notes.remove(0);
            }
        }
        // Stored every update, so the next frame starts after the newest entry.
        self.last_seen = last;
        (popups, added)
    }
}

struct Slot {
    group: NodeId,
    image: Option<NodeId>,
    button: Option<NodeId>,
}

pub struct EventNotify {
    root: NodeId,
    ribbon: Option<NodeId>,
    slots: Vec<Slot>,
    pub state: NotifyState,
    /// `+0xc0`: the smoothed right padding.
    padding: f32,
    /// Open popups: root and OK button (id 200).
    popups: Vec<(NodeId, Option<NodeId>)>,
}

fn set_sprite(ui: &mut Ui, node: NodeId, name: &str) {
    if let Some(s) = ui.res.sprite(Some(SPRITES), name) {
        ui.set_images(node, s);
    }
}

impl EventNotify {
    /// `FUN_14009d0d0` adds the dialog; OnOpened `FUN_140069030` binds the slots.
    pub fn open(ui: &mut Ui, screen: &mut Screen, log: &EventLog) -> Option<EventNotify> {
        let dialog = screen.add_dialog(ui, DIALOG)?;
        let root = screen.dialogs[dialog].root;
        let ribbon = ui.find(root, "ribbonNotify");
        let mut slots = Vec::new();
        if let Some(content) = ui.find(root, "notifyContent") {
            for group in ui.node(content).children.clone() {
                slots.push(Slot {
                    group,
                    image: ui.find(group, "notifyImage"),
                    button: ui.find(group, "notifyButton"),
                });
            }
        }
        let state = NotifyState::new(log, slots.len());
        let mut notify = EventNotify {
            root,
            ribbon,
            slots,
            state,
            padding: 0.0,
            popups: Vec::new(),
        };
        notify.refresh(ui, log, 0);
        Some(notify)
    }

    /// Per frame: the update (`FUN_140068590`), popups, then the layout refresh
    /// (`FUN_140068a20`). `toolbar_width` is the toolbar root's layout width (`+0x60`).
    /// Returns whether the alert sound would play (`Audio/UI/WarningEffect.rsc`).
    pub fn update(
        &mut self,
        ui: &mut Ui,
        screen: &mut Screen,
        log: &EventLog,
        dt: f32,
        toolbar_width: i32,
    ) -> bool {
        let (popups, added) = self.state.update(
            log,
            |i| crate::ui::utility::format_event(ui, &log.entries[i]),
            dt,
        );
        for i in popups {
            self.open_popup(ui, screen, log, i);
        }
        self.refresh(ui, log, toolbar_width);
        added
    }

    /// `EventPopupDialog` OnOpened `FUN_140069580`: icon `EventIcon{icon}`, the full text,
    /// and `buttonOk` as element 200.
    fn open_popup(&mut self, ui: &mut Ui, screen: &mut Screen, log: &EventLog, entry: usize) {
        let Some(d) = screen.add_dialog(ui, POPUP) else {
            return;
        };
        let root = screen.dialogs[d].root;
        let e = &log.entries[entry];
        if let Some(icon) = ui.find(root, "imageIcon") {
            set_sprite(ui, icon, &format!("EventIcon{}", e.icon));
        }
        if let Some(text) = ui.find(root, "textEvent") {
            let line = crate::ui::utility::format_event(ui, e);
            crate::ui::utility::set_parameters(
                ui,
                &(text, crate::ui::utility::text_of(ui, text)),
                &[line],
            );
        }
        self.popups.push((root, ui.find(root, "buttonOk")));
    }

    /// `FUN_140068a20`.
    fn refresh(&mut self, ui: &mut Ui, log: &EventLog, toolbar_width: i32) {
        let own = ui.node(self.root).rect;
        let base = toolbar_width - (own[2] - own[0]) + 0x2c;
        let target = match self.state.notes.len() {
            0 => 0.0,
            n => (n - 1) as f32 * 40.0 + base as f32,
        };
        self.padding = self.padding * 0.8 + target * 0.2;
        let node = ui.node_mut(self.root);
        node.margins = [0, (self.padding + 0.5) as i32, 0, 2];
        if let Some(r) = self.ribbon {
            ui.set_visible(r, true);
        }
        for (i, slot) in self.slots.iter().enumerate() {
            match self.state.notes.get(i) {
                Some(n) => {
                    let name = format!("EventIcon{}", n.category);
                    let located = log.entries.get(n.entry).is_some_and(|e| e.cell >= 0);
                    if let Some(image) = slot.image {
                        set_sprite(ui, image, &name);
                        ui.set_visible(image, !located);
                        ui.node_mut(image).color = n.color;
                    }
                    if let Some(button) = slot.button {
                        set_sprite(ui, button, &name);
                        ui.set_visible(button, located);
                        ui.node_mut(button).color = n.color;
                    }
                    ui.node_mut(slot.group).tooltip = n.text.clone();
                }
                None => {
                    if let Some(image) = slot.image {
                        ui.node_mut(image).color = 0xffff_ffff;
                        ui.set_visible(image, true);
                    }
                    if let Some(button) = slot.button {
                        ui.set_visible(button, false);
                    }
                    ui.node_mut(slot.group).tooltip.clear();
                }
            }
        }
    }

    /// A click: the OK button closes its popup; a ribbon button (`FUN_140068fe0`, ids
    /// `0x5dc + i`) returns its entry's cell for the camera (`FUN_140068f00`).
    pub fn click(
        &mut self,
        ui: &mut Ui,
        screen: &mut Screen,
        log: &EventLog,
        node: NodeId,
    ) -> Option<Option<i32>> {
        if let Some(i) = self.popups.iter().position(|p| p.1 == Some(node)) {
            let (root, _) = self.popups.remove(i);
            screen.remove_root(ui, root);
            return Some(None);
        }
        let i = self.slots.iter().position(|s| {
            s.button
                .is_some_and(|b| b == node || ui.descendant(b, node))
        })?;
        let note = self.state.notes.get(i)?;
        let cell = log.entries.get(note.entry)?.cell;
        Some((cell >= 0).then_some(cell))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn log_with(kinds: &[(i32, i32)]) -> EventLog {
        let mut log = EventLog::default();
        for &(kind, severity) in kinds {
            log.add(kind, severity, -1, 0, 1, Default::default(), false);
        }
        log
    }

    #[test]
    fn groups_and_text() {
        assert_eq!(notify_group(2), 0);
        assert_eq!(notify_group(22), 3);
        assert_eq!(notify_group(31), 2);
        assert_eq!(notify_group(9), 5);
        assert_eq!(
            short_text("Early Spring 1: The reserve of food is low!"),
            "The reserve of food is low!"
        );
        assert_eq!(short_text("no colon"), "no colon");
    }

    #[test]
    fn adds_new_enabled_entries_and_expires() {
        let mut log = log_with(&[]);
        let mut s = NotifyState::new(&log, 6);
        // An entry added before the first update is skipped.
        log.add(22, 3, -1, 0, 1, Default::default(), false);
        s.update(&log, |_| String::new(), 0.0);
        assert!(s.notes.is_empty());
        let mut log = log_with(&[]);
        let mut s = NotifyState::new(&log, 6);
        s.update(&log, |_| String::new(), 0.0);
        log.add(22, 3, -1, 0, 1, Default::default(), false);
        // Low food (storage group, severity 3), a coming of age (category -1), a tornado.
        log.add(22, 3, -1, 0, 1, Default::default(), false);
        log.add(9, 0, -1, 0, 1, Default::default(), false);
        log.add(30, 3, -1, 0, 1, Default::default(), false);
        let (popups, added) = s.update(&log, |_| "Spring 1: x".into(), 0.0);
        assert!(added && popups.is_empty());
        assert_eq!(
            s.notes.iter().map(|n| n.entry).collect::<Vec<_>>(),
            vec![0, 1, 3]
        );
        assert_eq!(s.notes[0].text, "x");
        // Nothing new: no sound.
        assert!(!s.update(&log, |_| String::new(), 29.0).1);
        assert_eq!(s.notes.len(), 3);
        s.update(&log, |_| String::new(), 1.0);
        assert!(s.notes.is_empty());
    }

    #[test]
    fn disabled_group_and_capacity() {
        let mut log = log_with(&[]);
        log.notify[3] = false;
        let mut s = NotifyState::new(&log, 2);
        for _ in 0..4 {
            log.add(17, 3, -1, 0, 1, Default::default(), false);
        }
        log.add(31, 3, -1, 0, 1, Default::default(), false);
        log.add(0x1a, 3, -1, 0, 1, Default::default(), false);
        log.add(0x1e, 3, -1, 0, 1, Default::default(), false);
        let (popups, _) = s.update(&log, |_| String::new(), 0.0);
        // Storage warnings are off; types 26 (icon 12) opens a popup; the oldest drops.
        assert_eq!(popups, vec![5]);
        assert_eq!(
            s.notes.iter().map(|n| n.entry).collect::<Vec<_>>(),
            vec![5, 6]
        );
    }
}
