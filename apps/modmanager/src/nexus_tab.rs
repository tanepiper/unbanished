//! The Nexus Mods tab: the API key, searching, and installing a mod's main file (Premium) or
//! the file an `nxm://` link names, with the same crate as the game's Browse tab.

use std::path::Path;
use std::sync::mpsc;

use banished_mods::text::TextSettings;
use banished_nexus::api::{self, ModSummary, Sort, User};
use banished_nexus::native::{self, Source, Step};
use banished_nexus::nxm::NxmLink;
use banished_nexus::{App, Error};
use eframe::egui::{self, Color32, RichText};

const PER_PAGE: u32 = 30;

enum Msg {
    User(Result<User, Error>),
    Results(Result<api::SearchPage, Error>),
    Progress(f32),
    Installed(Result<Vec<std::path::PathBuf>, Error>, u32),
    Thumbnail(String, Option<egui::ColorImage>),
}

pub struct NexusTab {
    key: Option<String>,
    key_edit: String,
    user: Option<User>,
    text: String,
    sort: Sort,
    page: u32,
    results: Vec<ModSummary>,
    total: u32,
    searching: bool,
    installing: Option<String>,
    progress: f32,
    message: String,
    link: String,
    started: bool,
    thumbnails: std::collections::HashMap<String, Option<egui::TextureHandle>>,
    send: mpsc::Sender<Msg>,
    receive: mpsc::Receiver<Msg>,
}

fn app() -> App {
    App::un_banished(env!("CARGO_PKG_VERSION"))
}

impl NexusTab {
    pub fn new() -> NexusTab {
        let (send, receive) = mpsc::channel();
        NexusTab {
            key: native::stored_key(),
            key_edit: String::new(),
            user: None,
            text: String::new(),
            sort: Sort::Downloads,
            page: 0,
            results: Vec::new(),
            total: 0,
            searching: false,
            installing: None,
            progress: 0.0,
            message: String::new(),
            link: String::new(),
            started: false,
            thumbnails: std::collections::HashMap::new(),
            send,
            receive,
        }
    }

    fn spawn(
        &self,
        ctx: &egui::Context,
        job: impl FnOnce(&mpsc::Sender<Msg>) -> Msg + Send + 'static,
    ) {
        let send = self.send.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let msg = job(&send);
            let _ = send.send(msg);
            ctx.request_repaint();
        });
    }

    /// A mod's thumbnail, loaded once (Nexus Mods serves them as WebP).
    fn thumbnail(&mut self, ctx: &egui::Context, url: &str) -> Option<egui::TextureHandle> {
        if let Some(t) = self.thumbnails.get(url) {
            return t.clone();
        }
        self.thumbnails.insert(url.to_string(), None);
        let url = url.to_string();
        self.spawn(ctx, move |_| {
            let image = native::download(&url, |_, _| {})
                .ok()
                .and_then(|b| banished_mods::preview::decode(&b))
                .map(|i| {
                    egui::ColorImage::from_rgba_unmultiplied(
                        [i.width as usize, i.height as usize],
                        &i.rgba,
                    )
                });
            Msg::Thumbnail(url, image)
        });
        None
    }

    fn validate(&mut self, ctx: &egui::Context) {
        let Some(key) = self.key.clone() else { return };
        self.spawn(ctx, move |_| {
            Msg::User(
                native::send(&api::validate(&app(), &key), false).and_then(|b| api::parse_user(&b)),
            )
        });
    }

    fn search(&mut self, ctx: &egui::Context) {
        self.searching = true;
        let (text, sort, offset) = (self.text.clone(), self.sort, self.page * PER_PAGE);
        self.spawn(ctx, move |_| {
            let request = api::search(&app(), &text, sort, offset, PER_PAGE);
            Msg::Results(native::send(&request, false).and_then(|b| api::parse_search(&b)))
        });
    }

    fn install(
        &mut self,
        ctx: &egui::Context,
        dir: &Path,
        mod_id: u32,
        name: String,
        link: Option<NxmLink>,
    ) {
        let Some(key) = self.key.clone() else {
            self.message = "Set an API key first.".into();
            return;
        };
        if self.installing.is_some() {
            return;
        }
        if link.is_none() && self.user.as_ref().is_some_and(|u| !u.premium) {
            self.needs_link(mod_id);
            return;
        }
        self.installing = Some(name);
        self.progress = 0.0;
        self.message.clear();
        let dir = dir.to_path_buf();
        self.spawn(ctx, move |send| {
            let app = app();
            let source = match link {
                Some(l) => Source::Link(l),
                None => {
                    let files = native::send(&api::files(&app, mod_id), false)
                        .and_then(|b| api::parse_files(&b));
                    let file = match files {
                        Ok(f) => api::main_file(&f).cloned(),
                        Err(e) => return Msg::Installed(Err(e), mod_id),
                    };
                    let Some(file) = file else {
                        return Msg::Installed(Err(Error::NoPackage), mod_id);
                    };
                    Source::File {
                        mod_id,
                        file_id: file.file_id,
                        file_name: file.file_name,
                    }
                }
            };
            let result = native::install_from(&app, &key, &source, &dir, |step| {
                if let Step::Download(done, Some(total)) = step {
                    let _ = send.send(Msg::Progress(done as f32 / total.max(1) as f32));
                }
            });
            Msg::Installed(result, mod_id)
        });
    }

    fn needs_link(&mut self, mod_id: u32) {
        let url = api::files_page(mod_id);
        open_in_browser(&url);
        self.message =
            "Without Premium, Nexus Mods only allows downloads from its website. On the \
            page that opened, press Mod Manager Download, copy the nxm:// link, and paste it \
            below."
                .into();
    }

    fn save_key(&mut self, settings_path: Option<&Path>) {
        let key = self.key_edit.trim().to_string();
        let Some(path) = settings_path else {
            self.message = "No settings file to save the key in.".into();
            return;
        };
        let mut s = TextSettings::load(path).unwrap_or_default();
        s.set_text("Nexus", "ApiKey", &key);
        match s.save(path) {
            Ok(()) => {
                self.key = (!key.is_empty()).then_some(key);
                self.key_edit.clear();
                self.user = None;
                self.message = "Key saved.".into();
            }
            Err(e) => self.message = format!("Could not save the key: {e}"),
        }
    }

    /// Draw the tab. Returns `true` when an install finished, so the list is read again.
    pub fn ui(
        &mut self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        dir: &Path,
        settings_path: Option<&Path>,
    ) -> bool {
        let mut installed = false;
        for msg in self.receive.try_iter().collect::<Vec<_>>() {
            match msg {
                Msg::User(Ok(u)) => self.user = Some(u),
                Msg::User(Err(e)) => self.message = e.to_string(),
                Msg::Results(Ok(page)) => {
                    self.searching = false;
                    self.results = page.mods;
                    self.total = page.total;
                }
                Msg::Results(Err(e)) => {
                    self.searching = false;
                    self.message = e.to_string();
                }
                Msg::Progress(p) => self.progress = p,
                Msg::Thumbnail(url, image) => {
                    let t = image.map(|i| ctx.load_texture(&url, i, egui::TextureOptions::LINEAR));
                    self.thumbnails.insert(url, t);
                }
                Msg::Installed(result, mod_id) => {
                    let name = self.installing.take().unwrap_or_default();
                    match result {
                        Ok(paths) => {
                            let files: Vec<String> = paths
                                .iter()
                                .filter_map(|p| p.file_name()?.to_str().map(str::to_string))
                                .collect();
                            self.message = format!("Installed {name}: {}", files.join(", "));
                            installed = true;
                        }
                        Err(Error::NeedsPremium) => {
                            if let Some(u) = &mut self.user {
                                u.premium = false;
                            }
                            self.needs_link(mod_id);
                        }
                        Err(e) => self.message = format!("{name}: {e}"),
                    }
                }
            }
        }
        if !self.started {
            self.started = true;
            self.validate(ctx);
            self.search(ctx);
        }

        // The key.
        ui.horizontal(|ui| {
            match (&self.key, &self.user) {
                (None, _) => {
                    ui.colored_label(Color32::from_rgb(230, 170, 60), "No API key.");
                }
                (Some(_), Some(u)) => {
                    ui.label(format!(
                        "Signed in as {} ({})",
                        u.name,
                        if u.premium {
                            "Premium: direct installs"
                        } else {
                            "not Premium: installs through the website"
                        }
                    ));
                }
                (Some(_), None) => {
                    ui.label("API key set.");
                }
            }
            ui.add(
                egui::TextEdit::singleline(&mut self.key_edit)
                    .password(true)
                    .hint_text("Paste your personal API key")
                    .desired_width(240.0),
            );
            if ui.button("Save key").clicked() {
                self.save_key(settings_path);
                self.validate(ctx);
            }
            ui.hyperlink_to("Get a key", "https://www.nexusmods.com/settings/api-keys");
        });
        ui.label(
            RichText::new(
                "Your key stays on this computer and is sent only to api.nexusmods.com. A \
                 personal key is for your own use.",
            )
            .small()
            .weak(),
        );
        ui.separator();

        // Search.
        let mut go = false;
        ui.horizontal(|ui| {
            let edit = ui.add(
                egui::TextEdit::singleline(&mut self.text)
                    .hint_text("Search Banished mods")
                    .desired_width(260.0),
            );
            go |= edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            go |= ui.button("Search").clicked();
            let before = self.sort;
            egui::ComboBox::from_id_salt("sort")
                .selected_text(match self.sort {
                    Sort::Downloads => "Most downloaded",
                    Sort::Endorsements => "Most endorsed",
                    Sort::Updated => "Recently updated",
                    Sort::Name => "Name",
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.sort, Sort::Downloads, "Most downloaded");
                    ui.selectable_value(&mut self.sort, Sort::Endorsements, "Most endorsed");
                    ui.selectable_value(&mut self.sort, Sort::Updated, "Recently updated");
                    ui.selectable_value(&mut self.sort, Sort::Name, "Name");
                });
            if self.sort != before {
                go = true;
            }
            let pages = self.total.div_ceil(PER_PAGE).max(1);
            if ui
                .add_enabled(self.page > 0, egui::Button::new("◀"))
                .clicked()
            {
                self.page -= 1;
                self.search(ctx);
            }
            ui.label(format!(
                "Page {} of {pages} ({} mods)",
                self.page + 1,
                self.total
            ));
            if ui
                .add_enabled(self.page + 1 < pages, egui::Button::new("▶"))
                .clicked()
            {
                self.page += 1;
                self.search(ctx);
            }
            if self.searching {
                ui.spinner();
            }
        });
        if go {
            self.page = 0;
            self.search(ctx);
        }

        // An nxm:// link, and the progress of an install.
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.link)
                    .hint_text("nxm:// link from Mod Manager Download")
                    .desired_width(360.0),
            );
            if ui
                .add_enabled(self.installing.is_none(), egui::Button::new("Install link"))
                .clicked()
            {
                match NxmLink::parse(&self.link) {
                    Ok(l) => {
                        let id = l.mod_id;
                        self.install(ctx, dir, id, format!("mod {id}"), Some(l));
                        self.link.clear();
                    }
                    Err(e) => self.message = e.to_string(),
                }
            }
            if let Some(name) = &self.installing {
                ui.add(
                    egui::ProgressBar::new(self.progress)
                        .text(format!("Installing {name}"))
                        .desired_width(220.0),
                );
            }
        });
        if !self.message.is_empty() {
            ui.label(RichText::new(&self.message).color(Color32::from_rgb(230, 170, 60)));
        }
        ui.separator();

        // The results.
        let mut install: Option<(u32, String)> = None;
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let results = self.results.clone();
                for m in &results {
                    ui.horizontal(|ui| {
                        let thumb = m.thumbnail.as_deref().and_then(|u| self.thumbnail(ctx, u));
                        match thumb {
                            Some(t) => {
                                ui.add_sized(
                                    [85.0, 48.0],
                                    egui::Image::new(&t).fit_to_exact_size(egui::vec2(85.0, 48.0)),
                                );
                            }
                            None => {
                                ui.allocate_exact_size(
                                    egui::vec2(85.0, 48.0),
                                    egui::Sense::hover(),
                                );
                            }
                        }
                        if ui
                            .add_enabled(
                                self.installing.is_none() && self.key.is_some(),
                                egui::Button::new("Install"),
                            )
                            .on_disabled_hover_text("Set an API key to install")
                            .clicked()
                        {
                            install = Some((m.mod_id, m.name.clone()));
                        }
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                ui.hyperlink_to(RichText::new(&m.name).strong(), m.page());
                                ui.label(
                                    RichText::new(format!(
                                        "by {} · version {} · {} downloads · {} endorsements",
                                        m.author, m.version, m.downloads, m.endorsements
                                    ))
                                    .small()
                                    .weak(),
                                );
                            });
                            ui.label(&m.summary);
                        });
                    });
                    ui.separator();
                }
                if self.results.is_empty() && !self.searching {
                    ui.weak("No mods found.");
                }
            });
        if let Some((id, name)) = install {
            self.install(ctx, dir, id, name, None);
        }
        installed
    }
}

/// Open an address in the web browser.
fn open_in_browser(url: &str) {
    let result = if cfg!(target_os = "macos") {
        std::process::Command::new("open").arg(url).spawn()
    } else if cfg!(windows) {
        std::process::Command::new("cmd")
            .args(["/C", "start", "", url])
            .spawn()
    } else {
        std::process::Command::new("xdg-open").arg(url).spawn()
    };
    if let Err(e) = result {
        eprintln!("could not open {url}: {e}");
    }
}
