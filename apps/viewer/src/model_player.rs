//! Floating clip and timeline controls for animated models.

use unbanished_formats::ui::flags;
use unbanished_ui::{Control, Kind, Node, NodeId, SpriteRef, Ui};

const FONT: &str = r"Font\FontMedium.rsc:resource";
const FONT_SMALL: &str = r"Font\FontSmall.rsc:resource";
const BUTTON: &str = r"Dialog\SharedElements.rsc:buttonOk";
const PANEL: &str = r"Dialog\SharedElements.rsc:sunkenBorder";
const BAR: &str = r"Dialog\SharedElements.rsc:dialogBorder";
const SPRITES: &str = r"Dialog\SpriteSheet.rsc:resource";

pub struct ModelPlayerUi {
    pub window: NodeId,
    pub drag_handle: NodeId,
    pub play: NodeId,
    play_icon: NodeId,
    play_caption: NodeId,
    play_sprite: Option<SpriteRef>,
    pause_sprite: Option<SpriteRef>,
    pub stop: NodeId,
    pub previous_clip: NodeId,
    pub next_clip: NodeId,
    pub step_back: NodeId,
    pub step_forward: NodeId,
    pub seek: NodeId,
    model_info: NodeId,
    clip: NodeId,
    position: NodeId,
    duration: f32,
}

impl ModelPlayerUi {
    pub fn new(ui: &mut Ui) -> Self {
        let window = ui.group();
        ui.node_mut(window).min_size = [540, 206];

        let background = ui.instantiate(PANEL).unwrap_or_else(|| ui.group());
        {
            let node = ui.node_mut(background);
            node.flags = flags::FILL_WIDTH | flags::FILL_HEIGHT;
            node.color = 0xFFE9D7B2;
        }
        ui.add_child(window, background);

        let content = ui.ribbon(true, 5);
        {
            let node = ui.node_mut(content);
            node.flags = flags::FILL_WIDTH | flags::FILL_HEIGHT;
            node.margins = [10, 10, 8, 8];
        }
        ui.add_child(window, content);

        let drag_handle = ui.group();
        ui.node_mut(drag_handle).min_size = [0, 34];
        let header_background = ui.instantiate(BAR).unwrap_or_else(|| ui.group());
        ui.node_mut(header_background).flags = flags::FILL_WIDTH | flags::FILL_HEIGHT;
        ui.node_mut(header_background).color = 0xFFE6D4AE;
        ui.add_child(drag_handle, header_background);
        let header = ui.ribbon(false, 6);
        {
            let node = ui.node_mut(header);
            node.flags = flags::FILL_WIDTH | flags::FILL_HEIGHT;
            node.margins = [8, 8, 4, 4];
        }
        let heading_icon = ui.res.sprite(Some(SPRITES), "ToolbarPlay");
        let heading_icon = ui.add(Node::new(Kind::Image {
            sprite: heading_icon,
            size: [16, 16],
        }));
        ui.node_mut(heading_icon).color = 0xFFFFD46E;
        let heading = ui.text(FONT, "Model Animation");
        ui.node_mut(heading).color = 0xFFFFE8B2;
        ui.node_mut(heading).flags = flags::FILL_WIDTH;
        ui.add_child(header, heading_icon);
        ui.add_child(header, heading);
        ui.add_child(drag_handle, header);
        ui.add_child(content, drag_handle);

        let model_info = ui.text(FONT_SMALL, "");
        {
            let node = ui.node_mut(model_info);
            node.min_size = [0, 32];
            node.margins = [6, 6, 2, 2];
            node.color = 0xFFEFE0C4;
            if let Kind::Text(text) = &mut node.kind {
                text.wrap = true;
                text.max_width = 500;
            }
        }
        let model_info_border = ui.instantiate(PANEL);
        ui.node_mut(model_info).border = model_info_border;
        ui.add_child(content, model_info);

        let clip_card = ui.group();
        ui.node_mut(clip_card).min_size = [0, 38];
        let clip_background = ui.instantiate(PANEL).unwrap_or_else(|| ui.group());
        ui.node_mut(clip_background).flags = flags::FILL_WIDTH | flags::FILL_HEIGHT;
        ui.add_child(clip_card, clip_background);
        let clip = ui.text(FONT, "Select an animated model");
        {
            let node = ui.node_mut(clip);
            node.flags = flags::FILL_WIDTH;
            node.margins = [10, 10, 4, 4];
            node.color = 0xFFFFE8B2;
        }
        ui.add_child(clip_card, clip);
        ui.add_child(content, clip_card);

        let position = ui.text(FONT_SMALL, "00:00.00 / 00:00.00");
        ui.node_mut(position).color = 0xFFE6D6B8;
        ui.add_child(content, position);

        let seek = ui
            .instantiate(r"Dialog\Options.rsc:sliderMusic")
            .unwrap_or_else(|| ui.slider([0.0, 1.0]));
        {
            let node = ui.node_mut(seek);
            node.flags = flags::FILL_WIDTH;
            node.min_size = [0, 24];
            node.margins = [4, 4, 0, 0];
        }
        if let Kind::Control(Control::Slider(slider)) = &mut ui.node_mut(seek).kind {
            slider.number = None;
        }
        ui.add_child(content, seek);

        let buttons = ui.ribbon(false, 6);
        ui.node_mut(buttons).flags = flags::FILL_WIDTH;
        let make_button = |ui: &mut Ui, label: &str, icon_name: Option<&str>| {
            let button = ui.instantiate(BUTTON).unwrap_or_else(|| ui.group());
            let button_content = ui.ribbon(false, 5);
            {
                let node = ui.node_mut(button_content);
                node.flags = flags::FILL_WIDTH | flags::FILL_HEIGHT;
                node.margins = [5, 5, 2, 2];
            }
            let sprite = icon_name.and_then(|name| ui.res.sprite(Some(SPRITES), name));
            let icon = ui.add(Node::new(Kind::Image {
                sprite,
                size: [16, 16],
            }));
            ui.node_mut(icon).color = 0xFFFFD98A;
            let caption = ui.text(FONT, label);
            ui.node_mut(caption).margins = [2, 2, 0, 0];
            ui.add_child(button_content, icon);
            ui.add_child(button_content, caption);
            if let Kind::Button { content, .. } = &mut ui.node_mut(button).kind {
                *content = Some(button_content);
            }
            ui.node_mut(button).min_size = [74, 40];
            ui.add_child(buttons, button);
            (button, icon, caption)
        };
        let (previous_clip, _, _) = make_button(ui, "Prev clip", Some("ArrowLeft"));
        let (play, play_icon, play_caption) = make_button(ui, "Play", Some("ToolbarPlay"));
        let (stop, _, _) = make_button(ui, "Stop", None);
        let (next_clip, _, _) = make_button(ui, "Next clip", Some("ArrowRight"));
        let (step_back, _, _) = make_button(ui, "-0.1s", Some("ArrowLeftSmall"));
        let (step_forward, _, _) = make_button(ui, "+0.1s", Some("ArrowRightSmall"));
        ui.add_child(content, buttons);

        Self {
            window,
            drag_handle,
            play,
            play_icon,
            play_caption,
            play_sprite: ui.res.sprite(Some(SPRITES), "ToolbarPlay"),
            pause_sprite: ui.res.sprite(Some(SPRITES), "ToolbarPause"),
            stop,
            previous_clip,
            next_clip,
            step_back,
            step_forward,
            seek,
            model_info,
            clip,
            position,
            duration: 0.0,
        }
    }

    pub fn set_clip(&mut self, ui: &mut Ui, name: &str, index: usize, count: usize, duration: f32) {
        self.duration = duration.max(0.0);
        let label = if count == 0 {
            "No animation clips".to_owned()
        } else {
            format!("{name}    Clip {}/{}", index + 1, count)
        };
        ui.set_text(self.clip, &label);
        ui.set_slider_range(self.seek, [0.0, self.duration.max(0.001)]);
        let enabled = count > 0;
        for id in [
            self.previous_clip,
            self.play,
            self.stop,
            self.next_clip,
            self.step_back,
            self.step_forward,
            self.seek,
        ] {
            ui.node_mut(id).enabled = enabled;
        }
    }

    pub fn set_model_info(&self, ui: &mut Ui, text: &str) {
        ui.set_text(self.model_info, text);
    }

    pub fn set_playing(&self, ui: &mut Ui, playing: bool) {
        ui.set_text(self.play_caption, if playing { "Pause" } else { "Play" });
        if let Kind::Image { sprite, .. } = &mut ui.node_mut(self.play_icon).kind {
            *sprite = if playing {
                self.pause_sprite
            } else {
                self.play_sprite
            };
        }
    }

    pub fn set_position(&self, ui: &mut Ui, seconds: f32) {
        let position = seconds.clamp(0.0, self.duration);
        ui.set_value(self.seek, position);
        ui.set_text(
            self.position,
            &format!("{} / {}", format_time(position), format_time(self.duration)),
        );
    }
}

fn format_time(seconds: f32) -> String {
    let centiseconds = (seconds.max(0.0) * 100.0).round() as u64;
    format!(
        "{:02}:{:02}.{:02}",
        centiseconds / 6000,
        (centiseconds / 100) % 60,
        centiseconds % 100
    )
}
