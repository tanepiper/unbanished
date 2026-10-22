//! Installing a mod: the download, the link it needs and the replies.

use super::*;

impl Browse {
    /// Start installing `mod_id` (or the file a link names).
    pub(super) fn install(
        &mut self,
        ui: &mut Ui,
        mod_id: u32,
        name: String,
        link: Option<NxmLink>,
    ) {
        if self.installing.is_some() {
            return;
        }
        if !self.nexus.can_install() {
            // Nexus Mods without a key: the mod's page, and what installing needs.
            nexus::open_in_browser(&api::files_page(mod_id));
            show(ui, self.progress_group, true);
            show(ui, self.ribbon_progress, true);
            show(ui, self.ribbon_available, false);
            if let Some((n, _)) = &self.text_update {
                ui.set_text(
                    *n,
                    "Installing from Nexus Mods needs your API key: add NexusApiKey to the settings.",
                );
            }
            show(ui, self.text_progress.as_ref().map(|t| t.0), false);
            return;
        }
        if link.is_none() && self.premium == Some(false) {
            self.needs_link(ui, mod_id);
            return;
        }
        self.installing = Some((mod_id, name.clone()));
        show(ui, self.progress_group, true);
        show(ui, self.ribbon_progress, true);
        show(ui, self.ribbon_available, false);
        set_param(ui, &self.text_update, &name);
        set_param(ui, &self.text_progress, "0");
        if let Some(bar) = self.bar {
            ui.set_value(bar, 0.0);
        }
        self.nexus.start(Task::Install { mod_id, link });
    }

    /// Without Premium: the mod's files page, and how to finish from there.
    pub(super) fn needs_link(&mut self, ui: &mut Ui, mod_id: u32) {
        nexus::open_in_browser(&api::files_page(mod_id));
        show(ui, self.progress_group, true);
        show(ui, self.ribbon_progress, true);
        show(ui, self.ribbon_available, false);
        if let Some((n, _)) = &self.text_update {
            ui.set_text(
                *n,
                "On the page, press Mod Manager Download, then paste the nxm:// link in the search box.",
            );
        }
        show(ui, self.text_progress.as_ref().map(|t| t.0), false);
    }

    /// Collect the background's replies: results, progress, and finished installs, which join
    /// the mod list (`Modification::add`) and are recorded as installed from Nexus Mods.
    pub fn poll(
        &mut self,
        ui: &mut Ui,
        settings: &mut Persistence,
        mods: &mut crate::mods::Modification,
    ) {
        for reply in self.nexus.poll() {
            match reply {
                Reply::User(u) => {
                    log::info!(
                        "Nexus Mods: signed in as {} ({})",
                        u.name,
                        if u.premium { "Premium" } else { "not Premium" }
                    );
                    self.premium = Some(u.premium);
                }
                Reply::Results(page) => self.show_results(ui, page.mods, page.total),
                Reply::Progress(done, total) => {
                    let fraction = total.map_or(0.0, |t| done as f32 / t.max(1) as f32);
                    set_param(
                        ui,
                        &self.text_progress,
                        &format!("{}", (fraction * 100.0) as u32),
                    );
                    if let Some(bar) = self.bar {
                        ui.set_value(bar, fraction);
                    }
                }
                Reply::Installed(mod_id, found) => {
                    self.installing = None;
                    for (file, _) in &found {
                        settings.set_int(self.nexus.site().record_section(), file, mod_id as i32);
                    }
                    log::info!(
                        "Nexus Mods: installed mod {mod_id}: {} package(s)",
                        found.len()
                    );
                    mods.add(found, settings);
                    show(ui, self.ribbon_progress, false);
                    show(ui, self.ribbon_available, true);
                }
                Reply::Preview(mod_id, w, h, rgba) => {
                    let key = format!("modpreview:{mod_id}");
                    let sprite = ui.res.image_sheet(&key, w, h, rgba);
                    for r in self.results.iter().filter(|r| r.summary.mod_id == mod_id) {
                        if let Some(n) = r.preview {
                            if let Kind::Image { sprite: s, .. } = &mut ui.node_mut(n).kind {
                                *s = Some(sprite);
                            }
                            ui.set_visible(n, true);
                        }
                    }
                }
                Reply::Failed(message) if message.is_empty() => {}
                Reply::NeedsLink(mod_id) => {
                    self.installing = None;
                    self.premium = Some(false);
                    self.needs_link(ui, mod_id);
                }
                Reply::Failed(message) => {
                    log::warn!("Nexus Mods: {message}");
                    if let Some((_, _)) = self.installing.take() {
                        if let Some((n, _)) = &self.text_update {
                            ui.set_text(*n, &message);
                        }
                        show(ui, self.text_progress.as_ref().map(|t| t.0), false);
                    } else if self.searching {
                        self.searching = false;
                        show(ui, self.working, false);
                        show(ui, self.error, true);
                    }
                }
            }
        }
    }
}
