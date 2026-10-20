//! The asset browser, built with the reimplemented game UI (`banished_ui`) and the game's own
//! widget templates from `Dialog\SharedElements`.

mod animation;
mod browse;
mod input;
mod screen;
mod select;

use banished_formats::texture::decode_rgba;
use banished_formats::types::AnimationClip;
use banished_formats::ui::flags;
use banished_ui::{DialogInstance, Event, Key, Kind, Node, NodeId, Ui};
use glam::{Mat4, Vec3};
use web_time::Instant;

use crate::assets::Assets;
use crate::audio_player::AudioPlayerUi;
use crate::dialog_player::DialogPlayerUi;
use crate::model_player::ModelPlayerUi;
use crate::player::Player;
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
    model_player: ModelPlayerUi,
    model_window_position: Option<[i32; 2]>,
    model_window_dragging: bool,
    model_drag_offset: [i32; 2],
    dialog_player: DialogPlayerUi,
    dialog_window_position: Option<[i32; 2]>,
    dialog_window_dragging: bool,
    dialog_drag_offset: [i32; 2],
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

fn layout_floating_window(
    ui: &mut Ui,
    window: NodeId,
    area: [i32; 4],
    min_size: [i32; 2],
    saved_position: &mut Option<[i32; 2]>,
) {
    ui.layout_dialog(window, area);
    let rect = ui.node(window).rect;
    let width = (rect[2] - rect[0]).max(min_size[0]);
    let height = (rect[3] - rect[1]).max(min_size[1]);
    let mut position = saved_position.unwrap_or([
        area[0] + ((area[2] - area[0] - width) / 2).max(0),
        area[1] + ((area[3] - area[1] - height) / 2).max(0),
    ]);
    position[0] = position[0].clamp(area[0], (area[2] - width).max(area[0]));
    position[1] = position[1].clamp(area[1], (area[3] - height).max(area[1]));
    *saved_position = Some(position);
    ui.layout_dialog(
        window,
        [
            position[0],
            position[1],
            position[0] + width,
            position[1] + height,
        ],
    );
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
        let model_player = ModelPlayerUi::new(&mut ui);
        ui.add_child(main, model_player.window);
        let dialog_player = DialogPlayerUi::new(&mut ui);
        ui.add_child(main, dialog_player.window);
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
            model_player,
            model_window_position: None,
            model_window_dragging: false,
            model_drag_offset: [0, 0],
            dialog_player,
            dialog_window_position: None,
            dialog_window_dragging: false,
            dialog_drag_offset: [0, 0],
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

    // ------------------------------------------------------------ frame

    // ------------------------------------------------------------ input
}
