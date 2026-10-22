//! The Installed tab: the list of mods and the selected mod's details.

use super::*;

impl App {
    pub(super) fn installed_tab(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        self.list_tools(ui);
        ui.separator();
        let filter = self.filter.trim().to_lowercase();
        let mut dropped: Option<(usize, usize)> = None;
        let mut action: Option<(usize, i32)> = None;
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let count = self.mods.mods.len();
                for i in 0..count {
                    let m = &self.mods.mods[i];
                    if !filter.is_empty()
                        && !m.name.to_lowercase().contains(&filter)
                        && !m.author.to_lowercase().contains(&filter)
                        && !m.file.to_lowercase().contains(&filter)
                    {
                        continue;
                    }
                    let icon = self.icon(ctx, i);
                    let m = &self.mods.mods[i];
                    let (name, author, file) = (m.name.clone(), m.author.clone(), m.file.clone());
                    let version =
                        format!("{} ({})", m.user_version, version_string(m.game_version));
                    let mut enabled = m.enabled;
                    let conflict = self.mods.in_conflict(i);
                    let selected = self.selected == Some(i);
                    let row = ui
                        .dnd_drag_source(ui.scope_id().with(("mod", i)), i, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("⠿").weak())
                                    .on_hover_text("Drag to change the order");
                                if ui.add_enabled(i > 0, egui::Button::new("⤒").small()).on_hover_text("To the top: it wins over the others").clicked() {
                                    action = Some((i, 1));
                                }
                                if ui.add_enabled(i > 0, egui::Button::new("↑").small()).on_hover_text("Up").clicked() {
                                    action = Some((i, 2));
                                }
                                if ui.add_enabled(i + 1 < count, egui::Button::new("↓").small()).on_hover_text("Down").clicked() {
                                    action = Some((i, 3));
                                }
                                if ui.checkbox(&mut enabled, "").on_hover_text("On or off").changed() {
                                    action = Some((i, if enabled { 10 } else { 11 }));
                                }
                                match icon {
                                    Some(t) => {
                                        ui.add(egui::Image::new(&t).fit_to_exact_size(egui::vec2(32.0, 32.0)));
                                    }
                                    None => {
                                        ui.add_space(36.0);
                                    }
                                }
                                ui.vertical(|ui| {
                                    ui.horizontal(|ui| {
                                        let title = RichText::new(&name).strong();
                                        if ui.selectable_label(selected, title).clicked() {
                                            action = Some((i, 0));
                                        }
                                        if conflict {
                                            ui.colored_label(Color32::from_rgb(230, 170, 60), "⚠ conflict")
                                                .on_hover_text("Another enabled mod changes some of the same files");
                                        }
                                    });
                                    ui.label(
                                        RichText::new(format!("{author} · {version} · {file}.pkm"))
                                            .small()
                                            .weak(),
                                    );
                                });
                            });
                        })
                        .response;
                    if let Some(from) = row.dnd_release_payload::<usize>() {
                        dropped = Some((*from, i));
                    }
                    ui.separator();
                }
                if count == 0 {
                    ui.label("No .pkm mods in this folder. Put mods in WinData, or install them from the Nexus Mods tab.");
                }
            });
        if let Some((i, what)) = action {
            match what {
                0 => self.selected = Some(i),
                1 => self.move_mod(i, 0),
                2 => self.move_mod(i, i - 1),
                3 => self.move_mod(i, i + 1),
                10 => self.mods.enable(i, true),
                11 => self.mods.enable(i, false),
                _ => {}
            }
        }
        if let Some((from, to)) = dropped {
            if from != to {
                self.move_mod(from, to);
            }
        }
    }

    pub(super) fn details(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        let Some(i) = self.selected.filter(|&i| i < self.mods.mods.len()) else {
            ui.weak("Select a mod to see its details.");
            ui.add_space(8.0);
            ui.weak("The first mod in the list wins when two mods change the same file.");
            return;
        };
        let icon = self.icon(ctx, i);
        let m = &self.mods.mods[i];
        ui.horizontal(|ui| {
            if let Some(t) = icon {
                ui.add(egui::Image::new(&t).fit_to_exact_size(egui::vec2(48.0, 48.0)));
            }
            ui.vertical(|ui| {
                ui.heading(&m.name);
                ui.label(format!("by {}", m.author));
            });
        });
        ui.label(format!(
            "Version {} (made for Banished {})",
            m.user_version,
            version_string(m.game_version)
        ));
        ui.label(
            RichText::new(format!("WinData/{}.pkm", m.file))
                .small()
                .weak(),
        );
        ui.separator();
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.label(&m.description);
            if !m.conflicts.is_empty() {
                ui.separator();
                ui.strong("Shares files with");
                for f in &m.conflicts {
                    let on = self.mods.mods.iter().any(|o| &o.file == f && o.enabled);
                    let name = self
                        .mods
                        .mods
                        .iter()
                        .find(|o| &o.file == f)
                        .map_or(f.as_str(), |o| o.name.as_str());
                    let text = if on {
                        RichText::new(format!("{name} (on)")).color(Color32::from_rgb(230, 170, 60))
                    } else {
                        RichText::new(format!("{name} (off)")).weak()
                    };
                    ui.label(text);
                }
            }
            ui.separator();
            ui.collapsing(format!("Files ({})", m.names.len()), |ui| {
                for n in &m.names {
                    ui.label(RichText::new(n).small().monospace());
                }
            });
        });
    }
}
