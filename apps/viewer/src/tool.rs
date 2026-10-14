//! The asset browser, built with the reimplemented game UI (`banished_ui`) and the game's own
//! widget templates from `Dialog\SharedElements`.

use banished_formats::texture::decode_rgba;
use banished_formats::types::AnimationClip;
use banished_formats::ui::flags;
use banished_ui::{DialogInstance, Event, Key, Kind, Node, NodeId, Ui};
use glam::{Mat4, Vec3};
use web_time::Instant;

use crate::assets::Assets;
use crate::audio::Player;
use crate::audio_player::AudioPlayerUi;
use crate::render::{Frame, GpuMesh, RenderMode, Renderer, CLEAR_DARK, CLEAR_SKY};

const FONT: &str = r"Font\FontMedium.rsc:resource";
const FONT_SMALL: &str = r"Font\FontSmall.rsc:resource";
const BUTTON: &str = r"Dialog\SharedElements.rsc:buttonOk";
const PANEL: &str = r"Dialog\SharedElements.rsc:sunkenBorder";
const BAR: &str = r"Dialog\SharedElements.rsc:dialogBorder";
const TOP_BAR_HEIGHT: i32 = 48;
const STATUS_HEIGHT: i32 = 30;
const SIDEBAR_WIDTH: i32 = 380;
const PREVIEW_TEXTURE: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Models,
    Textures,
    Audio,
    Dialogs,
}

impl Mode {
    pub const ALL: [Mode; 4] = [Mode::Models, Mode::Textures, Mode::Audio, Mode::Dialogs];

    fn label(self) -> &'static str {
        match self {
            Mode::Models => "Models",
            Mode::Textures => "Textures",
            Mode::Audio => "Audio",
            Mode::Dialogs => "Dialogs",
        }
    }

    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    pub fn parse(s: &str) -> Option<Mode> {
        Mode::ALL
            .into_iter()
            .find(|m| m.label().eq_ignore_ascii_case(s))
    }
}

pub struct Camera {
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
    pub target: Vec3,
}

impl Camera {
    pub fn eye(&self) -> Vec3 {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        self.target + Vec3::new(cy * cp, sy * cp, sp) * self.distance
    }

    pub fn view_proj(&self, aspect: f32) -> Mat4 {
        use glam::camera::rh::{proj::directx::perspective, view::look_at_mat4};
        let proj = perspective(
            45f32.to_radians(),
            aspect,
            self.distance * 0.01,
            self.distance * 20.0,
        );
        proj * look_at_mat4(self.eye(), self.target, Vec3::Z)
    }
}

pub struct Tool {
    pub assets: Assets,
    pub ui: Ui,
    root: NodeId,
    mode_buttons: Vec<(Mode, NodeId)>,
    list: NodeId,
    categories: NodeId,
    subcategories: NodeId,
    categories_label: NodeId,
    subcategories_label: NodeId,
    resources_label: NodeId,
    sidebar_stack: NodeId,
    selected_category: usize,
    selected_subcategory: usize,
    main: NodeId,
    info: NodeId,
    picture: NodeId,
    audio_controls: NodeId,
    audio_player: AudioPlayerUi,
    audio_window_position: Option<[i32; 2]>,
    audio_window_dragging: bool,
    audio_drag_offset: [i32; 2],
    audio_duration: f32,
    audio_seek_position: f32,
    audio_looping: bool,
    audio_volume: f32,
    model_controls: NodeId,
    render_buttons: Vec<(RenderMode, NodeId)>,
    render_mode: RenderMode,
    visible_items: Vec<String>,
    play: NodeId,
    stop: NodeId,
    animation_next: NodeId,
    animation_back: NodeId,
    animation_forward: NodeId,
    animation_label: NodeId,
    animation_clips: Vec<AnimationClip>,
    animation_index: usize,
    animation_playing: bool,
    animation_started: Instant,
    animation_elapsed: f32,
    dialog_host: NodeId,
    dialog_background_host: NodeId,
    status: NodeId,
    dialog: Option<DialogInstance>,
    dialog_animation_started: Instant,
    dialog_animation_elapsed: f32,
    dialog_animation_playing: bool,
    permanent_nodes: usize,
    pub mode: Mode,
    pub mesh: Option<GpuMesh>,
    pub camera: Camera,
    player: Player,
    dragging: bool,
    last_cursor: Option<(i32, i32)>,
}

fn rect_contains(r: [i32; 4], x: i32, y: i32) -> bool {
    x >= r[0] && x < r[2] && y >= r[1] && y < r[3]
}

impl Tool {
    pub fn new(assets: Assets, player: Player) -> Tool {
        let mut ui = Ui::new(assets.archive.clone());
        let root = ui.group();

        // Top bar: mode buttons cloned from the game's standard OK button.
        let top = ui.viewport();
        {
            let n = ui.node_mut(top);
            n.flags = flags::FILL_WIDTH;
            n.min_size = [0, TOP_BAR_HEIGHT];
        }
        if let Some(bar) = ui.instantiate(BAR) {
            ui.node_mut(bar).color = 0xFFE6D4AE;
            ui.add_child(top, bar);
        }
        let buttons = ui.ribbon(false, 6);
        {
            let n = ui.node_mut(buttons);
            n.margins = [12, 12, 0, 0];
            n.alignment = 3; // middle-left
        }
        ui.add_child(top, buttons);
        let mut mode_buttons = Vec::new();
        for mode in Mode::ALL {
            if let Some(b) = ui.instantiate(BUTTON) {
                ui.set_text(b, mode.label());
                ui.node_mut(b).name = mode.label().to_string();
                ui.node_mut(b).color = 0xFFF2E2C1;
                ui.set_shortcut(b, b'1' as u32 + mode_buttons.len() as u32);
                ui.add_child(buttons, b);
                mode_buttons.push((mode, b));
            }
        }
        let title = ui.text(FONT, "Banished asset browser");
        {
            let n = ui.node_mut(title);
            n.alignment = 5; // middle-right
            n.margins = [0, 16, 0, 0];
            n.color = 0xFFFFE1A1;
        }
        ui.add_child(top, title);
        ui.add_child(root, top);

        // Sidebar list in a sunken panel.
        let side = ui.viewport();
        {
            let n = ui.node_mut(side);
            n.flags = flags::FILL_HEIGHT;
            n.min_size = [SIDEBAR_WIDTH, 0];
            n.margins = [8, 0, TOP_BAR_HEIGHT + 8, STATUS_HEIGHT + 8];
        }
        if let Some(panel) = ui.instantiate(PANEL) {
            ui.node_mut(panel).color = 0xFFE9D9BB;
            ui.add_child(side, panel);
        }
        let sidebar_stack = ui.ribbon(true, 10);
        {
            let n = ui.node_mut(sidebar_stack);
            n.flags = flags::FILL_WIDTH | flags::FILL_HEIGHT;
            n.margins = [8, 8, 8, 8];
        }
        let categories = ui.list(FONT_SMALL);
        {
            let n = ui.node_mut(categories);
            n.flags = flags::FILL_WIDTH;
            n.min_size = [0, 76];
        }
        let categories_label = ui.text(FONT_SMALL, "Type");
        ui.node_mut(categories_label).color = 0xFFFFD77C;
        ui.node_mut(categories_label).margins = [3, 0, 0, 0];
        let subcategories = ui.list(FONT_SMALL);
        {
            let n = ui.node_mut(subcategories);
            n.flags = flags::FILL_WIDTH;
            n.min_size = [0, 76];
        }
        let subcategories_label = ui.text(FONT_SMALL, "Category");
        ui.node_mut(subcategories_label).color = 0xFFFFD77C;
        ui.node_mut(subcategories_label).margins = [3, 0, 0, 0];
        let resources_label = ui.text(FONT_SMALL, "Resources");
        ui.node_mut(resources_label).color = 0xFFFFD77C;
        ui.node_mut(resources_label).margins = [3, 0, 0, 0];
        ui.node_mut(categories).color = 0xFFFFE7C1;
        ui.node_mut(subcategories).color = 0xFFFFE7C1;
        let list = ui.list(FONT_SMALL);
        {
            let n = ui.node_mut(list);
            n.flags = flags::FILL_WIDTH | flags::FILL_HEIGHT;
            n.color = 0xFFFFE7C1;
        }
        let model_controls = ui.ribbon(false, 4);
        {
            let n = ui.node_mut(model_controls);
            n.flags = flags::FILL_WIDTH;
            n.min_size = [0, 36];
        }
        ui.node_mut(model_controls).color = 0xFFF2E2C1;
        let render_buttons = [
            (RenderMode::Textured, "Textured"),
            (RenderMode::Vertices, "Vertices"),
            (RenderMode::Wireframe, "Wireframe"),
        ]
        .into_iter()
        .map(|(mode, label)| {
            let button = ui.instantiate(BUTTON).unwrap_or_else(|| ui.group());
            ui.set_text(button, label);
            ui.node_mut(button).color = 0xFFF2E2C1;
            ui.add_child(model_controls, button);
            (mode, button)
        })
        .collect::<Vec<_>>();
        ui.add_child(sidebar_stack, categories_label);
        ui.add_child(sidebar_stack, categories);
        ui.add_child(sidebar_stack, subcategories_label);
        ui.add_child(sidebar_stack, subcategories);
        ui.add_child(sidebar_stack, model_controls);
        ui.add_child(sidebar_stack, resources_label);
        ui.add_child(sidebar_stack, list);
        ui.add_child(side, sidebar_stack);
        ui.add_child(root, side);

        // Main area.
        let main = ui.viewport();
        {
            let n = ui.node_mut(main);
            n.flags = flags::FILL_WIDTH | flags::FILL_HEIGHT;
            n.margins = [SIDEBAR_WIDTH + 16, 8, TOP_BAR_HEIGHT + 8, STATUS_HEIGHT + 8];
        }
        let picture = ui.picture();
        let picture_border = ui.instantiate(PANEL);
        {
            let n = ui.node_mut(picture);
            n.flags = flags::FILL_WIDTH | flags::FILL_HEIGHT;
            n.margins = [12, 12, 48, 12];
            n.border = picture_border;
        }
        ui.add_child(main, picture);
        let dialog_background_host = ui.group();
        ui.add_child(main, dialog_background_host);
        let dialog_host = ui.group();
        ui.add_child(main, dialog_host);
        let info = ui.text(FONT, "");
        {
            let n = ui.node_mut(info);
            n.margins = [16, 16, 12, 0];
            n.color = 0xFFFFE7C1;
            if let Kind::Text(t) = &mut n.kind {
                t.wrap = true;
                t.max_width = 900;
            }
        }
        let info_border = ui.instantiate(PANEL);
        ui.node_mut(info).border = info_border;
        ui.add_child(main, info);
        let audio_controls = ui.ribbon(false, 8);
        {
            let n = ui.node_mut(audio_controls);
            n.margins = [12, 0, 120, 0];
        }
        let play = ui.instantiate(BUTTON).unwrap_or_else(|| ui.group());
        ui.set_text(play, "Play");
        let stop = ui.instantiate(BUTTON).unwrap_or_else(|| ui.group());
        ui.set_text(stop, "Stop");
        let animation_next = ui.instantiate(BUTTON).unwrap_or_else(|| ui.group());
        ui.set_text(animation_next, "Next clip");
        let animation_back = ui.instantiate(BUTTON).unwrap_or_else(|| ui.group());
        ui.set_text(animation_back, "-0.1s");
        let animation_forward = ui.instantiate(BUTTON).unwrap_or_else(|| ui.group());
        ui.set_text(animation_forward, "+0.1s");
        ui.add_child(audio_controls, play);
        ui.add_child(audio_controls, stop);
        ui.add_child(audio_controls, animation_next);
        ui.add_child(audio_controls, animation_back);
        ui.add_child(audio_controls, animation_forward);
        let animation_label = ui.text(FONT_SMALL, "");
        ui.node_mut(animation_label).margins = [8, 0, 0, 0];
        ui.add_child(audio_controls, animation_label);
        ui.add_child(main, audio_controls);
        let audio_player = AudioPlayerUi::new(&mut ui);
        ui.add_child(main, audio_player.window);
        ui.add_child(root, main);

        // Status line.
        let status = ui.text(FONT_SMALL, "");
        {
            let n = ui.node_mut(status);
            n.alignment = 6; // bottom-left
            n.margins = [14, 0, 0, 8];
            n.color = 0xFFE9D6AA;
        }
        let status_panel = ui.viewport();
        {
            let n = ui.node_mut(status_panel);
            n.flags = flags::FILL_WIDTH;
            n.min_size = [0, STATUS_HEIGHT];
            n.alignment = 6;
        }
        if let Some(background) = ui.instantiate(BAR) {
            ui.node_mut(background).flags = flags::FILL_WIDTH | flags::FILL_HEIGHT;
            ui.node_mut(background).color = 0xFFE6D4AE;
            ui.add_child(status_panel, background);
        }
        ui.add_child(root, status_panel);
        ui.add_child(root, status);

        let permanent_nodes = ui.nodes.len();
        let mut tool = Tool {
            assets,
            ui,
            root,
            mode_buttons,
            list,
            categories,
            subcategories,
            categories_label,
            subcategories_label,
            resources_label,
            sidebar_stack,
            selected_category: 0,
            selected_subcategory: 0,
            main,
            info,
            picture,
            audio_controls,
            audio_player,
            audio_window_position: None,
            audio_window_dragging: false,
            audio_drag_offset: [0, 0],
            audio_duration: 0.0,
            audio_seek_position: 0.0,
            audio_looping: false,
            audio_volume: 1.0,
            model_controls,
            render_buttons,
            render_mode: RenderMode::Textured,
            visible_items: Vec::new(),
            play,
            stop,
            animation_next,
            animation_back,
            animation_forward,
            animation_label,
            animation_clips: Vec::new(),
            animation_index: 0,
            animation_playing: false,
            animation_started: Instant::now(),
            animation_elapsed: 0.0,
            dialog_host,
            dialog_background_host,
            status,
            dialog: None,
            dialog_animation_started: Instant::now(),
            dialog_animation_elapsed: 0.0,
            dialog_animation_playing: false,
            permanent_nodes,
            mode: Mode::Models,
            mesh: None,
            camera: Camera {
                yaw: -0.8,
                pitch: 0.45,
                distance: 3.0,
                target: Vec3::ZERO,
            },
            player,
            dragging: false,
            last_cursor: None,
        };
        let counts = format!(
            "{} models   {} textures   {} sounds   {} dialogs",
            tool.assets.meshes.len(),
            tool.assets.textures.len(),
            tool.assets.sounds.len(),
            tool.assets.dialogs.len()
        );
        tool.set_status(&counts);
        tool.set_render_mode(RenderMode::Textured);
        tool
    }

    fn items(&self, mode: Mode) -> &[String] {
        match mode {
            Mode::Models => &self.assets.meshes,
            Mode::Textures => &self.assets.textures,
            Mode::Audio => &self.assets.sounds,
            Mode::Dialogs => &self.assets.dialogs,
        }
    }

    fn resource_dirs(&self, item: &str) -> Vec<String> {
        let path = item.replace('\\', "/");
        if self.mode == Mode::Dialogs {
            let stem = path.trim_end_matches(".crs");
            let stem = stem.strip_prefix("Dialog/").unwrap_or(stem);
            let (group, object) = stem.rsplit_once('_').unwrap_or((stem, "resource"));
            return vec![group.to_owned(), object.to_owned()];
        }
        let mut parts: Vec<_> = path.split('/').collect();
        parts.pop(); // resource filename
        let Some(first) = parts.first().copied() else {
            return Vec::new();
        };
        let root: &[&str] = match self.mode {
            Mode::Models => &[
                "Models",
                "Model",
                "Meshes",
                "Mesh",
                "Graphics",
                "GraphicsMesh",
                "",
            ],
            Mode::Textures => &["Textures", "Texture", "Images", "ImageBuffer", ""],
            Mode::Audio => &["Audio", "Sounds", "Sound", "SoundBuffer", ""],
            Mode::Dialogs => &["Dialogs", "Dialog", "UI", ""],
        };
        let skip = root
            .iter()
            .any(|name| !name.is_empty() && first.eq_ignore_ascii_case(name));
        parts
            .into_iter()
            .skip(usize::from(skip))
            .map(str::to_owned)
            .collect()
    }

    fn category_names(&self) -> Vec<String> {
        let categories: std::collections::BTreeSet<_> = self
            .items(self.mode)
            .iter()
            .filter_map(|item| self.resource_dirs(item).first().cloned())
            .collect();
        std::iter::once("All".to_owned())
            .chain(categories)
            .collect()
    }

    fn subcategory_names(&self, category: usize) -> Vec<String> {
        let selected = self.category_names().get(category).cloned();
        let subcategories: std::collections::BTreeSet<_> = self
            .items(self.mode)
            .iter()
            .filter_map(|item| {
                let dirs = self.resource_dirs(item);
                if category > 0 && dirs.first() != selected.as_ref() {
                    return None;
                }
                dirs.get(1).cloned()
            })
            .collect();
        std::iter::once("All".to_owned())
            .chain(subcategories)
            .collect()
    }

    fn filtered_items(&self, category: usize, subcategory: usize) -> Vec<String> {
        let category_name = self.category_names().get(category).cloned();
        let subcategory_name = self.subcategory_names(category).get(subcategory).cloned();
        if self.mode == Mode::Dialogs && category == 0 && subcategory == 0 {
            // The default dialog list represents each named dialog once. The
            // group and object selectors expose its individual resources.
            let mut groups = std::collections::BTreeMap::<String, Vec<String>>::new();
            for item in self.items(self.mode) {
                if let Some(group) = self.resource_dirs(item).first() {
                    groups.entry(group.clone()).or_default().push(item.clone());
                }
            }
            return groups
                .into_iter()
                .filter_map(|(group, mut items)| {
                    items.sort();
                    let preferred = match group.as_str() {
                        "Loading" => "_gameLoad0.",
                        // The generic SaveLoad resource combines the Save title
                        // with a Load action. Use its explicit Load dialog as
                        // the group's preview; the selector still lists every
                        // SaveLoad resource, including the generic resource.
                        "SaveLoad" => "_confirmLoad.",
                        _ => "_resource.",
                    };
                    let selected = items
                        .iter()
                        .position(|item| item.contains(preferred))
                        .unwrap_or(0);
                    items.get(selected).cloned()
                })
                .collect();
        }
        self.items(self.mode)
            .iter()
            .filter(|item| {
                let dirs = self.resource_dirs(item);
                (category == 0 || dirs.first() == category_name.as_ref())
                    && (subcategory == 0 || dirs.get(1) == subcategory_name.as_ref())
            })
            .cloned()
            .collect()
    }

    fn display_name(entry: &str) -> String {
        entry.trim_end_matches(".crs").replace('\\', "/")
    }

    fn display_resource_name(&self, entry: &str) -> String {
        let path = entry.replace('\\', "/");
        if self.mode == Mode::Dialogs {
            let stem = path.trim_end_matches(".crs");
            let stem = stem.strip_prefix("Dialog/").unwrap_or(stem);
            let (group, object) = stem.rsplit_once('_').unwrap_or((stem, "resource"));
            return if self.selected_category == 0 {
                group.to_owned()
            } else if self.selected_subcategory > 0 {
                format!("{group}_{object}")
            } else {
                object.to_owned()
            };
        }
        let mut parts: Vec<_> = path.split('/').collect();
        let dirs = self.resource_dirs(entry);
        let root_parts = parts.len().saturating_sub(dirs.len() + 1);
        let category_parts = usize::from(self.selected_category > 0).min(dirs.len());
        let subcategory_parts = usize::from(self.selected_subcategory > 0)
            .min(dirs.len().saturating_sub(category_parts));
        parts.drain(..root_parts + category_parts + subcategory_parts);
        parts.join("/").trim_end_matches(".crs").to_owned()
    }

    pub fn set_status(&mut self, text: &str) {
        log::info!("{text}");
        self.ui.set_text(self.status, text);
    }

    fn set_info(&mut self, text: &str) {
        self.ui.set_text(self.info, text);
    }

    fn set_render_mode(&mut self, mode: RenderMode) {
        self.render_mode = mode;
        for &(button_mode, button) in &self.render_buttons {
            let node = self.ui.node_mut(button);
            node.flags = if button_mode == mode {
                node.flags & !flags::DIM
            } else {
                node.flags | flags::DIM
            };
        }
    }

    pub fn set_mode(&mut self, renderer: &mut Renderer, mode: Mode) {
        self.mode = mode;
        self.ui.set_text(
            self.categories_label,
            if mode == Mode::Dialogs {
                "Dialog group"
            } else {
                "Type"
            },
        );
        {
            let category_list = self.ui.node_mut(self.categories);
            category_list.min_size = [0, if mode == Mode::Dialogs { 128 } else { 76 }];
            category_list.flags = flags::FILL_WIDTH;
        }
        {
            let state_list = self.ui.node_mut(self.subcategories);
            state_list.min_size = [0, if mode == Mode::Dialogs { 0 } else { 76 }];
            state_list.flags = flags::FILL_WIDTH
                | if mode == Mode::Dialogs {
                    flags::FILL_HEIGHT
                } else {
                    0
                };
        }
        self.ui.set_text(
            self.subcategories_label,
            if mode == Mode::Dialogs {
                "State / resource"
            } else {
                "Category"
            },
        );
        self.player.stop();
        self.animation_playing = false;
        self.dialog_animation_playing = false;
        self.dialog_animation_elapsed = 0.0;
        self.audio_player.set_playing(&mut self.ui, false);
        self.animation_clips.clear();
        self.animation_index = 0;
        let categories = self.category_names();
        let initial_category = usize::from(mode == Mode::Dialogs && categories.len() > 1);
        self.ui.set_list_items(self.categories, categories);
        self.ui.list_select(self.categories, Some(initial_category));
        self.selected_category = initial_category;
        self.selected_subcategory = 0;
        self.visible_items = self.filtered_items(initial_category, 0);
        if mode == Mode::Dialogs {
            self.ui.set_list_items(
                self.subcategories,
                self.visible_items
                    .iter()
                    .map(|name| self.display_resource_name(name))
                    .collect(),
            );
            self.ui.list_select(self.subcategories, Some(0));
        } else {
            self.ui
                .set_list_items(self.subcategories, self.subcategory_names(0));
            self.ui.list_select(self.subcategories, Some(0));
            self.ui.set_list_items(
                self.list,
                self.visible_items
                    .iter()
                    .map(|n| self.display_resource_name(n))
                    .collect(),
            );
        }
        for &(m, b) in &self.mode_buttons {
            // Highlight the active mode by dimming the others.
            let n = self.ui.node_mut(b);
            n.flags = if m == mode {
                n.flags & !flags::DIM
            } else {
                n.flags | flags::DIM
            };
        }
        self.ui.set_visible(self.picture, mode == Mode::Textures);
        self.ui.set_visible(self.list, mode != Mode::Dialogs);
        self.ui
            .set_visible(self.audio_player.window, mode == Mode::Audio);
        self.ui
            .set_visible(self.resources_label, mode != Mode::Dialogs);
        self.ui.node_mut(self.sidebar_stack).children = if mode == Mode::Dialogs {
            vec![
                self.categories_label,
                self.categories,
                self.model_controls,
                self.list,
                self.subcategories_label,
                self.subcategories,
            ]
        } else {
            vec![
                self.categories_label,
                self.categories,
                self.subcategories_label,
                self.subcategories,
                self.model_controls,
                self.resources_label,
                self.list,
            ]
        };
        self.ui
            .set_visible(self.model_controls, mode == Mode::Models);
        self.ui.set_visible(self.audio_controls, false);
        self.ui.set_visible(
            self.animation_next,
            mode == Mode::Models && !self.animation_clips.is_empty(),
        );
        self.ui.set_visible(
            self.animation_label,
            mode == Mode::Models && !self.animation_clips.is_empty(),
        );
        self.ui.set_visible(self.animation_back, false);
        self.ui.set_visible(self.animation_forward, false);
        self.ui.set_text(self.play, "Play");
        self.ui.set_visible(self.dialog_host, mode == Mode::Dialogs);
        self.ui.node_mut(self.dialog_host).children.clear();
        self.mesh = None;
        self.set_info("");
        self.select(renderer, 0);
    }

    pub fn select(&mut self, renderer: &mut Renderer, index: usize) {
        let Some(name) = self.visible_items.get(index).cloned() else {
            return;
        };
        self.ui.list_select(
            if self.mode == Mode::Dialogs {
                self.subcategories
            } else {
                self.list
            },
            Some(index),
        );
        self.ui.clear_input();
        if self.mode == Mode::Audio {
            self.player.stop();
            self.audio_seek_position = 0.0;
            self.audio_duration = 0.0;
            self.audio_player.set_track(&mut self.ui, "", 0.0, false);
            self.audio_player.set_playing(&mut self.ui, false);
            self.audio_player.set_position(&mut self.ui, 0.0);
        }
        self.ui
            .node_mut(self.dialog_background_host)
            .children
            .clear();
        self.ui.nodes.truncate(self.permanent_nodes);
        self.ui.node_mut(self.dialog_host).children.clear();
        self.dialog = None;
        self.dialog_animation_started = Instant::now();
        self.dialog_animation_elapsed = 0.0;
        self.dialog_animation_playing = false;
        let shown = Self::display_name(&name);
        match self.mode {
            Mode::Models => match renderer.load_mesh(&self.assets, &name) {
                Ok(mesh) => {
                    self.camera.target = mesh.center;
                    self.camera.distance = mesh.radius * 2.6;
                    let parts = mesh.part_count();
                    if let Ok(definition) = self.assets.mesh(&name) {
                        self.animation_clips = self
                            .assets
                            .animations_for_mesh(&name, &definition.source)
                            .into_iter()
                            .filter(|clip| {
                                clip.bone_count == definition.bones.len() && !clip.frames.is_empty()
                            })
                            .collect();
                    }
                    log::info!(
                        "{}: matched {} animation clips",
                        shown,
                        self.animation_clips.len()
                    );
                    self.animation_elapsed = 0.0;
                    self.animation_playing = false;
                    self.ui.set_visible(
                        self.audio_controls,
                        self.mode == Mode::Audio || !self.animation_clips.is_empty(),
                    );
                    self.ui
                        .set_visible(self.animation_next, !self.animation_clips.is_empty());
                    self.ui
                        .set_visible(self.animation_label, !self.animation_clips.is_empty());
                    self.ui
                        .set_visible(self.animation_back, !self.animation_clips.is_empty());
                    self.ui
                        .set_visible(self.animation_forward, !self.animation_clips.is_empty());
                    self.ui.set_text(self.play, "Play");
                    self.refresh_animation_label();
                    self.mesh = Some(mesh);
                    self.set_info(&format!(
                        "{shown}\n{parts} part(s)   drag to orbit, wheel to zoom\n{} animation clip(s)",
                        self.animation_clips.len()
                    ));
                }
                Err(e) => self.set_info(&format!("{shown}\n{e}")),
            },
            Mode::Textures => match self.assets.image_entry(&name) {
                Ok(img) => {
                    let rgba = img
                        .levels
                        .first()
                        .and_then(|l| decode_rgba(img.format, img.width, img.height, l));
                    let kind = if img.image_type == 0 { "2D" } else { "cube" };
                    self.set_info(&format!(
                        "{shown}\n{}x{} {:?} {kind}   {} mip(s)   source {}",
                        img.width,
                        img.height,
                        img.format,
                        img.mip_count(),
                        img.source
                    ));
                    if let Some(rgba) = rgba {
                        let view = crate::render::upload_rgba(
                            renderer.device(),
                            renderer.queue(),
                            &name,
                            img.width,
                            img.height,
                            &[rgba],
                        );
                        renderer.set_ui_texture(PREVIEW_TEXTURE, &view);
                        self.ui.node_mut(self.picture).kind = Kind::Picture {
                            texture: PREVIEW_TEXTURE,
                            size: [img.width as i32, img.height as i32],
                        };
                    }
                }
                Err(e) => self.set_info(&format!("{shown}\n{e}")),
            },
            Mode::Audio => match self.assets.sound(&name) {
                Ok(s) => {
                    self.audio_duration = s.length_seconds.max(0.0);
                    self.audio_looping = s.looping;
                    self.player.set_looping(self.audio_looping).ok();
                    let track_name = name
                        .rsplit(['\\', '/'])
                        .next()
                        .unwrap_or(&name)
                        .trim_end_matches(".crs")
                        .replace('_', " ");
                    self.audio_player.set_track(
                        &mut self.ui,
                        &track_name,
                        self.audio_duration,
                        self.audio_looping,
                    );
                    self.audio_player
                        .set_volume(&mut self.ui, self.audio_volume);
                    self.set_info(&format!(
                        "{shown}\n{:.1}s   {} ch   {} Hz   {}{}   source {}",
                        s.length_seconds,
                        s.format.channels,
                        s.format.sample_rate,
                        if s.streamed { "streamed" } else { "in memory" },
                        if s.looping { "   loops" } else { "" },
                        s.source
                    ));
                }
                Err(e) => self.set_info(&format!("{shown}\n{e}")),
            },
            Mode::Dialogs => {
                let reference = Self::dialog_reference(&name);
                self.ui.node_mut(self.dialog_host).children.clear();
                match self.ui.instantiate_dialog(&reference) {
                    Some(d) => {
                        if let Some(sprite) = d.background {
                            let background = self.ui.add(Node::new(Kind::Image {
                                sprite: Some(sprite),
                                size: [-1, -1],
                            }));
                            self.ui.node_mut(background).flags =
                                flags::FILL_WIDTH | flags::FILL_HEIGHT;
                            self.ui.add_child(self.dialog_background_host, background);
                        }
                        self.ui.add_child(self.dialog_host, d.root);
                        self.set_info(&reference);
                        self.dialog = Some(d);
                        self.dialog_animation_started = Instant::now();
                        self.dialog_animation_elapsed = 0.0;
                        self.dialog_animation_playing = self.ui.has_animations();
                        let animated = self.ui.has_animations();
                        self.ui.set_visible(self.audio_controls, animated);
                        for node in [
                            self.animation_next,
                            self.animation_back,
                            self.animation_label,
                        ] {
                            self.ui.set_visible(node, animated);
                        }
                        self.ui.set_visible(self.animation_forward, false);
                        self.ui.set_text(
                            self.play,
                            if self.dialog_animation_playing {
                                "Pause"
                            } else {
                                "Play"
                            },
                        );
                        self.ui.set_text(self.stop, "First frame");
                        self.ui.set_text(self.animation_next, "Next frame");
                        self.ui.set_text(self.animation_back, "Prev frame");
                        self.refresh_dialog_animation_label();
                    }
                    None => self.set_info(&format!("{shown}\nUnable to load dialog")),
                }
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn select_matching(&mut self, renderer: &mut Renderer, filter: &str) -> bool {
        let filter = filter.to_lowercase();
        if self.mode == Mode::Dialogs {
            let categories = self.category_names();
            let group = categories
                .iter()
                .skip(1)
                .find(|group| group.to_lowercase() == filter)
                .cloned();
            let target = if let Some(group) = group {
                self.filtered_items(0, 0)
                    .into_iter()
                    .find(|item| self.resource_dirs(item).first() == Some(&group))
            } else {
                self.assets
                    .dialogs
                    .iter()
                    .find(|item| item.to_lowercase().contains(&filter))
                    .cloned()
            };
            let Some(target) = target else {
                return false;
            };
            let Some(group) = self.resource_dirs(&target).first().cloned() else {
                return false;
            };
            let Some(category) = categories.iter().position(|name| *name == group) else {
                return false;
            };
            self.selected_category = category;
            self.selected_subcategory = 0;
            self.ui.list_select(self.categories, Some(category));
            self.visible_items = self.filtered_items(category, 0);
            self.ui.set_list_items(
                self.subcategories,
                self.visible_items
                    .iter()
                    .map(|name| self.display_resource_name(name))
                    .collect(),
            );
            let index = self
                .visible_items
                .iter()
                .position(|item| *item == target)
                .unwrap_or(0);
            self.ui.list_select(self.subcategories, Some(index));
            self.select(renderer, index);
            return true;
        }
        let Some(index) = self
            .visible_items
            .iter()
            .position(|item| item.to_lowercase().contains(&filter))
        else {
            return false;
        };
        self.select(renderer, index);
        true
    }

    pub fn step(&mut self, renderer: &mut Renderer, delta: isize) {
        let n = self.visible_items.len() as isize;
        if n == 0 {
            return;
        }
        let selection_list = if self.mode == Mode::Dialogs {
            self.subcategories
        } else {
            self.list
        };
        let current = match &self.ui.node(selection_list).kind {
            Kind::List(l) => l.selected.unwrap_or(0) as isize,
            _ => 0,
        };
        self.select(renderer, (current + delta).rem_euclid(n) as usize);
    }

    fn step_audio(&mut self, renderer: &mut Renderer, delta: isize) {
        let was_playing = self.player.is_playing();
        self.step(renderer, delta);
        if was_playing {
            self.play_selected();
        }
    }

    fn play_selected(&mut self) {
        let Kind::List(l) = &self.ui.node(self.list).kind else {
            return;
        };
        let Some(name) = l.selected.and_then(|i| self.visible_items.get(i)).cloned() else {
            return;
        };
        let result = self
            .assets
            .sound(&name)
            .map_err(|e| e.to_string())
            .and_then(|sound| {
                let stream = if sound.streamed {
                    Some(
                        self.assets
                            .archive
                            .read(&sound.stream_entry)
                            .map_err(|e| e.to_string())?,
                    )
                } else {
                    None
                };
                let pcm = crate::audio::decode(&sound, stream.as_deref())?;
                self.player.play(pcm, self.audio_looping)?;
                if self.audio_seek_position > 0.0 {
                    self.player
                        .seek(std::time::Duration::from_secs_f32(self.audio_seek_position))?;
                }
                Ok(())
            });
        match result {
            Ok(()) => {
                self.audio_player.set_playing(&mut self.ui, true);
                self.set_status(&format!("Playing {}", Self::display_name(&name)));
            }
            Err(e) => self.set_status(&format!("{}: {e}", Self::display_name(&name))),
        }
    }

    fn animation_duration(&self) -> f32 {
        self.animation_clips
            .get(self.animation_index)
            .map(|clip| clip.frames.len() as f32 / clip.sample_rate.max(0.001))
            .unwrap_or(0.0)
    }

    fn refresh_animation_label(&mut self) {
        if let Some(clip) = self.animation_clips.get(self.animation_index) {
            let label = format!(
                "{}   {:.2}/{:.2}s",
                clip.name,
                self.animation_elapsed,
                self.animation_duration()
            );
            self.ui.set_text(self.animation_label, &label);
        } else {
            self.ui.set_text(self.animation_label, "");
        }
    }

    pub fn animation_is_playing(&self) -> bool {
        self.animation_playing
            || (self.mode == Mode::Audio && self.player.is_playing())
            || (self.mode == Mode::Dialogs
                && self.dialog_animation_playing
                && self.ui.has_animations())
    }

    pub fn update_animation(&mut self, renderer: &Renderer) {
        if let Some(clip) = self.animation_clips.get(self.animation_index).cloned() {
            if self.animation_playing {
                self.animation_elapsed = self.animation_started.elapsed().as_secs_f32();
                let duration = self.animation_duration();
                if !clip.looping && self.animation_elapsed >= duration {
                    self.animation_elapsed = duration;
                    self.animation_playing = false;
                    self.ui.set_text(self.play, "Play");
                }
                self.refresh_animation_label();
            }
            if let Some(mesh) = &self.mesh {
                renderer.set_animation_pose(mesh, &clip, self.animation_elapsed);
            }
        }
        if self.mode == Mode::Dialogs {
            if self.dialog_animation_playing {
                self.dialog_animation_elapsed =
                    self.dialog_animation_started.elapsed().as_secs_f32();
            }
            self.ui.update_animations(self.dialog_animation_elapsed);
            self.refresh_dialog_animation_label();
        }
        if self.mode == Mode::Audio {
            if self.player.is_playing() || self.player.is_paused() {
                self.audio_seek_position = self.player.position().as_secs_f32();
            } else if self.audio_duration > 0.0 {
                self.audio_seek_position = self.audio_seek_position.min(self.audio_duration);
            }
            self.audio_player
                .set_position(&mut self.ui, self.audio_seek_position);
            self.audio_player
                .set_volume(&mut self.ui, self.audio_volume);
            self.audio_player
                .set_playing(&mut self.ui, self.player.is_playing());
        }
    }

    fn refresh_dialog_animation_label(&mut self) {
        if let Some((frame, count, fps)) = self.ui.animation_info() {
            self.ui.set_text(
                self.animation_label,
                &format!("Frame {}/{}  {} fps", frame + 1, count, fps),
            );
        } else {
            self.ui.set_text(self.animation_label, "");
        }
    }

    fn toggle_dialog_animation(&mut self) {
        if !self.ui.has_animations() {
            return;
        }
        if self.dialog_animation_playing {
            self.dialog_animation_elapsed = self.dialog_animation_started.elapsed().as_secs_f32();
            self.dialog_animation_playing = false;
        } else {
            self.dialog_animation_started =
                Instant::now() - std::time::Duration::from_secs_f32(self.dialog_animation_elapsed);
            self.dialog_animation_playing = true;
        }
        self.ui.set_text(
            self.play,
            if self.dialog_animation_playing {
                "Pause"
            } else {
                "Play"
            },
        );
    }

    fn step_dialog_frame(&mut self, delta: isize) {
        let Some((frame, count, fps)) = self.ui.animation_info() else {
            return;
        };
        if count == 0 {
            return;
        }
        let next = (frame as isize + delta).rem_euclid(count as isize) as usize;
        self.dialog_animation_playing = false;
        self.dialog_animation_elapsed = next as f32 / fps.max(1) as f32;
        self.ui.set_animation_frame(next);
        self.ui.set_text(self.play, "Play");
        self.refresh_dialog_animation_label();
    }

    fn dialog_reference(name: &str) -> String {
        let stem = name.trim_end_matches(".crs");
        let (path, object) = stem.rsplit_once('_').unwrap_or((stem, "resource"));
        format!("{path}.rsc:{object}")
    }

    fn toggle_animation(&mut self) {
        if self.animation_clips.is_empty() {
            return;
        }
        self.animation_playing = !self.animation_playing;
        if self.animation_playing {
            self.animation_started =
                Instant::now() - std::time::Duration::from_secs_f32(self.animation_elapsed);
        }
        self.ui.set_text(
            self.play,
            if self.animation_playing {
                "Pause"
            } else {
                "Play"
            },
        );
    }

    fn next_animation(&mut self) {
        if self.animation_clips.is_empty() {
            return;
        }
        self.animation_index = (self.animation_index + 1) % self.animation_clips.len();
        self.animation_elapsed = 0.0;
        self.animation_playing = false;
        self.ui.set_text(self.play, "Play");
        self.refresh_animation_label();
    }

    fn seek_animation(&mut self, seconds: f32) {
        self.animation_elapsed =
            (self.animation_elapsed + seconds).clamp(0.0, self.animation_duration());
        if self.animation_playing {
            self.animation_started =
                Instant::now() - std::time::Duration::from_secs_f32(self.animation_elapsed);
        }
        self.refresh_animation_label();
    }

    // ------------------------------------------------------------ frame

    pub fn layout(&mut self, size: (u32, u32)) {
        self.ui.screen = [size.0 as i32, size.1 as i32];
        // Dialog roots have their own placement pass. Laying them out as ordinary children
        // first can change the constraints of their containers before their first frame.
        let dialogs = std::mem::take(&mut self.ui.node_mut(self.dialog_host).children);
        self.ui
            .layout(self.root, [0, 0, size.0 as i32, size.1 as i32]);
        if self.mode == Mode::Dialogs {
            // Centre previewed dialogs in the main area (DialogInstance::ComputeLayout).
            let r = self.ui.node(self.main).rect;
            for &c in &dialogs {
                self.ui
                    .layout_dialog(c, [r[0] + 12, r[1] + 48, r[2] - 12, r[3] - 12]);
                self.suppress_overlapping_text(c);
            }
        }
        self.ui.node_mut(self.dialog_host).children = dialogs;
        if self.mode == Mode::Audio {
            let area = self.ui.node(self.main).rect;
            self.ui.layout_dialog(self.audio_player.window, area);
            let window = self.ui.node(self.audio_player.window).rect;
            let width = (window[2] - window[0]).max(520);
            let height = (window[3] - window[1]).max(220);
            let mut position = self.audio_window_position.unwrap_or([
                area[0] + ((area[2] - area[0] - width) / 2).max(0),
                area[1] + ((area[3] - area[1] - height) / 2).max(0),
            ]);
            position[0] = position[0].clamp(area[0], (area[2] - width).max(area[0]));
            position[1] = position[1].clamp(area[1], (area[3] - height).max(area[1]));
            self.audio_window_position = Some(position);
            self.ui.layout_dialog(
                self.audio_player.window,
                [
                    position[0],
                    position[1],
                    position[0] + width,
                    position[1] + height,
                ],
            );
        }
    }

    fn suppress_overlapping_text(&mut self, root: NodeId) {
        let mut pending = vec![(root, root)];
        let mut texts = Vec::new();
        while let Some((id, parent)) = pending.pop() {
            let node = self.ui.node(id);
            if !node.visible() {
                continue;
            }
            pending.extend(self.ui.children(id).into_iter().map(|child| (child, id)));
            {
                let rect = match &node.kind {
                    Kind::Text(text) if !text.text.is_empty() => {
                        let [l, t, r, _] = node.rect;
                        let x = match text.justify {
                            1 => l + ((r - l) - text.layout.width) / 2,
                            2 => r - text.layout.width,
                            _ => l,
                        };
                        Some([x, t, x + text.layout.width, t + text.layout.height])
                    }
                    Kind::Label {
                        text,
                        icon,
                        icon_size,
                        text_alignment,
                        orientation,
                        gap,
                        ..
                    } if !text.text.is_empty() => {
                        let [l, t, r, b] = node.rect;
                        let iw = if icon_size[0] > 0 {
                            icon_size[0]
                        } else {
                            icon.map_or(0, |s| s.width)
                        };
                        let ih = if icon_size[1] > 0 {
                            icon_size[1]
                        } else {
                            icon.map_or(0, |s| s.height)
                        };
                        let [al, at, ar, ab] = match orientation {
                            0 => [l, t, r - gap - iw, b],
                            1 => [l + iw + gap, t, r, b],
                            3 => [l, t + ih + gap, r, b],
                            _ => [l, t, r, b - ih - gap],
                        };
                        let w = text.layout.width;
                        let h = text.layout.height;
                        let x = match text_alignment % 3 {
                            1 => al + ((ar - al) - w) / 2,
                            2 => ar - w,
                            _ => al,
                        };
                        let y = match text_alignment / 3 {
                            1 => at + ((ab - at) - h) / 2,
                            2 => ab - h,
                            _ => at,
                        };
                        Some([x, y, x + w, y + h])
                    }
                    _ => None,
                };
                if let Some(rect) = rect {
                    texts.push((id, parent, rect));
                }
            }
        }
        texts.sort_by_key(|(id, _, _)| *id);
        let mut kept: Vec<(NodeId, [i32; 4])> = Vec::new();
        for (id, parent, rect) in texts {
            let area = ((rect[2] - rect[0]).max(0) as i64) * ((rect[3] - rect[1]).max(0) as i64);
            let covered = kept.iter().any(|(kept_parent, other)| {
                if *kept_parent != parent {
                    return false;
                }
                let w = (rect[2].min(other[2]) - rect[0].max(other[0])).max(0) as i64;
                let h = (rect[3].min(other[3]) - rect[1].max(other[1])).max(0) as i64;
                let other_area =
                    ((other[2] - other[0]).max(0) as i64) * ((other[3] - other[1]).max(0) as i64);
                area.min(other_area) > 0 && w * h * 10 >= area.min(other_area) * 9
            });
            if covered {
                self.ui.set_visible(id, false);
            } else {
                kept.push((parent, rect));
            }
        }
    }

    pub fn viewport(&self) -> [f32; 4] {
        let r = self.ui.node(self.main).rect;
        [
            r[0] as f32,
            r[1] as f32,
            (r[2] - r[0]) as f32,
            (r[3] - r[1]) as f32,
        ]
    }

    pub fn frame<'a>(&'a self, quads: &'a [banished_ui::Quad]) -> Frame<'a> {
        let vp = self.viewport();
        Frame {
            clear: if self.mode == Mode::Models {
                CLEAR_SKY
            } else {
                CLEAR_DARK
            },
            mesh: if self.mode == Mode::Models {
                self.mesh.as_ref()
            } else {
                None
            },
            view_proj: self.camera.view_proj(vp[2] / vp[3].max(1.0)),
            eye: self.camera.eye(),
            viewport: Some(vp),
            ui: Some((&self.ui, quads)),
            render_mode: self.render_mode,
        }
    }

    pub fn quads(&self) -> Vec<banished_ui::Quad> {
        self.ui.draw(self.root)
    }

    // ------------------------------------------------------------ input

    fn handle(&mut self, renderer: &mut Renderer, event: Event) {
        match event {
            Event::Clicked(id) => {
                if let Some(&(mode, _)) = self.mode_buttons.iter().find(|(_, b)| *b == id) {
                    self.set_mode(renderer, mode);
                } else if id == self.audio_player.play && self.mode == Mode::Audio {
                    if self.player.is_playing() {
                        self.audio_seek_position = self.player.position().as_secs_f32();
                        self.player.pause();
                        self.audio_player.set_playing(&mut self.ui, false);
                        self.set_status("Paused");
                    } else if self.player.is_paused() {
                        self.player.resume();
                        self.audio_player.set_playing(&mut self.ui, true);
                        self.set_status("Resumed");
                    } else {
                        self.play_selected();
                    }
                } else if id == self.audio_player.stop && self.mode == Mode::Audio {
                    self.player.stop();
                    self.audio_seek_position = 0.0;
                    self.audio_player.set_position(&mut self.ui, 0.0);
                    self.audio_player.set_playing(&mut self.ui, false);
                    self.set_status("Stopped");
                } else if id == self.audio_player.previous && self.mode == Mode::Audio {
                    self.step_audio(renderer, -1);
                } else if id == self.audio_player.next && self.mode == Mode::Audio {
                    self.step_audio(renderer, 1);
                } else if id == self.audio_player.loop_toggle && self.mode == Mode::Audio {
                    self.audio_looping = !self.audio_looping;
                    match self.player.set_looping(self.audio_looping) {
                        Ok(()) => {
                            self.audio_player
                                .set_looping(&mut self.ui, self.audio_looping);
                        }
                        Err(error) => {
                            self.set_status(&format!("Could not change looping: {error}"))
                        }
                    }
                } else if id == self.play {
                    if self.mode == Mode::Audio {
                        self.play_selected();
                    } else if self.mode == Mode::Dialogs {
                        self.toggle_dialog_animation();
                    } else {
                        self.toggle_animation();
                    }
                } else if id == self.stop {
                    if self.mode == Mode::Audio {
                        self.player.stop();
                        self.set_status("Stopped");
                    } else if self.mode == Mode::Dialogs {
                        self.dialog_animation_playing = false;
                        self.dialog_animation_elapsed = 0.0;
                        self.dialog_animation_started = Instant::now();
                        self.ui.set_animation_frame(0);
                        self.ui.set_text(self.play, "Play");
                        self.refresh_dialog_animation_label();
                    } else {
                        self.animation_playing = false;
                        self.animation_elapsed = 0.0;
                        self.ui.set_text(self.play, "Play");
                        self.refresh_animation_label();
                    }
                } else if id == self.animation_next {
                    if self.mode == Mode::Dialogs {
                        self.step_dialog_frame(1);
                    } else {
                        self.next_animation();
                    }
                } else if id == self.animation_back {
                    if self.mode == Mode::Dialogs {
                        self.step_dialog_frame(-1);
                    } else {
                        self.seek_animation(-0.1);
                    }
                } else if id == self.animation_forward {
                    if self.mode == Mode::Dialogs {
                        self.step_dialog_frame(1);
                    } else {
                        self.seek_animation(0.1);
                    }
                } else if let Some((mode, _)) =
                    self.render_buttons.iter().find(|(_, button)| *button == id)
                {
                    self.set_render_mode(*mode);
                } else {
                    self.dialog_event(event);
                }
            }
            Event::Changed(id) if id == self.audio_player.seek && self.mode == Mode::Audio => {
                if let Some(position) = self.ui.value(id) {
                    self.audio_seek_position = position.clamp(0.0, self.audio_duration);
                    if let Err(error) = self
                        .player
                        .seek(std::time::Duration::from_secs_f32(self.audio_seek_position))
                    {
                        self.set_status(&format!("Could not seek audio: {error}"));
                    }
                    self.audio_player
                        .set_position(&mut self.ui, self.audio_seek_position);
                }
            }
            Event::Changed(id) if id == self.audio_player.volume && self.mode == Mode::Audio => {
                if let Some(volume) = self.ui.value(id) {
                    self.audio_volume = volume.clamp(0.0, 1.0);
                    self.player.set_volume(self.audio_volume);
                    self.audio_player
                        .set_volume(&mut self.ui, self.audio_volume);
                }
            }
            Event::Selected(id, index) if id == self.list => self.select(renderer, index),
            Event::Selected(id, index) if id == self.categories => {
                self.selected_category = index;
                self.selected_subcategory = 0;
                self.visible_items = self.filtered_items(index, 0);
                if self.mode == Mode::Dialogs {
                    self.ui.set_list_items(
                        self.subcategories,
                        self.visible_items
                            .iter()
                            .map(|name| self.display_resource_name(name))
                            .collect(),
                    );
                } else {
                    self.ui
                        .set_list_items(self.subcategories, self.subcategory_names(index));
                    self.ui.set_list_items(
                        self.list,
                        self.visible_items
                            .iter()
                            .map(|n| self.display_resource_name(n))
                            .collect(),
                    );
                }
                self.ui.list_select(self.subcategories, Some(0));
                self.select(renderer, 0);
            }
            Event::Selected(id, index) if id == self.subcategories => {
                if self.mode == Mode::Dialogs {
                    self.selected_subcategory = index;
                    self.select(renderer, index);
                    return;
                }
                self.selected_subcategory = index;
                self.visible_items = self.filtered_items(self.selected_category, index);
                self.ui.set_list_items(
                    self.list,
                    self.visible_items
                        .iter()
                        .map(|n| self.display_resource_name(n))
                        .collect(),
                );
                self.select(renderer, 0);
            }
            _ => self.dialog_event(event),
        }
    }

    pub fn pointer_move(&mut self, renderer: &mut Renderer, x: i32, y: i32) {
        self.ui.pointer_move(x, y, self.root);
        if self.audio_window_dragging && self.mode == Mode::Audio {
            let area = self.ui.node(self.main).rect;
            let rect = self.ui.node(self.audio_player.window).rect;
            let width = rect[2] - rect[0];
            let height = rect[3] - rect[1];
            self.audio_window_position = Some([
                (x - self.audio_drag_offset[0]).clamp(area[0], (area[2] - width).max(area[0])),
                (y - self.audio_drag_offset[1]).clamp(area[1], (area[3] - height).max(area[1])),
            ]);
        }
        if let (true, Some((px, py))) = (self.dragging, self.last_cursor) {
            self.camera.yaw -= (x - px) as f32 * 0.01;
            self.camera.pitch = (self.camera.pitch + (y - py) as f32 * 0.01).clamp(-1.5, 1.5);
        }
        self.last_cursor = Some((x, y));
        self.handle_pending(renderer);
    }

    pub fn pointer_button(&mut self, renderer: &mut Renderer, down: bool) {
        let (x, y) = self.last_cursor.unwrap_or((-1, -1));
        let header = self.ui.node(self.audio_player.drag_handle).rect;
        let start_drag = down && self.mode == Mode::Audio && rect_contains(header, x, y);
        let event = self.ui.pointer_button(down, self.root);
        if start_drag && event.is_none() {
            let window = self.ui.node(self.audio_player.window).rect;
            self.audio_window_dragging = true;
            self.audio_drag_offset = [x - window[0], y - window[1]];
        }
        if !down {
            self.audio_window_dragging = false;
        }
        let in_view = rect_contains(self.ui.node(self.main).rect, x, y);
        self.dragging = down && event.is_none() && in_view && self.mode == Mode::Models;
        if let Some(e) = event {
            self.handle(renderer, e);
        }
        self.handle_pending(renderer);
    }

    pub fn wheel(&mut self, lines: f32) {
        if !self.ui.wheel((lines * 40.0) as i32) && self.mode == Mode::Models {
            self.camera.distance *= (1.0 - lines * 0.1).clamp(0.5, 1.5);
        }
    }

    fn dialog_event(&mut self, event: Event) {
        let Some(dialog) = &self.dialog else { return };
        // Application controllers receive events only from their attached subtree.
        let mut handled = false;
        dialog.dispatch(&self.ui, event, |_, _| handled = true);
        let id = match event {
            Event::Clicked(id)
            | Event::Changed(id)
            | Event::Selected(id, _)
            | Event::Submitted(id)
            | Event::Link(id, _) => id,
        };
        if let Event::Link(id, index) = event {
            if let Some(target) = self.ui.document_link(id, index).map(str::to_owned) {
                self.set_status(&format!("Link: {target}"));
                return;
            }
        }
        if id >= self.permanent_nodes {
            let action = self.ui.node(id).name.clone();
            self.set_status(&format!(
                "{}{}",
                action,
                if handled { " updated" } else { " selected" }
            ));
        }
    }

    fn handle_pending(&mut self, renderer: &mut Renderer) {
        for event in self.ui.drain_events() {
            self.handle(renderer, event);
        }
    }

    pub fn key(&mut self, renderer: &mut Renderer, key: Key, text: Option<&str>) {
        let handled =
            self.ui.key(self.root, key) || text.is_some_and(|text| self.ui.text_input(text));
        self.handle_pending(renderer);
        if !handled {
            match key {
                Key::Right if self.mode == Mode::Models && !self.animation_clips.is_empty() => {
                    self.seek_animation(0.1)
                }
                Key::Left if self.mode == Mode::Models && !self.animation_clips.is_empty() => {
                    self.seek_animation(-0.1)
                }
                Key::Down | Key::Right => self.step(renderer, 1),
                Key::Up | Key::Left => self.step(renderer, -1),
                Key::PageDown => self.step(renderer, 20),
                Key::PageUp => self.step(renderer, -20),
                _ => {}
            }
        }
    }

    pub fn text_input(&mut self, renderer: &mut Renderer, text: &str) {
        self.ui.text_input(text);
        self.handle_pending(renderer);
    }
}
