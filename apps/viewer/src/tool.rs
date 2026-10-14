//! The asset browser, built with the reimplemented game UI (`banished_ui`) and the game's own
//! widget templates from `Dialog\SharedElements`.

use banished_formats::texture::decode_rgba;
use banished_formats::ui::flags;
use banished_ui::{DialogInstance, Event, Key, Kind, NodeId, Ui};
use glam::{Mat4, Vec3};

use crate::assets::Assets;
use crate::audio::Player;
use crate::render::{Frame, GpuMesh, Renderer, CLEAR_DARK, CLEAR_SKY};

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
    main: NodeId,
    info: NodeId,
    picture: NodeId,
    audio_controls: NodeId,
    play: NodeId,
    stop: NodeId,
    dialog_host: NodeId,
    status: NodeId,
    dialog: Option<DialogInstance>,
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
            ui.add_child(top, bar);
        }
        let buttons = ui.ribbon(false, 6);
        {
            let n = ui.node_mut(buttons);
            n.margins = [10, 10, 0, 0];
            n.alignment = 3; // middle-left
        }
        ui.add_child(top, buttons);
        let mut mode_buttons = Vec::new();
        for mode in Mode::ALL {
            if let Some(b) = ui.instantiate(BUTTON) {
                ui.set_text(b, mode.label());
                ui.node_mut(b).name = mode.label().to_string();
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
            ui.add_child(side, panel);
        }
        let list = ui.list(FONT_SMALL);
        {
            let n = ui.node_mut(list);
            n.flags = flags::FILL_WIDTH | flags::FILL_HEIGHT;
            n.margins = [6, 6, 6, 6];
        }
        ui.add_child(side, list);
        ui.add_child(root, side);

        // Main area.
        let main = ui.viewport();
        {
            let n = ui.node_mut(main);
            n.flags = flags::FILL_WIDTH | flags::FILL_HEIGHT;
            n.margins = [SIDEBAR_WIDTH + 16, 8, TOP_BAR_HEIGHT + 8, STATUS_HEIGHT + 8];
        }
        let picture = ui.picture();
        {
            let n = ui.node_mut(picture);
            n.flags = flags::FILL_WIDTH | flags::FILL_HEIGHT;
            n.margins = [12, 12, 48, 12];
        }
        ui.add_child(main, picture);
        let dialog_host = ui.group();
        ui.add_child(main, dialog_host);
        let info = ui.text(FONT, "");
        {
            let n = ui.node_mut(info);
            n.margins = [12, 12, 10, 0];
            if let Kind::Text(t) = &mut n.kind {
                t.wrap = true;
                t.max_width = 900;
            }
        }
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
        ui.add_child(audio_controls, play);
        ui.add_child(audio_controls, stop);
        ui.add_child(main, audio_controls);
        ui.add_child(root, main);

        // Status line.
        let status = ui.text(FONT_SMALL, "");
        {
            let n = ui.node_mut(status);
            n.alignment = 6; // bottom-left
            n.margins = [12, 0, 0, 8];
        }
        ui.add_child(root, status);

        let permanent_nodes = ui.nodes.len();
        let mut tool = Tool {
            assets,
            ui,
            root,
            mode_buttons,
            list,
            main,
            info,
            picture,
            audio_controls,
            play,
            stop,
            dialog_host,
            status,
            dialog: None,
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

    fn display_name(entry: &str) -> String {
        entry.trim_end_matches(".crs").replace('\\', "/")
    }

    pub fn set_status(&mut self, text: &str) {
        log::info!("{text}");
        self.ui.set_text(self.status, text);
    }

    fn set_info(&mut self, text: &str) {
        self.ui.set_text(self.info, text);
    }

    pub fn set_mode(&mut self, renderer: &mut Renderer, mode: Mode) {
        self.mode = mode;
        self.player.stop();
        let names = self
            .items(mode)
            .iter()
            .map(|n| Self::display_name(n))
            .collect();
        self.ui.set_list_items(self.list, names);
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
        self.ui
            .set_visible(self.audio_controls, mode == Mode::Audio);
        self.ui.set_visible(self.dialog_host, mode == Mode::Dialogs);
        self.ui.node_mut(self.dialog_host).children.clear();
        self.mesh = None;
        self.set_info("");
        self.select(renderer, 0);
    }

    pub fn select(&mut self, renderer: &mut Renderer, index: usize) {
        let Some(name) = self.items(self.mode).get(index).cloned() else {
            return;
        };
        self.ui.list_select(self.list, Some(index));
        self.ui.clear_input();
        self.ui.nodes.truncate(self.permanent_nodes);
        self.ui.node_mut(self.dialog_host).children.clear();
        self.dialog = None;
        let shown = Self::display_name(&name);
        match self.mode {
            Mode::Models => match renderer.load_mesh(&self.assets, &name) {
                Ok(mesh) => {
                    self.camera.target = mesh.center;
                    self.camera.distance = mesh.radius * 2.6;
                    let parts = mesh.part_count();
                    self.mesh = Some(mesh);
                    self.set_info(&format!(
                        "{shown}\n{parts} part(s)   drag to orbit, wheel to zoom"
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
                let reference = format!(
                    "{}.rsc:resource",
                    name.trim_end_matches("_resource.crs")
                        .trim_end_matches(".crs")
                );
                self.ui.node_mut(self.dialog_host).children.clear();
                match self.ui.instantiate_dialog(&reference) {
                    Some(d) => {
                        self.ui.add_child(self.dialog_host, d.root);
                        self.set_info(&reference);
                        self.dialog = Some(d);
                    }
                    None => self.set_info(&format!("{shown}\nUnable to load dialog")),
                }
            }
        }
    }

    pub fn step(&mut self, renderer: &mut Renderer, delta: isize) {
        let n = self.items(self.mode).len() as isize;
        if n == 0 {
            return;
        }
        let current = match &self.ui.node(self.list).kind {
            Kind::List(l) => l.selected.unwrap_or(0) as isize,
            _ => 0,
        };
        self.select(renderer, (current + delta).rem_euclid(n) as usize);
    }

    fn play_selected(&mut self) {
        let Kind::List(l) = &self.ui.node(self.list).kind else {
            return;
        };
        let Some(name) = l.selected.and_then(|i| self.assets.sounds.get(i)).cloned() else {
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
                self.player.play(pcm, sound.looping)
            });
        match result {
            Ok(()) => self.set_status(&format!("Playing {}", Self::display_name(&name))),
            Err(e) => self.set_status(&format!("{}: {e}", Self::display_name(&name))),
        }
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
            }
        }
        self.ui.node_mut(self.dialog_host).children = dialogs;
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
                } else if id == self.play {
                    self.play_selected();
                } else if id == self.stop {
                    self.player.stop();
                    self.set_status("Stopped");
                } else {
                    self.dialog_event(event);
                }
            }
            Event::Selected(id, index) if id == self.list => self.select(renderer, index),
            _ => self.dialog_event(event),
        }
    }

    pub fn pointer_move(&mut self, renderer: &mut Renderer, x: i32, y: i32) {
        self.ui.pointer_move(x, y, self.root);
        if let (true, Some((px, py))) = (self.dragging, self.last_cursor) {
            self.camera.yaw -= (x - px) as f32 * 0.01;
            self.camera.pitch = (self.camera.pitch + (y - py) as f32 * 0.01).clamp(-1.5, 1.5);
        }
        self.last_cursor = Some((x, y));
        self.handle_pending(renderer);
    }

    pub fn pointer_button(&mut self, renderer: &mut Renderer, down: bool) {
        let event = self.ui.pointer_button(down, self.root);
        let (x, y) = self.last_cursor.unwrap_or((-1, -1));
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
