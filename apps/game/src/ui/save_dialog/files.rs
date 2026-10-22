//! Asking to confirm, and saving, loading and deleting a save.

use super::*;

impl SaveLoadDialog {
    pub(super) fn ask(
        &mut self,
        ui: &mut Ui,
        screen: &mut Screen,
        name: &str,
        c: Confirm,
        params: &[&str],
    ) {
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
                let mut t = crate::ui::building_ui::text_of(ui, n);
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
    pub(super) fn save(&mut self) -> Outcome {
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
    pub(super) fn load(
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
            // Deviation: a rule hook's entry is not a mod (`unbanished_sim::hooks`).
            .filter(|(_, m)| !unbanished_sim::hooks::is_saved_hook(m))
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
                crate::ui::mod_ui::version_string(m.values[1])
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
        self.reading = Some(crate::saves::save_store::read(&file));
        self.start(Operation::Load, SAVE_LOAD_TIME);
        Outcome::Stay
    }

    /// Operation 5.
    pub(super) fn delete(&mut self) {
        if let Some(file) = self.selected_file() {
            crate::saves::save_store::delete(file);
        }
        self.deleting = true;
        self.start(Operation::Delete, 0.0);
    }
}
