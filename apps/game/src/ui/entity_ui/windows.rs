//! Finding, pinning, closing and placing the open entity windows.

use super::*;

impl EntityWindows {
    /// The root of the window containing `node`.
    pub fn root_containing(&self, ui: &Ui, node: NodeId) -> Option<NodeId> {
        self.open
            .iter()
            .find(|w| ui.descendant(w.root, node))
            .map(|w| w.root)
    }

    /// Whether a window with this root is open.
    pub fn is_open(&self, root: NodeId) -> bool {
        self.open.iter().any(|w| w.root == root)
    }

    /// The entity of the window containing `node`.
    pub fn entity_containing(&self, ui: &Ui, node: NodeId) -> Option<Entity> {
        self.open
            .iter()
            .find(|w| ui.descendant(w.root, node))
            .map(|w| w.entity)
    }

    /// TownHallUI's combos (`FUN_140081650` event `0x14`) in the window containing `node`.
    pub fn town_hall_select(
        &mut self,
        ui: &mut Ui,
        statistics: &unbanished_game::work::tracker::Statistics,
        node: NodeId,
    ) -> bool {
        let Some(w) = self.open.iter_mut().find(|w| ui.descendant(w.root, node)) else {
            return false;
        };
        w.bound.iter_mut().any(|c| match c {
            Controller::TownHall(c) => c.select(ui, statistics, node),
            _ => false,
        })
    }

    /// PageToggleUI's button (`FUN_14007abe0`) in the window containing `node`.
    pub fn toggle_page(&mut self, ui: &mut Ui, node: NodeId) -> bool {
        let Some(w) = self.open.iter_mut().find(|w| ui.descendant(w.root, node)) else {
            return false;
        };
        w.bound.iter_mut().any(|c| match c {
            Controller::PageToggle(c) => c.click(ui, node),
            _ => false,
        })
    }

    /// `FUN_140108510` pins the window that asked (EntityUI `+0x38 +0xd8`).
    pub fn pin_containing(&mut self, ui: &Ui, node: NodeId) {
        if let Some(w) = self.open.iter_mut().find(|w| ui.descendant(w.root, node)) {
            w.pinned = true;
        }
    }

    pub fn close_containing(
        &mut self,
        ui: &mut Ui,
        screen: &mut Screen,
        node: NodeId,
    ) -> Option<Entity> {
        let i = self.open.iter().position(|w| ui.descendant(w.root, node))?;
        let w = self.open.remove(i);
        self.saved.extend(w.settings(ui));
        screen.remove_root(ui, w.root);
        Some(w.entity)
    }

    /// `UserInterface::SaveAllSettings` (before a game is saved): every open window's
    /// settings, then all those written since the last call.
    pub fn take_settings(&mut self, ui: &Ui) -> Vec<(usize, Vec<u8>)> {
        let open: Vec<(usize, Vec<u8>)> = self.open.iter().filter_map(|w| w.settings(ui)).collect();
        let mut all = std::mem::take(&mut self.saved);
        all.extend(open);
        all
    }

    /// EntityUI/ToolUI pinning changes only presentation state (ToolUI +0x40).
    pub fn pin(&mut self, ui: &Ui, node: NodeId) -> bool {
        let Some(w) = self.open.iter_mut().find(|w| ui.descendant(w.root, node)) else {
            return false;
        };
        w.pinned = !w.pinned;
        true
    }

    /// The window under `(x, y)` (topmost first), and whether the point is on its
    /// `borderTitle`.
    pub fn window_at(&self, ui: &Ui, x: i32, y: i32) -> Option<(usize, bool)> {
        let inside = |n: NodeId| {
            let [x0, y0, x1, y1] = ui.node(n).rect;
            x0 <= x && x < x1 && y0 <= y && y < y1
        };
        (0..self.open.len()).rev().find_map(|i| {
            let w = &self.open[i];
            inside(w.root).then(|| {
                let title = ui.find(w.root, "borderTitle").is_some_and(inside);
                (i, title)
            })
        })
    }

    /// `FUN_140108950`: the entity is gone; its window closes, pinned or not.
    pub fn close_entity(&mut self, ui: &mut Ui, screen: &mut Screen, entity: Entity) -> bool {
        let Some(i) = self.open.iter().position(|w| w.entity == entity) else {
            return false;
        };
        let w = self.open.remove(i);
        screen.remove_root(ui, w.root);
        true
    }

    /// `FUN_140102160` when the selected entity is off the screen: its unpinned host closes.
    /// Returns whether it did (the selection goes with it).
    pub fn retire(&mut self, ui: &mut Ui, screen: &mut Screen, entity: Entity) -> bool {
        let Some(i) = self
            .open
            .iter()
            .position(|w| w.entity == entity && !w.pinned)
        else {
            return false;
        };
        let w = self.open.remove(i);
        self.saved.extend(w.settings(ui));
        screen.remove_root(ui, w.root);
        true
    }

    /// FUN_140102270: place the unpinned host above the projected object's rectangle.
    pub fn position(&self, ui: &mut Ui, entity: Entity, bounds: [f32; 4]) {
        for w in self.open.iter().filter(|w| w.entity == entity && !w.pinned) {
            let size = ui.node(w.root).size;
            let n = ui.node_mut(w.root);
            n.alignment = 0;
            n.margins = [
                (((bounds[0] + bounds[2]) * 0.5) as i32 - size[0] / 2).max(0),
                0,
                (bounds[1] as i32 - size[1]).max(0),
                0,
            ];
        }
    }
}
