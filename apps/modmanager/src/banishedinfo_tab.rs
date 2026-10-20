//! The banishedinfo.com tab: the site's list (by section and order) and search, each mod's
//! thumbnail and versions, and installing a version into the pack folder.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{mpsc, Arc};

use banished_banishedinfo::native::Client;
use banished_banishedinfo::{Error, Mod, Sort, Version, SECTIONS};
use eframe::egui::{self, Color32, RichText};

const PER_PAGE: u32 = 30;

enum Msg {
    Mods(Result<Vec<Mod>, Error>, bool),
    Versions(u32, Result<Vec<Version>, Error>),
    Thumbnail(String, Option<egui::ColorImage>),
    Progress(f32),
    Installed(String, Result<Vec<std::path::PathBuf>, Error>),
}

pub struct BanishedInfoTab {
    client: Arc<Client>,
    text: String,
    sort: Sort,
    section: usize,
    page: u32,
    /// The list shows search results (no pages).
    searching_text: bool,
    mods: Vec<Mod>,
    last_page: bool,
    loading: bool,
    versions: HashMap<u32, Vec<Version>>,
    thumbnails: HashMap<String, Option<egui::TextureHandle>>,
    installing: Option<String>,
    progress: f32,
    message: String,
    started: bool,
    send: mpsc::Sender<Msg>,
    receive: mpsc::Receiver<Msg>,
}

/// Decode a thumbnail (PNG or JPEG) for egui.
fn decode_image(bytes: &[u8]) -> Option<egui::ColorImage> {
    let i = banished_mods::preview::decode(bytes)?;
    Some(egui::ColorImage::from_rgba_unmultiplied(
        [i.width as usize, i.height as usize],
        &i.rgba,
    ))
}

impl BanishedInfoTab {
    pub fn new() -> BanishedInfoTab {
        let (send, receive) = mpsc::channel();
        BanishedInfoTab {
            client: Arc::new(Client::new(env!("CARGO_PKG_VERSION"))),
            text: String::new(),
            sort: Sort::Featured,
            section: 0,
            page: 0,
            searching_text: false,
            mods: Vec::new(),
            last_page: false,
            loading: false,
            versions: HashMap::new(),
            thumbnails: HashMap::new(),
            installing: None,
            progress: 0.0,
            message: String::new(),
            started: false,
            send,
            receive,
        }
    }

    fn spawn(
        &self,
        ctx: &egui::Context,
        job: impl FnOnce(&Client, &mpsc::Sender<Msg>) -> Msg + Send + 'static,
    ) {
        let (client, send, ctx) = (self.client.clone(), self.send.clone(), ctx.clone());
        std::thread::spawn(move || {
            let msg = job(&client, &send);
            let _ = send.send(msg);
            ctx.request_repaint();
        });
    }

    fn load(&mut self, ctx: &egui::Context) {
        self.loading = true;
        self.message.clear();
        let text = self.text.trim().to_string();
        self.searching_text = !text.is_empty();
        let (sort, section, start) = (self.sort, SECTIONS[self.section].0, self.page * PER_PAGE);
        self.spawn(ctx, move |c, _| {
            if text.is_empty() {
                let r = c.list(sort, section, start, PER_PAGE);
                let last = r.as_ref().is_ok_and(|m| m.len() < PER_PAGE as usize);
                Msg::Mods(r, last)
            } else {
                Msg::Mods(c.search(&text), true)
            }
        });
    }

    /// The thumbnail of a mod, loaded once (one request per image shown).
    fn thumbnail(&mut self, ctx: &egui::Context, url: &str) -> Option<egui::TextureHandle> {
        if let Some(t) = self.thumbnails.get(url) {
            return t.clone();
        }
        self.thumbnails.insert(url.to_string(), None);
        let url = url.to_string();
        self.spawn(ctx, move |c, _| {
            let image = c.image(&url).ok().and_then(|b| decode_image(&b));
            Msg::Thumbnail(url, image)
        });
        None
    }

    fn install(&mut self, ctx: &egui::Context, dir: &Path, m: &Mod, version: Option<Version>) {
        if self.installing.is_some() {
            return;
        }
        self.installing = Some(m.title.clone());
        self.progress = 0.0;
        self.message.clear();
        let (m, dir) = (m.clone(), dir.to_path_buf());
        self.spawn(ctx, move |c, send| {
            let version = match version {
                Some(v) => v,
                None => match c.versions(&m) {
                    Ok(v) => v[0].clone(),
                    Err(e) => return Msg::Installed(m.title.clone(), Err(e)),
                },
            };
            let r = c.install(&version, &dir, |done, total| {
                if let Some(t) = total {
                    let _ = send.send(Msg::Progress(done as f32 / t.max(1) as f32));
                }
            });
            Msg::Installed(format!("{} {}", m.title, version.label), r)
        });
    }

    /// Draw the tab. Returns `true` when an install finished, so the list is read again.
    pub fn ui(&mut self, ctx: &egui::Context, ui: &mut egui::Ui, dir: &Path) -> bool {
        let mut installed = false;
        for msg in self.receive.try_iter().collect::<Vec<_>>() {
            match msg {
                Msg::Mods(r, last) => {
                    self.loading = false;
                    match r {
                        Ok(mods) => {
                            self.mods = mods;
                            self.last_page = last;
                        }
                        Err(e) => self.message = e.to_string(),
                    }
                }
                Msg::Versions(id, Ok(v)) => {
                    self.versions.insert(id, v);
                }
                Msg::Versions(_, Err(e)) => self.message = e.to_string(),
                Msg::Thumbnail(url, image) => {
                    let t = image.map(|i| ctx.load_texture(&url, i, egui::TextureOptions::LINEAR));
                    self.thumbnails.insert(url, t);
                }
                Msg::Progress(p) => self.progress = p,
                Msg::Installed(name, r) => {
                    self.installing = None;
                    match r {
                        Ok(paths) => {
                            let files: Vec<String> = paths
                                .iter()
                                .filter_map(|p| p.file_name()?.to_str().map(str::to_string))
                                .collect();
                            self.message = format!("Installed {name}: {}", files.join(", "));
                            installed = true;
                        }
                        Err(e) => self.message = format!("{name}: {e}"),
                    }
                }
            }
        }
        if !self.started {
            self.started = true;
            self.load(ctx);
        }

        ui.label(
            RichText::new(
                "Mods from banishedinfo.com, a fan site with no account needed. Un-Banished reads \
                 its pages only when you list, search or install.",
            )
            .small()
            .weak(),
        );
        let mut go = false;
        ui.horizontal_wrapped(|ui| {
            let edit = ui.add(
                egui::TextEdit::singleline(&mut self.text)
                    .hint_text("Search banishedinfo.com")
                    .desired_width(240.0),
            );
            go |= edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            go |= ui.button("Search").clicked();
            if self.searching_text && ui.button("Show the list").clicked() {
                self.text.clear();
                go = true;
            }
            let (sort, section) = (self.sort, self.section);
            egui::ComboBox::from_id_salt("bi-sort")
                .selected_text(match self.sort {
                    Sort::Featured => "Featured",
                    Sort::Downloads => "Most downloaded",
                    Sort::Trending => "Popular today",
                    Sort::Recent => "Newest",
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.sort, Sort::Featured, "Featured");
                    ui.selectable_value(&mut self.sort, Sort::Downloads, "Most downloaded");
                    ui.selectable_value(&mut self.sort, Sort::Trending, "Popular today");
                    ui.selectable_value(&mut self.sort, Sort::Recent, "Newest");
                });
            egui::ComboBox::from_id_salt("bi-section")
                .selected_text(SECTIONS[self.section].1)
                .show_ui(ui, |ui| {
                    for (i, (_, name)) in SECTIONS.iter().enumerate() {
                        ui.selectable_value(&mut self.section, i, *name);
                    }
                });
            if (self.sort, self.section) != (sort, section) {
                self.text.clear();
                go = true;
            }
            if !self.searching_text {
                if ui
                    .add_enabled(self.page > 0 && !self.loading, egui::Button::new("◀"))
                    .clicked()
                {
                    self.page -= 1;
                    self.load(ctx);
                }
                ui.label(format!("Page {}", self.page + 1));
                if ui
                    .add_enabled(!self.last_page && !self.loading, egui::Button::new("▶"))
                    .clicked()
                {
                    self.page += 1;
                    self.load(ctx);
                }
            }
            if self.loading {
                ui.spinner();
            }
            if let Some(name) = &self.installing {
                ui.add(
                    egui::ProgressBar::new(self.progress)
                        .text(format!("Installing {name}"))
                        .desired_width(220.0),
                );
            }
        });
        if go {
            self.page = 0;
            self.load(ctx);
        }
        if !self.message.is_empty() {
            ui.label(RichText::new(&self.message).color(Color32::from_rgb(230, 170, 60)));
        }
        ui.separator();

        let mut install: Option<(Mod, Option<Version>)> = None;
        let mut want_versions: Option<Mod> = None;
        let mods = self.mods.clone();
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for m in &mods {
                    ui.horizontal(|ui| {
                        let thumb = m.thumbnail.as_deref().and_then(|u| self.thumbnail(ctx, u));
                        match thumb {
                            Some(t) => {
                                ui.add_sized(
                                    [48.0, 48.0],
                                    egui::Image::new(&t).fit_to_exact_size(egui::vec2(48.0, 48.0)),
                                );
                            }
                            None => {
                                ui.allocate_exact_size(
                                    egui::vec2(48.0, 48.0),
                                    egui::Sense::hover(),
                                );
                            }
                        }
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                ui.hyperlink_to(RichText::new(&m.title).strong(), m.page());
                                ui.label(
                                    RichText::new(format!(
                                        "by {} · version {} · {} downloads · {}% liked",
                                        m.author, m.version, m.downloads, m.rating
                                    ))
                                    .small()
                                    .weak(),
                                );
                            });
                            ui.label(m.info.lines().next().unwrap_or(""));
                            ui.horizontal(|ui| {
                                let idle = self.installing.is_none();
                                if ui
                                    .add_enabled(idle, egui::Button::new("Install newest"))
                                    .clicked()
                                {
                                    install = Some((m.clone(), None));
                                }
                                match self.versions.get(&m.id) {
                                    Some(versions) => {
                                        for v in versions.iter().take(6) {
                                            if ui
                                                .add_enabled(
                                                    idle,
                                                    egui::Button::new(format!("v{}", v.label))
                                                        .small(),
                                                )
                                                .on_hover_text("Install this version")
                                                .clicked()
                                            {
                                                install = Some((m.clone(), Some(v.clone())));
                                            }
                                        }
                                    }
                                    None => {
                                        if ui.small_button("Versions…").clicked() {
                                            want_versions = Some(m.clone());
                                        }
                                    }
                                }
                            });
                        });
                    });
                    ui.separator();
                }
                if mods.is_empty() && !self.loading {
                    ui.weak("No mods found.");
                }
            });
        if let Some(m) = want_versions {
            self.spawn(ctx, move |c, _| Msg::Versions(m.id, c.versions(&m)));
        }
        if let Some((m, v)) = install {
            self.install(ctx, dir, &m, v);
        }
        installed
    }
}
