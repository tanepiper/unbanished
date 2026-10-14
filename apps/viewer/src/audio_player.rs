//! Reusable floating audio-player UI built from the game UI runtime.

use banished_formats::ui::flags;
use banished_ui::{NodeId, Ui};

const FONT: &str = r"Font\FontMedium.rsc:resource";
const FONT_SMALL: &str = r"Font\FontSmall.rsc:resource";
const BUTTON: &str = r"Dialog\SharedElements.rsc:buttonOk";
const PANEL: &str = r"Dialog\SharedElements.rsc:sunkenBorder";
const BAR: &str = r"Dialog\SharedElements.rsc:dialogBorder";

pub struct AudioPlayerUi {
    pub window: NodeId,
    pub drag_handle: NodeId,
    pub play: NodeId,
    pub stop: NodeId,
    pub previous: NodeId,
    pub next: NodeId,
    pub loop_toggle: NodeId,
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
            node.min_size = [520, 220];
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
        let seek = ui.slider([0.0, 1.0]);
        {
            let node = ui.node_mut(seek);
            node.flags = flags::FILL_WIDTH;
            node.min_size = [0, 24];
            node.margins = [4, 4, 0, 0];
        }
        ui.add_child(content, seek);

        let buttons = ui.ribbon(false, 6);
        ui.node_mut(buttons).flags = flags::FILL_WIDTH;
        let make_button = |ui: &mut Ui, label: &str| {
            let button = ui.instantiate(BUTTON).unwrap_or_else(|| ui.group());
            ui.set_text(button, label);
            ui.add_child(buttons, button);
            button
        };
        let previous = make_button(ui, "Prev");
        let play = make_button(ui, "Play");
        let stop = make_button(ui, "Stop");
        let next = make_button(ui, "Next");
        let loop_toggle = make_button(ui, "Loop Off");
        ui.add_child(content, buttons);

        let volume_text = ui.text(FONT_SMALL, "Volume 100%");
        ui.add_child(content, volume_text);
        let volume = ui.slider([0.0, 1.0]);
        {
            let node = ui.node_mut(volume);
            node.flags = flags::FILL_WIDTH;
            node.min_size = [0, 24];
            node.margins = [4, 4, 0, 0];
        }
        ui.add_child(content, volume);

        Self {
            window,
            drag_handle,
            play,
            stop,
            previous,
            next,
            loop_toggle,
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
        ui.set_text(self.play, if playing { "Pause" } else { "Play" });
    }

    pub fn set_looping(&self, ui: &mut Ui, looping: bool) {
        ui.set_text(
            self.loop_toggle,
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
