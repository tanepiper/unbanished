//! A step's setup: entering, updating and leaving it, the locked controls and the closed dialogs.

use super::*;

impl GameplayLoop {
    pub(super) fn setup_enter(&mut self, ctx: &mut Ctx, s: &Setup) {
        match s {
            // `FUN_1400e7ac0`: everything locked but the listed elements; the listed dialogs
            // close; the highlight points at its element.
            Setup::Dialog { close, .. } => {
                self.apply_locks(ctx.ui, s);
                for path in close {
                    self.close_dialogs(ctx, path);
                }
                self.update_highlight(ctx, s, false);
            }
            // `FUN_1400e75a0`: the camera goes to the cells.
            Setup::Camera(c) => {
                self.focus_cells(c[0], c[1], c[2], c[3]);
            }
            // `FUN_1400e8230`.
            Setup::Speed { speed, unpause } => {
                if let Some(i) = super::super::SPEED_LEVELS
                    .iter()
                    .position(|&l| l as i32 == *speed)
                {
                    self.speed = i as u8;
                }
                if *unpause {
                    self.paused = false;
                }
                self.update_hud(ctx.ui);
            }
            // `FUN_1400e83f0`: the player may place and select only the templates, there;
            // with a template to place the camera goes there.
            Setup::Restrict {
                rect,
                rotation,
                place,
                select,
            } => {
                self.restrict = Some(Restrict {
                    rect: *rect,
                    rotation: *rotation,
                    place: place.clone(),
                    select: select.clone(),
                });
                if !place.is_empty() {
                    self.focus_cells(rect[0], rect[1], rect[2], rect[3]);
                }
            }
        }
    }

    pub(super) fn setup_update(&mut self, ctx: &mut Ctx, s: &Setup) {
        // `FUN_1400e7990`: the locks again (new dialogs appear) and the highlight follows.
        if let Setup::Dialog { .. } = s {
            self.apply_locks(ctx.ui, s);
            self.update_highlight(ctx, s, true);
        }
    }

    pub(super) fn setup_exit(&mut self, ctx: &mut Ctx, s: &Setup) {
        match s {
            // `FUN_1400e7930`: everything unlocked; the highlight closes.
            Setup::Dialog { .. } => {
                for root in self.dialog_roots() {
                    set_locked(ctx.ui, root, false);
                }
                self.close_highlight(ctx);
            }
            // `FUN_1400e8360`: the restriction lifts; with a template to place, its tool goes.
            Setup::Restrict { place, .. } => {
                self.restrict = None;
                if !place.is_empty() {
                    self.end_placement(ctx);
                }
            }
            _ => {}
        }
    }

    /// `FUN_1400e7ec0`: every dialog locked (`FUN_1400e7800`, each root and its children),
    /// then the listed elements unlocked.
    pub(super) fn apply_locks(&mut self, ui: &mut Ui, s: &Setup) {
        let Setup::Dialog { enable, .. } = s else {
            return;
        };
        for root in self.dialog_roots() {
            set_locked(ui, root, true);
        }
        for path in enable {
            if let Some(n) = self.tutorial_element(ui, path) {
                set_locked(ui, n, false);
            }
        }
    }

    /// `FUN_1400e7ac0`'s close list: an entity's window (and its selection) or a utility
    /// window with the controller.
    pub(super) fn close_dialogs(&mut self, ctx: &mut Ctx, path: &str) {
        let parts: Vec<&str> = path.split(':').collect();
        let ["DialogInstance", class, ..] = parts.as_slice() else {
            return;
        };
        let id = class_id(class);
        let Some(screen) = self.screen.as_mut() else {
            return;
        };
        let roots: Vec<NodeId> = self
            .entities
            .open
            .iter()
            .filter(|w| w.controllers.iter().any(|(c, _)| c.type_id == id))
            .map(|w| w.root)
            .collect();
        for root in roots {
            self.entities.open.retain(|w| w.root != root);
            screen.remove_root(ctx.ui, root);
            self.selection.selected = None;
        }
        self.utility.close_with(ctx.ui, screen, id);
    }
}
