//! The window: the installed mods (on, off, order, details), mod lists, the Nexus Mods tab,
//! and Save, Revert and Play.

use std::collections::HashMap;
use std::path::PathBuf;

use banished_mods::text::{self, TextSettings};
use banished_mods::Modification;
use eframe::egui::{self, Color32, RichText};

use crate::launch;
use crate::lists;
use crate::nexus_tab::NexusTab;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Installed,
    Nexus,
}

pub struct App {
    dir: PathBuf,
    dir_edit: String,
    settings_path: Option<PathBuf>,
    settings: TextSettings,
    mods: Modification,
    icons: HashMap<String, egui::TextureHandle>,
    selected: Option<usize>,
    filter: String,
    tab: Tab,
    status: String,
    list_name: String,
    lists: Vec<String>,
    nexus: NexusTab,
    /// Test captures: `BANISHED_MODS_SCREENSHOT=out.png` saves the window after a few seconds
    /// and quits (`BANISHED_MODS_TAB=nexus` and `BANISHED_MODS_SELECT=N` set up the view).
    capture: Option<(PathBuf, std::time::Instant, bool)>,
}

/// Write a screenshot as PNG.
fn write_png(path: &std::path::Path, image: &egui::ColorImage) -> std::io::Result<()> {
    let file = std::io::BufWriter::new(std::fs::File::create(path)?);
    let mut encoder = png::Encoder::new(file, image.size[0] as u32, image.size[1] as u32);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(std::io::Error::other)?;
    let bytes: Vec<u8> = image.pixels.iter().flat_map(|c| c.to_array()).collect();
    writer
        .write_image_data(&bytes)
        .map_err(std::io::Error::other)
}

/// `Configuration::GetVersionString(version, true)`, as the game's Mods dialog shows it.
fn version_string(version: u32) -> String {
    let stage = match version & 0xff {
        0xfb => "R",
        0xfc => "P",
        0xfd => "A",
        0xfe => "B",
        _ => "",
    };
    format!(
        "{}.{}.{}{stage}",
        version >> 24,
        (version >> 16) & 0xff,
        (version >> 8) & 0xff
    )
}

impl App {
    pub fn new(_cc: &eframe::CreationContext<'_>, args: &[String]) -> App {
        let settings_path = text::settings_path();
        let settings = settings_path
            .as_deref()
            .and_then(|p| TextSettings::load(p).ok())
            .unwrap_or_default();
        let remembered = settings
            .get_text(launch::FOLDER_SECTION, launch::FOLDER_NAME)
            .map(str::to_string);
        let dir = launch::pack_dir(args, remembered.as_deref());
        let mut app = App {
            dir_edit: dir.display().to_string(),
            dir,
            settings_path,
            settings,
            mods: Modification::default(),
            icons: HashMap::new(),
            selected: None,
            filter: String::new(),
            tab: Tab::Installed,
            status: String::new(),
            list_name: String::new(),
            lists: launch::lists_dir().map_or_else(Vec::new, |d| lists::names(&d)),
            nexus: NexusTab::new(),
            capture: std::env::var_os("BANISHED_MODS_SCREENSHOT")
                .map(|p| (PathBuf::from(p), std::time::Instant::now(), false)),
        };
        app.rescan();
        if std::env::var("BANISHED_MODS_TAB").as_deref() == Ok("nexus") {
            app.tab = Tab::Nexus;
        }
        app.selected = std::env::var("BANISHED_MODS_SELECT")
            .ok()
            .and_then(|v| v.parse().ok());
        app
    }

    /// Read the packages and the settings again (what the game would see now).
    fn rescan(&mut self) {
        if let Some(p) = &self.settings_path {
            if let Ok(s) = TextSettings::load(p) {
                self.settings = s;
            }
        }
        self.mods = Modification::refresh(banished_mods::find(&self.dir), &self.settings);
        self.icons.clear();
        self.selected = None;
        self.status = format!("{} mods in {}", self.mods.mods.len(), self.dir.display());
    }

    /// Write the list to the settings file, as the game's Mods dialog does on Ok (`SaveList`).
    /// The file is read again first, so values the game wrote since are kept.
    fn save(&mut self) -> bool {
        let Some(path) = self.settings_path.clone() else {
            self.status = "No settings file: set BANISHED_SETTINGS".into();
            return false;
        };
        let mut settings = TextSettings::load(&path).unwrap_or_default();
        self.mods.save_list(&mut settings);
        settings.set_text(
            launch::FOLDER_SECTION,
            launch::FOLDER_NAME,
            &self.dir.display().to_string(),
        );
        match settings.save(&path) {
            Ok(()) => {
                self.settings = settings;
                self.mods.dirty = false;
                self.status = format!("Saved to {}", path.display());
                true
            }
            Err(e) => {
                self.status = format!("Could not save {}: {e}", path.display());
                false
            }
        }
    }

    fn icon(&mut self, ctx: &egui::Context, i: usize) -> Option<egui::TextureHandle> {
        let m = &self.mods.mods[i];
        let key = format!("{}:{:08x}", m.file, m.crc);
        if let Some(t) = self.icons.get(&key) {
            return Some(t.clone());
        }
        let (w, h, argb) = &m.icon;
        if *w == 0 || *h == 0 || argb.len() < (*w * *h * 4) as usize {
            return None;
        }
        let rgba: Vec<u8> = argb
            .chunks(4)
            .flat_map(|p| [p[2], p[1], p[0], p[3]])
            .collect();
        let image = egui::ColorImage::from_rgba_unmultiplied([*w as usize, *h as usize], &rgba);
        let t = ctx.load_texture(&key, image, egui::TextureOptions::LINEAR);
        self.icons.insert(key, t.clone());
        Some(t)
    }

    /// Move a mod from one place in the list to another, one swap at a time.
    fn move_mod(&mut self, from: usize, to: usize) {
        if from < to {
            for j in from..to {
                self.mods.swap(j, j + 1);
            }
        } else {
            for j in (to..from).rev() {
                self.mods.swap(j, j + 1);
            }
        }
        self.selected = Some(to);
    }

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("Game data (WinData):");
            let edit = ui.add(
                egui::TextEdit::singleline(&mut self.dir_edit)
                    .desired_width(ui.available_width() - 90.0),
            );
            let enter = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if ui.button("Read again").clicked() || enter {
                self.dir = PathBuf::from(self.dir_edit.trim());
                self.rescan();
            }
        });
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.tab, Tab::Installed, "Installed mods");
            ui.selectable_value(&mut self.tab, Tab::Nexus, "Nexus Mods");
        });
    }

    fn bottom_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let unsaved = self.mods.dirty;
            if ui
                .add_enabled(unsaved, egui::Button::new("Save"))
                .on_hover_text(
                    "Write the list to the settings file; the game uses it when it starts",
                )
                .clicked()
            {
                self.save();
            }
            if ui
                .add_enabled(unsaved, egui::Button::new("Revert"))
                .on_hover_text("Read the settings file again")
                .clicked()
            {
                self.rescan();
            }
            let play = ui
                .button(RichText::new("▶ Play").strong())
                .on_hover_text("Save, then start Un-Banished with this game data");
            if play.clicked() && (!self.mods.dirty || self.save()) {
                match launch::start_game(&self.dir) {
                    Ok(()) => self.status = "Un-Banished is starting".into(),
                    Err(e) => {
                        self.status = format!(
                            "Could not start {}: {e}",
                            launch::game_executable().display()
                        )
                    }
                }
            }
            ui.separator();
            let enabled = self.mods.mods.iter().filter(|m| m.enabled).count();
            ui.label(format!("{enabled} of {} on", self.mods.mods.len()));
            if unsaved {
                ui.colored_label(Color32::from_rgb(230, 170, 60), "Not saved");
            }
            ui.separator();
            ui.label(&self.status);
        });
    }

    fn list_tools(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.filter)
                    .hint_text("Filter by name or author")
                    .desired_width(200.0),
            );
            if ui.button("All on").clicked() {
                for i in 0..self.mods.mods.len() {
                    self.mods.enable(i, true);
                }
            }
            if ui.button("All off").clicked() {
                for i in 0..self.mods.mods.len() {
                    self.mods.enable(i, false);
                }
            }
            ui.separator();
            ui.label("Mod list:");
            egui::ComboBox::from_id_salt("lists")
                .selected_text("Load…")
                .show_ui(ui, |ui| {
                    for name in self.lists.clone() {
                        if ui.selectable_label(false, &name).clicked() {
                            self.load_list(&name);
                        }
                    }
                    if self.lists.is_empty() {
                        ui.label("No saved lists");
                    }
                });
            ui.add(
                egui::TextEdit::singleline(&mut self.list_name)
                    .hint_text("Name")
                    .desired_width(120.0),
            );
            if ui
                .add_enabled(
                    !self.list_name.trim().is_empty(),
                    egui::Button::new("Save list"),
                )
                .clicked()
            {
                self.save_list();
            }
        });
    }

    fn load_list(&mut self, name: &str) {
        let Some(dir) = launch::lists_dir() else {
            return;
        };
        match std::fs::read_to_string(dir.join(lists::file_name(name))) {
            Ok(text) => {
                let missing = lists::apply(&mut self.mods, &text);
                self.list_name = name.to_string();
                self.status = if missing.is_empty() {
                    format!("Loaded the list {name}; press Save to keep it")
                } else {
                    format!("Loaded {name}; not installed: {}", missing.join(", "))
                };
            }
            Err(e) => self.status = format!("Could not read the list {name}: {e}"),
        }
    }

    fn save_list(&mut self) {
        let Some(dir) = launch::lists_dir() else {
            return;
        };
        let name = self.list_name.trim().to_string();
        let path = dir.join(lists::file_name(&name));
        let result = std::fs::create_dir_all(&dir)
            .and_then(|()| std::fs::write(&path, lists::to_text(&self.mods)));
        self.status = match result {
            Ok(()) => format!("Saved the list {name}"),
            Err(e) => format!("Could not save {}: {e}", path.display()),
        };
        self.lists = lists::names(&dir);
    }

    fn installed_tab(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
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

    fn details(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
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

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if let Some((path, started, asked)) = &mut self.capture {
            ctx.request_repaint();
            if !*asked && started.elapsed().as_secs_f32() > 4.0 {
                *asked = true;
                let path = path.clone();
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(
                    egui::ScreenshotCallback::new(move |image| {
                        if let Err(e) = write_png(&path, &image) {
                            eprintln!("{}: {e}", path.display());
                        }
                        std::process::exit(0);
                    }),
                ));
            }
        }
        egui::Panel::top("top").show(ui, |ui| {
            ui.add_space(4.0);
            self.top_bar(ui);
            ui.add_space(2.0);
        });
        egui::Panel::bottom("bottom").show(ui, |ui| {
            ui.add_space(4.0);
            self.bottom_bar(ui);
            ui.add_space(4.0);
        });
        match self.tab {
            Tab::Installed => {
                egui::Panel::right("details")
                    .default_size(300.0)
                    .min_size(220.0)
                    .show(ui, |ui| self.details(&ctx, ui));
                egui::CentralPanel::default().show(ui, |ui| self.installed_tab(&ctx, ui));
            }
            Tab::Nexus => {
                let installed = egui::CentralPanel::default()
                    .show(ui, |ui| {
                        self.nexus
                            .ui(&ctx, ui, &self.dir, self.settings_path.as_deref())
                    })
                    .inner;
                if installed {
                    let dirty = self.mods.dirty;
                    let keep = lists::to_text(&self.mods);
                    self.rescan();
                    if dirty {
                        lists::apply(&mut self.mods, &keep);
                    }
                    self.status = "Installed. Turn the new mod on in Installed mods.".into();
                }
            }
        }
    }
}
