//! The Mods dialog's Browse tab, backed by Nexus Mods instead of the Steam Workshop.
//!
//! The elements are the original's. The Steam build fills `groupBrowse` with
//! `Dialog/Mod.rsc:browser` (`FUN_14006f790`): `textSearch`, `buttonSearch`, `comboSort`,
//! `comboPage`, `buttonPrevPage` and `buttonNextPage`, `tableResult`, `textResultNone`,
//! `textResultError` and `imageWorking`. Each result is a `Dialog/Mod.rsc:searchResultGroup`
//! in the table (`FUN_14006dee0`): `textModTitle` (its parameter is the title), the preview
//! (`groupModPreview`), `buttonSubscribe` and `buttonUnsubscribe`. Below the tabs,
//! `groupProgress` shows a download (`textUpdate`, `progressUpdate` with `textProgress`) and
//! then `ribbonUpdateAvailable` with `buttonUpdateReload`.
//!
//! Deviation: there is a Browse tab for each mod site (`platform::nexus::sites`), Nexus Mods and
//! (native builds) banishedinfo.com, shown by default. The sort list keeps the original's three entries: Most Popular (most
//! downloaded), Most Recent (most recently updated) and Subscribed (the mods installed from
//! the site). Installing from Nexus Mods needs an API key; without one Subscribe opens the mod's
//! page. Subscribe installs the mod's main file with a Premium account; otherwise it
//! opens the mod's files page, whose "Mod Manager Download" gives an `nxm://` link, and a link
//! pasted into the search box installs that file. The tabs' labels read "Browse Nexus Mods" and
//! "Browse banishedinfo.com". Previews show the sites' thumbnails (PNG, JPEG or WebP, native
//! builds only). Not ported:
//! Unsubscribe, which stays hidden.

use unbanished_nexus::api::{self, ModSummary, Sort};
use unbanished_nexus::nxm::NxmLink;
use unbanished_ui::{Control, Kind, NodeId, Ui};

use crate::platform::nexus::{self, Nexus, Reply, Task, PER_PAGE};
use crate::settings::Persistence;

const BROWSER: &str = r"Dialog\Mod.rsc:browser";
const RESULT: &str = r"Dialog\Mod.rsc:searchResultGroup";

/// A text and its own string, whose `@0` the parameter replaces.
type Param = Option<(NodeId, String)>;

/// One result in the table.
struct Result {
    summary: ModSummary,
    subscribe: Option<NodeId>,
    /// The preview image (hidden until a preview arrives).
    preview: Option<NodeId>,
}

/// The order the sort list's entries stand for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Order {
    /// Most Popular.
    Popular,
    /// Most Recent.
    Recent,
    /// Subscribed: installed from Nexus Mods.
    Subscribed,
}

pub struct Browse {
    nexus: Nexus,
    search: Option<NodeId>,
    search_button: Option<NodeId>,
    sort: Option<NodeId>,
    page_combo: Option<NodeId>,
    prev: Option<NodeId>,
    next: Option<NodeId>,
    table: Option<NodeId>,
    none: Option<NodeId>,
    error: Option<NodeId>,
    working: Option<NodeId>,
    progress_group: Option<NodeId>,
    ribbon_progress: Option<NodeId>,
    ribbon_available: Option<NodeId>,
    text_update: Param,
    text_progress: Param,
    bar: Option<NodeId>,
    reload: Option<NodeId>,
    results: Vec<Result>,
    order: Order,
    text: String,
    page: u32,
    pages: u32,
    /// A search is running.
    searching: bool,
    /// The mod being installed, and its name.
    installing: Option<(u32, String)>,
    /// Whether the account has Premium, once known.
    premium: Option<bool>,
}

fn param(ui: &Ui, root: NodeId, name: &str) -> Param {
    let n = ui.find(root, name)?;
    Some((n, crate::ui::building_ui::text_of(ui, n)))
}

fn set_param(ui: &mut Ui, p: &Param, text: &str) {
    if let Some((n, template)) = p {
        ui.set_text(*n, &template.replace("@0", text));
    }
}

fn show(ui: &mut Ui, n: Option<NodeId>, visible: bool) {
    if let Some(n) = n {
        ui.set_visible(n, visible);
    }
}

/// The text typed into an edit box.
fn edit_text(ui: &Ui, n: Option<NodeId>) -> String {
    match n.map(|n| &ui.node(n).kind) {
        Some(Kind::Control(Control::Edit(e))) => e.text.text.clone(),
        _ => String::new(),
    }
}

impl Browse {
    /// One Browse tab per site (`nexus::sites`): the original's `buttonBrowse` and `groupBrowse`
    /// for the first, and a copy of both added to `tabMod` for each other site. Without sites
    /// the tab and the progress are hidden, as in the build without Steam.
    pub fn open_all(
        ui: &mut Ui,
        root: NodeId,
        settings: &Persistence,
        dir: Option<std::path::PathBuf>,
    ) -> Vec<Browse> {
        let sites = nexus::sites(settings);
        let header = ui.find(root, "buttonBrowse");
        let group = ui.find(root, "groupBrowse");
        let tabs = ui.find(root, "tabMod");
        let (Some(header), Some(group), Some(tabs), false) =
            (header, group, tabs, sites.is_empty())
        else {
            show(ui, ui.find(root, "buttonBrowse"), false);
            show(ui, ui.find(root, "groupProgress"), false);
            return Vec::new();
        };
        let (header_ref, group_ref) = (
            ui.node(header).reference.clone(),
            ui.node(group).reference.clone(),
        );
        let mut out = Vec::new();
        for (i, (site, key)) in sites.into_iter().enumerate() {
            let (header, group) = if i == 0 {
                (header, group)
            } else {
                let (Some(h), Some(g)) = (ui.instantiate(&header_ref), ui.instantiate(&group_ref))
                else {
                    continue;
                };
                ui.add_tab(tabs, h, g);
                (h, g)
            };
            if let Some(b) = Browse::open(ui, root, settings, dir.clone(), site, key, header, group)
            {
                out.push(b);
            }
        }
        out
    }

    /// Fill one Browse tab (`header` and its page `group`) for `site` and start its first
    /// search.
    #[allow(clippy::too_many_arguments)]
    fn open(
        ui: &mut Ui,
        root: NodeId,
        settings: &Persistence,
        dir: Option<std::path::PathBuf>,
        site: nexus::Site,
        key: String,
        header: NodeId,
        group: NodeId,
    ) -> Option<Browse> {
        let progress_group = ui.find(root, "groupProgress");
        if let Some(label) = ui.find(header, "labelBrowse") {
            ui.set_text(label, site.label());
        }
        let browser = ui.instantiate(BROWSER)?;
        ui.add_child(group, browser);
        let f = |ui: &Ui, n: &str| ui.find(browser, n);
        let b = Browse {
            nexus: Nexus::new(site, key, dir),
            search: f(ui, "textSearch"),
            search_button: f(ui, "buttonSearch"),
            sort: f(ui, "comboSort"),
            page_combo: f(ui, "comboPage"),
            prev: f(ui, "buttonPrevPage"),
            next: f(ui, "buttonNextPage"),
            table: f(ui, "tableResult"),
            none: f(ui, "textResultNone"),
            error: f(ui, "textResultError"),
            working: f(ui, "imageWorking"),
            progress_group,
            ribbon_progress: ui.find(root, "ribbonProgress"),
            ribbon_available: ui.find(root, "ribbonUpdateAvailable"),
            text_update: param(ui, root, "textUpdate"),
            text_progress: param(ui, root, "textProgress"),
            bar: ui.find(root, "progressUpdate"),
            reload: ui.find(root, "buttonUpdateReload"),
            results: Vec::new(),
            order: Order::Popular,
            text: String::new(),
            page: 0,
            pages: 1,
            searching: false,
            installing: None,
            premium: None,
        };
        show(ui, b.progress_group, false);
        show(ui, b.none, false);
        show(ui, b.error, false);
        // `ComboElement::SetCurrentItem(comboSort, 0)` (`FUN_14006f790`).
        if let Some(sort) = b.sort {
            ui.select_combo_item(sort, 0);
        }
        b.nexus.start(Task::Validate);
        let mut b = b;
        b.search(ui, settings);
        Some(b)
    }

    /// The ids of the mods installed from Nexus Mods (`NexusMod_<file>` in the settings).
    fn installed(
        &self,
        settings: &Persistence,
        mods: Option<&crate::mods::Modification>,
    ) -> Vec<u32> {
        let section = self.nexus.site().record_section();
        let mut ids: Vec<u32> = mods
            .into_iter()
            .flat_map(|m| m.mods.iter())
            .filter_map(|m| {
                let mut id = 0;
                settings.get_int(section, &m.file, &mut id);
                (id > 0).then_some(id as u32)
            })
            .collect();
        ids.sort_unstable();
        ids.dedup();
        ids
    }

    fn search(&mut self, ui: &mut Ui, settings: &Persistence) {
        self.search_with(ui, settings, None);
    }

    fn search_with(
        &mut self,
        ui: &mut Ui,
        settings: &Persistence,
        mods: Option<&crate::mods::Modification>,
    ) {
        let task = match self.order {
            Order::Popular | Order::Recent => Task::Search {
                text: self.text.clone(),
                sort: if self.order == Order::Popular {
                    Sort::Downloads
                } else {
                    Sort::Updated
                },
                page: self.page,
            },
            Order::Subscribed => Task::Installed(self.installed(settings, mods)),
        };
        if let Task::Installed(ids) = &task {
            if ids.is_empty() {
                self.show_results(ui, Vec::new(), 0);
                return;
            }
        }
        self.searching = true;
        show(ui, self.working, true);
        show(ui, self.none, false);
        show(ui, self.error, false);
        self.nexus.start(task);
    }

    /// Put a page of results in the table (`FUN_14006dee0`): the page list, then one result
    /// group per mod, row by row.
    fn show_results(&mut self, ui: &mut Ui, mods: Vec<ModSummary>, total: u32) {
        self.searching = false;
        show(ui, self.working, false);
        self.pages = total.div_ceil(PER_PAGE).max(1);
        if let Some(combo) = self.page_combo {
            ui.set_choices(combo, (1..=self.pages).map(|p| p.to_string()).collect());
            ui.select_combo_item(combo, self.page as usize);
        }
        if let Some(n) = self.prev {
            ui.node_mut(n).enabled = self.page > 0;
        }
        if let Some(n) = self.next {
            ui.node_mut(n).enabled = self.page + 1 < self.pages;
        }
        if let Some(table) = self.table {
            if let Kind::Control(Control::Table { cells, rows, .. }) = &mut ui.node_mut(table).kind
            {
                cells.clear();
                rows.clear();
            }
        }
        self.results.clear();
        show(ui, self.none, mods.is_empty());
        for (i, m) in mods.into_iter().enumerate() {
            let Some(group) = ui.instantiate(RESULT) else {
                continue;
            };
            if let Some(table) = self.table {
                ui.set_table_child(table, i, group);
            }
            let title = param(ui, group, "textModTitle");
            set_param(ui, &title, &m.name);
            if let Some(t) = title.map(|t| t.0) {
                ui.node_mut(t).tooltip = format!("{} (by {})\n{}", m.name, m.author, m.summary);
            }
            show(ui, ui.find(group, "buttonUnsubscribe"), false);
            // The loading image stands in for a preview until one arrives.
            let preview = ui.find(group, "imageModPreview");
            show(ui, preview, false);
            if let Some(url) = &m.thumbnail {
                self.nexus.start(Task::Preview {
                    mod_id: m.mod_id,
                    url: url.clone(),
                });
            }
            self.results.push(Result {
                summary: m,
                subscribe: ui.find(group, "buttonSubscribe"),
                preview,
            });
        }
    }

    /// Start installing `mod_id` (or the file a link names).
    fn install(&mut self, ui: &mut Ui, mod_id: u32, name: String, link: Option<NxmLink>) {
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
    fn needs_link(&mut self, ui: &mut Ui, mod_id: u32) {
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

    /// Whether `node` is one of the tab's controls; handles it when it is. Returns `true` when
    /// the Reload button was pressed.
    pub fn click(
        &mut self,
        ui: &mut Ui,
        settings: &Persistence,
        mods: &crate::mods::Modification,
        node: NodeId,
    ) -> Option<bool> {
        let on = |n: Option<NodeId>| n.is_some_and(|b| ui.descendant(b, node));
        if on(self.reload) {
            return Some(true);
        }
        if on(self.search_button) {
            let text = edit_text(ui, self.search).trim().to_string();
            if text.starts_with("nxm://") {
                match NxmLink::parse(&text) {
                    Ok(link) => {
                        self.install(ui, link.mod_id, format!("{}", link.mod_id), Some(link))
                    }
                    Err(e) => log::warn!("{e}"),
                }
                return Some(false);
            }
            self.text = text;
            self.page = 0;
            self.search_with(ui, settings, Some(mods));
            return Some(false);
        }
        if on(self.prev) && self.page > 0 {
            self.page -= 1;
            self.search_with(ui, settings, Some(mods));
            return Some(false);
        }
        if on(self.next) && self.page + 1 < self.pages {
            self.page += 1;
            self.search_with(ui, settings, Some(mods));
            return Some(false);
        }
        if let Some(i) = self.results.iter().position(|r| on(r.subscribe)) {
            let m = &self.results[i].summary;
            let (id, name) = (m.mod_id, m.name.clone());
            self.install(ui, id, name, None);
            return Some(false);
        }
        if on(self.sort) || on(self.page_combo) || on(self.search) {
            return Some(false);
        }
        None
    }

    /// A combo's choice (`Event::Selected`): the sort order or the page.
    pub fn selected(
        &mut self,
        ui: &mut Ui,
        settings: &Persistence,
        mods: &crate::mods::Modification,
        node: NodeId,
        index: usize,
    ) -> bool {
        let on = |n: Option<NodeId>| n.is_some_and(|b| ui.descendant(b, node));
        if on(self.sort) {
            self.order = match index {
                0 => Order::Popular,
                1 => Order::Recent,
                _ => Order::Subscribed,
            };
            self.page = 0;
            self.search_with(ui, settings, Some(mods));
            return true;
        }
        if on(self.page_combo) && index as u32 != self.page {
            self.page = index as u32;
            self.search_with(ui, settings, Some(mods));
            return true;
        }
        false
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
                                *s = Some(sprite.clone());
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
