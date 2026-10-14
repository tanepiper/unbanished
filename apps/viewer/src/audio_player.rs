//! Reusable floating audio-player UI built from the game UI runtime.

use banished_formats::ui::flags;
use banished_ui::{Control, Kind, Node, NodeId, SpriteRef, Ui};

const FONT: &str = r"Font\FontMedium.rsc:resource";
const FONT_SMALL: &str = r"Font\FontSmall.rsc:resource";
const BUTTON: &str = r"Dialog\SharedElements.rsc:buttonOk";
const PANEL: &str = r"Dialog\SharedElements.rsc:sunkenBorder";
const BAR: &str = r"Dialog\SharedElements.rsc:dialogBorder";
const SPRITES: &str = r"Dialog\SpriteSheet.rsc:resource";

pub struct AudioPlayerUi {
    pub window: NodeId,
    pub drag_handle: NodeId,
    pub play: NodeId,
    play_icon: NodeId,
    play_caption: NodeId,
    play_sprite: Option<SpriteRef>,
    pause_sprite: Option<SpriteRef>,
    pub stop: NodeId,
    pub previous: NodeId,
    pub next: NodeId,
    pub loop_toggle: NodeId,
    loop_caption: NodeId,
    pub seek: NodeId,
    pub volume: NodeId,
    track: NodeId,
    position: NodeId,
    volume_text: NodeId,
    duration: f32,
}

impl AudioPlayerUi {
    pub fn new(ui: &mut Ui) -> Self {
        let window = ui.group();
        {
            let node = ui.node_mut(window);
            node.min_size = [520, 208];
            node.alignment = 0;
        }

        let background = ui.instantiate(PANEL).unwrap_or_else(|| ui.group());
        ui.node_mut(background).flags = flags::FILL_WIDTH | flags::FILL_HEIGHT;
        ui.add_child(window, background);

        let content = ui.ribbon(true, 4);
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
        ui.add_child(drag_handle, header_background);
        let header_content = ui.ribbon(false, 6);
        {
            let node = ui.node_mut(header_content);
            node.flags = flags::FILL_WIDTH | flags::FILL_HEIGHT;
            node.margins = [8, 8, 4, 4];
        }
        let heading = ui.text(FONT, "Audio Player");
        ui.node_mut(heading).flags = flags::FILL_WIDTH;
        ui.add_child(header_content, heading);
        ui.add_child(drag_handle, header_content);
        ui.add_child(content, drag_handle);

        let track = ui.text(FONT, "Select a sound from the list");
        ui.node_mut(track).flags = flags::FILL_WIDTH;
        ui.add_child(content, track);

        let position = ui.text(FONT_SMALL, "00:00 / 00:00");
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
                size: [0, 0],
            }));
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
        let (previous, _, _) = make_button(ui, "Prev", Some("ArrowLeft"));
        let (play, play_icon, play_caption) = make_button(ui, "Play", Some("ToolbarPlay"));
        let (stop, _, _) = make_button(ui, "Stop", None);
        let (next, _, _) = make_button(ui, "Next", Some("ArrowRight"));
        let (loop_toggle, _, loop_caption) = make_button(ui, "Loop Off", None);
        ui.add_child(content, buttons);

        let volume_text = ui.text(FONT_SMALL, "Volume 100%");
        ui.add_child(content, volume_text);
        let volume = ui
            .instantiate(r"Dialog\Options.rsc:sliderMusic")
            .unwrap_or_else(|| ui.slider([0.0, 1.0]));
        {
            let node = ui.node_mut(volume);
            node.flags = flags::FILL_WIDTH;
            node.min_size = [0, 24];
            node.margins = [4, 4, 0, 0];
        }
        if let Kind::Control(Control::Slider(slider)) = &mut ui.node_mut(volume).kind {
            slider.number = None;
        }
        ui.add_child(content, volume);

        Self {
            window,
            drag_handle,
            play,
            play_icon,
            play_caption,
            play_sprite: ui.res.sprite(Some(SPRITES), "ToolbarPlay"),
            pause_sprite: ui.res.sprite(Some(SPRITES), "ToolbarPause"),
            stop,
            previous,
            next,
            loop_toggle,
            loop_caption,
            seek,
            volume,
            track,
            position,
            volume_text,
            duration: 0.0,
        }
    }

    pub fn set_track(&mut self, ui: &mut Ui, name: &str, duration: f32, looping: bool) {
        self.duration = duration.max(0.0);
        ui.set_text(self.track, name);
        ui.set_slider_range(self.seek, [0.0, self.duration.max(0.001)]);
        self.set_position(ui, 0.0);
        self.set_looping(ui, looping);
        self.set_playing(ui, false);
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

    pub fn set_looping(&self, ui: &mut Ui, looping: bool) {
        ui.set_text(
            self.loop_caption,
            if looping { "Loop On" } else { "Loop Off" },
        );
    }

    pub fn set_position(&self, ui: &mut Ui, seconds: f32) {
        let position = seconds.clamp(0.0, self.duration);
        ui.set_value(self.seek, position);
        ui.set_text(
            self.position,
            &format!("{} / {}", format_time(position), format_time(self.duration)),
        );
    }

    pub fn set_volume(&self, ui: &mut Ui, volume: f32) {
        let volume = volume.clamp(0.0, 1.0);
        ui.set_value(self.volume, volume);
        ui.set_text(
            self.volume_text,
            &format!("Volume {:>3}%", (volume * 100.0).round()),
        );
    }
}

fn format_time(seconds: f32) -> String {
    let seconds = seconds.max(0.0) as u64;
    format!("{:02}:{:02}", seconds / 60, seconds % 60)
}
