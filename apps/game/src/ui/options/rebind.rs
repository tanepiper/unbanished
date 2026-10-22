//! Rebinding a key: the bind dialogs and the key they wait for.

use super::*;

impl OptionsDialog {
    /// `FUN_140077e70`: show `inputBind` for action `row + 5`.
    pub(super) fn start_rebind(
        &mut self,
        ctx: &mut Ctx,
        screen: &mut Screen,
        row: usize,
        slot: usize,
    ) {
        let Some(index) = screen.add_modal(ctx.ui, BIND_DIALOG) else {
            return;
        };
        let root = screen.dialogs[index].root;
        let name = action_name(ctx.ui, FIRST_KEY + row);
        if let Some(text) = ctx.ui.find(root, "inputBindText") {
            ctx.ui.set_parameters(text, &[(0, &name)]);
        }
        self.rebind = Some(Rebind::Waiting {
            root,
            row,
            slot,
            armed: false,
        });
    }

    pub(super) fn close_rebind(&mut self, ctx: &mut Ctx, screen: &mut Screen) {
        if let Some(Rebind::Waiting { root, .. } | Rebind::Confirm { root, .. }) =
            self.rebind.take()
        {
            screen.remove_root(ctx.ui, root);
        }
    }

    /// Message box buttons: `inputBind` Unbind (result 2) or Cancel; `inputBound` Yes (2) or
    /// No (3).
    pub(super) fn rebind_click(&mut self, ctx: &mut Ctx, screen: &mut Screen, id: NodeId) {
        let Some(state) = &self.rebind else { return };
        let (root, row, slot) = match *state {
            Rebind::Waiting {
                root, row, slot, ..
            }
            | Rebind::Confirm {
                root, row, slot, ..
            } => (root, row, slot),
        };
        let is = |name: &str| {
            ctx.ui
                .find(root, name)
                .is_some_and(|b| ctx.ui.descendant(b, id))
        };
        let yes = is("buttonYes") || is("buttonOk");
        let no = is("buttonNo") || is("buttonCancel");
        if !yes && !no {
            return;
        }
        let id_ = FIRST_KEY + row;
        match *state {
            Rebind::Waiting { .. } => {
                if yes {
                    // Result 2: `KeyMap::ClearInput(id, slot)` and the button shows "Unbound".
                    ctx.settings.keymap.set(id_, slot, None);
                    self.set_key_text(ctx, row, slot);
                }
                self.close_rebind(ctx, screen);
            }
            Rebind::Confirm { binding, other, .. } => {
                self.close_rebind(ctx, screen);
                if yes {
                    ctx.settings.keymap.clear_input(other, binding);
                    ctx.settings.keymap.set(id_, slot, Some(binding));
                }
                self.refresh_keys(ctx);
            }
        }
    }

    pub(super) fn set_key_text(&self, ctx: &mut Ctx, row: usize, slot: usize) {
        let Some(&button) = self.el.keys.get(row).map(|b| &b[slot]) else {
            return;
        };
        let binding = ctx.settings.keymap.binding(FIRST_KEY + row, slot);
        let text = key_name(ctx.ui, binding);
        if let Some(t) = ctx.ui.find(button, "textInputKey") {
            ctx.ui.set_text(t, &text);
        }
    }

    /// A released key (device 1, codes 0–0x68) or mouse button (device 2, codes 4–11) while
    /// the bind box waits: bind it, or ask when another action already uses it.
    pub fn released(&mut self, ctx: &mut Ctx, screen: &mut Screen, binding: Binding) {
        let Some(Rebind::Waiting {
            row, slot, armed, ..
        }) = self.rebind
        else {
            return;
        };
        let valid = match binding.0 {
            keymap::KEYBOARD => binding.1 < 0x69,
            keymap::MOUSE => (4..=0xb).contains(&binding.1),
            _ => false,
        };
        if !armed || !valid {
            return;
        }
        self.close_rebind(ctx, screen);
        let id = FIRST_KEY + row;
        let other = ctx.settings.keymap.id_for(binding);
        let this_name = action_name(ctx.ui, id);
        match other {
            Some(other) if action_name(ctx.ui, other) != this_name => {
                let Some(index) = screen.add_modal(ctx.ui, BOUND_DIALOG) else {
                    return;
                };
                let root = screen.dialogs[index].root;
                let key = key_name(ctx.ui, Some(binding));
                let other_name = action_name(ctx.ui, other);
                if let Some(text) = ctx.ui.find(root, "inputBoundText") {
                    ctx.ui
                        .set_parameters(text, &[(0, &key), (1, &other_name), (2, &this_name)]);
                }
                self.rebind = Some(Rebind::Confirm {
                    root,
                    row,
                    slot,
                    binding,
                    other,
                });
            }
            other => {
                // The same action name (or no other action): replace without asking.
                if let Some(other) = other {
                    ctx.settings.keymap.clear_input(other, binding);
                }
                ctx.settings.keymap.set(id, slot, Some(binding));
                self.refresh_keys(ctx);
            }
        }
    }

    /// Escape while a message box is open closes it (its Cancel/No); otherwise the dialog's
    /// Cancel shortcut handles it.
    pub fn escape(&mut self, ctx: &mut Ctx, screen: &mut Screen) -> Outcome {
        if self.rebind.is_some() {
            if matches!(self.rebind, Some(Rebind::Confirm { .. })) {
                self.close_rebind(ctx, screen);
                self.refresh_keys(ctx);
            }
            // While waiting, Escape is a key like any other (bound on release).
            return Outcome::Stay;
        }
        self.cancel(ctx);
        self.close(ctx, screen);
        Outcome::Closed
    }
}
