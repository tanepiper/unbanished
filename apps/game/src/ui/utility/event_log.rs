//! The event log window: its rows, its notification checkboxes, a click on a row, and the text of an event.

use super::*;

/// `EventLogUI` (vtable `0x140161d20`): OnOpened `FUN_140067fa0`, refresh `FUN_140067da0`,
/// events `FUN_140067f30`. Rows show the newest entries last; previous/next (ids 100/101) scroll
/// one entry, bottom (102) returns to the newest. Read-only over the log.
pub struct EventLogUi {
    previous: Option<NodeId>,
    next: Option<NodeId>,
    bottom: Option<NodeId>,
    /// Per display row: text (`@0` parameter) and go-to button.
    rows: Vec<(Option<(NodeId, String)>, Option<NodeId>)>,
    /// `+0x48`: entries scrolled back from the newest.
    scroll: i32,
    /// `notifybuttons` children (ids `0x67 + i`): the log's notify flags.
    checks: Vec<NodeId>,
}

/// `FUN_14009a2d0`'s text: `FormatIndexed(events[type], season, year, @2, @3, @4)`.
pub fn format_event(ui: &mut Ui, e: &unbanished_game::events::EventEntry) -> String {
    unbanished_sim::strings::format_event(&mut crate::ui::strings::UiStrings(&mut ui.res), e)
}

impl EventLogUi {
    /// OnOpened `FUN_140067fa0`. The checkboxes take the log's notify flags.
    pub(super) fn bind(
        ui: &mut Ui,
        root: NodeId,
        log: Option<&unbanished_game::events::EventLog>,
    ) -> EventLogUi {
        let checks: Vec<NodeId> = ui
            .find(root, "notifybuttons")
            .map(|n| ui.node(n).children.clone())
            .unwrap_or_default();
        for (i, &c) in checks.iter().enumerate() {
            let on = log.map_or(i != 0, |l| l.notify[i]);
            ui.set_checked(c, on);
        }
        let mut rows = Vec::new();
        if let Some(display) = ui.find(root, "display") {
            if let Kind::Control(unbanished_ui::Control::Table { cells, rows: r, .. }) =
                &ui.node(display).kind
            {
                for y in 0..r.len() {
                    let cell = |x: usize| cells.iter().find(|c| c.0 == x && c.1 == y).map(|c| c.2);
                    let button = cell(0);
                    let text = cell(1).map(|t| (t, text_of(ui, t)));
                    rows.push((text, button));
                }
            }
        }
        EventLogUi {
            previous: ui.find(root, "buttonPrevious"),
            next: ui.find(root, "buttonNext"),
            bottom: ui.find(root, "buttonBottom"),
            rows,
            scroll: 0,
            checks,
        }
    }

    /// `FUN_140067f30`.
    pub(super) fn click(
        &mut self,
        ui: &Ui,
        node: NodeId,
        log: &unbanished_game::events::EventLog,
    ) -> Option<UtilityClick> {
        if let Some(i) = self
            .checks
            .iter()
            .position(|&c| c == node || ui.descendant(c, node))
        {
            return Some(UtilityClick::Notify(i, ui.checked(self.checks[i])));
        }
        if let Some(r) = self
            .rows
            .iter()
            .position(|(_, b)| b.is_some_and(|b| b == node || ui.descendant(b, node)))
        {
            // Entry `n - scroll - rows + r`.
            let idx =
                log.entries.len() as i64 - self.scroll as i64 - self.rows.len() as i64 + r as i64;
            let cell = (idx >= 0)
                .then(|| log.entries.get(idx as usize))
                .flatten()
                .map_or(-1, |e| e.cell);
            return (cell >= 0)
                .then_some(UtilityClick::GoTo(cell))
                .or(Some(UtilityClick::Handled));
        }
        if Some(node) == self.previous {
            self.scroll += 1;
        } else if Some(node) == self.next {
            self.scroll -= 1;
        } else if Some(node) == self.bottom {
            self.scroll = 0;
        } else {
            return None;
        }
        if self.scroll < 0 {
            self.scroll = 0;
        }
        Some(UtilityClick::Handled)
    }

    /// `FUN_140067da0`.
    pub(super) fn update(&self, ui: &mut Ui, log: &unbanished_game::events::EventLog) {
        let n = log.entries.len() as i64;
        let rows = self.rows.len() as i64;
        let base = (n - self.scroll as i64 - 1) - (rows - 1);
        for (r, (text, button)) in self.rows.iter().enumerate() {
            let idx = base + r as i64;
            let entry = (idx >= 0).then(|| log.entries.get(idx as usize)).flatten();
            if let Some((node, template)) = text {
                let line = entry.map(|e| format_event(ui, e)).unwrap_or_default();
                ui.set_text(*node, &template.replace("@0", &line));
                if let Some(e) = entry {
                    ui.node_mut(*node).color = e.color;
                }
            }
            if let Some(b) = button {
                // Element slot `+0x40` (enabled) with whether the entry has a cell
                // (`+0x18 >= 0`); an empty row's button is disabled.
                ui.node_mut(*b).enabled = entry.is_some_and(|e| e.cell >= 0);
            }
        }
        for b in [self.bottom, self.next].into_iter().flatten() {
            ui.node_mut(b).enabled = self.scroll > 0;
        }
    }
}
