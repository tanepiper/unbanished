//! Floating frame controls for animated dialogs.

use banished_formats::ui::flags;
use banished_ui::{Control, Kind, Node, NodeId, SpriteRef, Ui};

const FONT: &str = r"Font\FontMedium.rsc:resource";
const FONT_SMALL: &str = r"Font\FontSmall.rsc:resource";
const BUTTON: &str = r"Dialog\SharedElements.rsc:buttonOk";
const PANEL: &str = r"Dialog\SharedElements.rsc:sunkenBorder";
const BAR: &str = r"Dialog\SharedElements.rsc:dialogBorder";
const SPRITES: &str = r"Dialog\SpriteSheet.rsc:resource";

pub struct DialogPlayerUi {
    pub window: NodeId,
    pub drag_handle: NodeId,
    pub previous: NodeId,
    pub play: NodeId,
    play_icon: NodeId,
    play_caption: NodeId,
    play_sprite: Option<SpriteRef>,
    pause_sprite: Option<SpriteRef>,
    pub stop: NodeId,
    pub next: NodeId,
    pub seek: NodeId,
    info: NodeId,
    frame: NodeId,
}

impl DialogPlayerUi {
    pub fn new(ui: &mut Ui) -> Self {
        let window = ui.group();
        ui.node_mut(window).min_size = [440, 184];
        let background = ui.instantiate(PANEL).unwrap_or_else(|| ui.group());
        ui.node_mut(background).flags = flags::FILL_WIDTH | flags::FILL_HEIGHT;
        ui.node_mut(background).color = 0xFFE9D7B2;
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
        let header_sprite = ui.res.sprite(Some(SPRITES), "ToolbarPlay");
        let icon = ui.add(Node::new(Kind::Image {
            sprite: header_sprite,
            size: [16, 16],
        }));
        ui.node_mut(icon).color = 0xFFFFD46E;
        let heading = ui.text(FONT, "Dialog Animation");
        ui.node_mut(heading).flags = flags::FILL_WIDTH;
        ui.node_mut(heading).color = 0xFFFFE8B2;
        ui.add_child(header, icon);
        ui.add_child(header, heading);
        ui.add_child(drag_handle, header);
        ui.add_child(content, drag_handle);

        let info = ui.text(FONT_SMALL, "Select a dialog");
        {
            let node = ui.node_mut(info);
            node.min_size = [0, 38];
            node.margins = [8, 8, 6, 6];
            node.color = 0xFFEFE0C4;
            if let Kind::Text(text) = &mut node.kind {
                text.wrap = true;
                text.max_width = 420;
            }
        }
        ui.node_mut(info).border = ui.instantiate(PANEL);
        ui.add_child(content, info);

        let frame = ui.text(FONT, "No animation frames");
        {
            let node = ui.node_mut(frame);
            node.min_size = [0, 38];
            node.margins = [10, 10, 4, 4];
            node.color = 0xFFFFE8B2;
        }
        let frame_background = ui.instantiate(PANEL).unwrap_or_else(|| ui.group());
        ui.node_mut(frame_background).flags = flags::FILL_WIDTH | flags::FILL_HEIGHT;
        ui.node_mut(frame).border = Some(frame_background);
        ui.add_child(content, frame);

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
        let make_button = |ui: &mut Ui, label: &str, sprite_name: Option<&str>| {
            let button = ui.instantiate(BUTTON).unwrap_or_else(|| ui.group());
            let button_content = ui.ribbon(false, 5);
            {
                let node = ui.node_mut(button_content);
                node.flags = flags::FILL_WIDTH | flags::FILL_HEIGHT;
                node.margins = [5, 5, 2, 2];
            }
            let sprite = sprite_name.and_then(|name| ui.res.sprite(Some(SPRITES), name));
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
            ui.node_mut(button).min_size = [82, 40];
            ui.add_child(buttons, button);
            (button, icon, caption)
        };
        let (previous, _, _) = make_button(ui, "Prev frame", Some("ArrowLeft"));
        let (play, play_icon, play_caption) = make_button(ui, "Play", Some("ToolbarPlay"));
        let (stop, _, _) = make_button(ui, "First frame", None);
        let (next, _, _) = make_button(ui, "Next frame", Some("ArrowRight"));
        ui.add_child(content, buttons);

        Self {
            window,
            drag_handle,
            previous,
            play,
            play_icon,
            play_caption,
            play_sprite: ui.res.sprite(Some(SPRITES), "ToolbarPlay"),
            pause_sprite: ui.res.sprite(Some(SPRITES), "ToolbarPause"),
            stop,
            next,
            seek,
            info,
            frame,
        }
    }

    pub fn set_info(&self, ui: &mut Ui, text: &str) {
        ui.set_text(self.info, text);
    }

    pub fn set_frame(&self, ui: &mut Ui, frame: usize, count: usize, fps: i32, playing: bool) {
        let animated = count > 0;
        if animated {
            ui.set_text(
                self.frame,
                &format!(
                    "Frame {}/{}    {} fps",
                    frame.min(count - 1) + 1,
                    count,
                    fps
                ),
            );
            ui.set_slider_range(self.seek, [0.0, (count - 1).max(1) as f32]);
            ui.set_value(self.seek, frame.min(count - 1) as f32);
        } else {
            ui.set_text(self.frame, "No animation frames");
            ui.set_slider_range(self.seek, [0.0, 1.0]);
            ui.set_value(self.seek, 0.0);
        }
        for id in [self.previous, self.play, self.stop, self.next, self.seek] {
            ui.node_mut(id).enabled = animated;
        }
        ui.set_text(self.play_caption, if playing { "Pause" } else { "Play" });
        if let Kind::Image { sprite, .. } = &mut ui.node_mut(self.play_icon).kind {
            *sprite = if playing {
                self.pause_sprite
            } else {
                self.play_sprite
            };
        }
    }
}
