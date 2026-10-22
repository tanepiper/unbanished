//! The help window: its pages and scrolling.

use super::*;

/// `HelpUI` (vtable `0x140162070`): bind `FUN_14006ae60`, refresh `FUN_14006ab90`, events
/// `FUN_14006abf0`. `comboTopic` (id `0xe74`) lists the help pages (`Game +0xa0`,
/// `Game\Help\HelpTopics.rsc`: string tables of Topic, Title and Text) by title; the chosen
/// page's text fills `documentHelp` (`0xe75`, `FUN_14006a880`), scrolled to the top, and joins
/// the history after the current page. A link opens the page whose topic it names
/// (`FUN_14006aaa0`); back (`0xe76`) and forward (`0xe77`) walk the history, each page keeping
/// where it was scrolled to; they are enabled when there is somewhere to go.
pub struct HelpUi {
    combo: Option<NodeId>,
    document: Option<NodeId>,
    container: Option<NodeId>,
    back: Option<NodeId>,
    forward: Option<NodeId>,
    /// The pages' string tables.
    pages: Vec<String>,
    /// `+0x68`: (page, scroll) visited; `+0x64` the current one.
    history: Vec<(usize, i32)>,
    at: usize,
}

impl HelpUi {
    pub(super) fn bind(ui: &mut Ui, root: NodeId, archive: &Archive) -> HelpUi {
        let pages: Vec<String> = archive
            .read_ref("Game/Help/HelpTopics.rsc:resource")
            .ok()
            .and_then(|d| external_list(&d).ok())
            .unwrap_or_default();
        let document = ui.find(root, "documentHelp");
        // The container that scrolls the document.
        let container = (0..ui.nodes.len()).find(|&n| {
            ui.descendant(root, n)
                && matches!(&ui.node(n).kind, Kind::Control(unbanished_ui::Control::Container {
                    content: Some(c), ..
                }) if Some(*c) == document || document.is_some_and(|d| ui.descendant(*c, d)))
        });
        let combo = ui.find(root, "comboTopic");
        if let Some(c) = combo {
            let titles = pages
                .iter()
                .map(|t| ui.res.string(Some(&t.replace('\\', "/")), "Title"))
                .collect();
            ui.set_choices(c, titles);
        }
        let mut help = HelpUi {
            combo,
            document,
            container,
            back: ui.find(root, "buttonBack"),
            forward: ui.find(root, "buttonForward"),
            pages,
            history: Vec::new(),
            at: 0,
        };
        if let Some(c) = help.combo {
            crate::menu::select_combo(ui, c, 0);
        }
        help.open(ui, 0);
        help
    }

    pub(super) fn scroll(&self, ui: &Ui) -> i32 {
        match self.container.map(|c| &ui.node(c).kind) {
            Some(Kind::Control(unbanished_ui::Control::Container { offset, .. })) => offset[1],
            _ => 0,
        }
    }

    pub(super) fn set_scroll(&self, ui: &mut Ui, y: i32) {
        if let Some(c) = self.container {
            if let Kind::Control(unbanished_ui::Control::Container { offset, .. }) =
                &mut ui.node_mut(c).kind
            {
                offset[1] = y;
            }
        }
    }

    pub(super) fn show(&self, ui: &mut Ui, page: usize) {
        let Some(table) = self.pages.get(page) else {
            return;
        };
        let text = ui.res.string(Some(&table.replace('\\', "/")), "Text");
        if let Some(d) = self.document {
            ui.set_text(d, &text);
        }
    }

    /// `FUN_14006a880`: the page, at the top, after the current one in the history.
    pub(super) fn open(&mut self, ui: &mut Ui, page: usize) {
        self.show(ui, page);
        let scroll = self.scroll(ui);
        self.set_scroll(ui, 0);
        self.history.truncate(self.at + 1);
        self.history.push((page, scroll));
        self.at = self.history.len() - 1;
    }

    /// `FUN_14006ab90`.
    pub(super) fn update(&self, ui: &mut Ui) {
        if let Some(b) = self.back {
            ui.node_mut(b).enabled = 0 < self.at;
        }
        if let Some(f) = self.forward {
            ui.node_mut(f).enabled = self.at + 1 < self.history.len();
        }
    }

    /// `FUN_14006abf0`: the topic's combo (event `0x14`), a link, back or forward.
    pub(super) fn event(&mut self, ui: &mut Ui, event: unbanished_ui::Event) -> bool {
        use unbanished_ui::Event;
        match event {
            Event::Selected(n, i) if Some(n) == self.combo => {
                self.open(ui, i);
                true
            }
            Event::Link(n, i) if Some(n) == self.document => {
                let Some(target) = ui.document_link(n, i).map(str::to_owned) else {
                    return true;
                };
                let found = self
                    .pages
                    .iter()
                    .position(|t| ui.res.string(Some(&t.replace('\\', "/")), "Topic") == target);
                if let Some(page) = found {
                    let scroll = self.scroll(ui);
                    if let Some(h) = self.history.get_mut(self.at) {
                        h.1 = scroll;
                    }
                    if let Some(c) = self.combo {
                        crate::menu::select_combo(ui, c, page);
                    }
                    self.open(ui, page);
                }
                true
            }
            Event::Clicked(n) if Some(n) == self.back || Some(n) == self.forward => {
                let forward = Some(n) == self.forward;
                if (forward && self.at + 1 >= self.history.len()) || (!forward && self.at == 0) {
                    return true;
                }
                let scroll = self.scroll(ui);
                if let Some(h) = self.history.get_mut(self.at) {
                    h.1 = scroll;
                }
                self.at = if forward { self.at + 1 } else { self.at - 1 };
                let (page, scroll) = self.history[self.at];
                self.show(ui, page);
                self.set_scroll(ui, scroll);
                if let Some(c) = self.combo {
                    crate::menu::select_combo(ui, c, page);
                }
                true
            }
            _ => false,
        }
    }
}
