//! Input: the interface's events, the pointer, the wheel and the keyboard.

use super::*;

impl Tool {
    pub(super) fn handle(&mut self, renderer: &mut Renderer, event: Event) {
        match event {
            Event::Clicked(id) => {
                if let Some(&(mode, _)) = self.mode_buttons.iter().find(|(_, b)| *b == id) {
                    self.set_mode(renderer, mode);
                } else if id == self.model_player.play && self.mode == Mode::Models {
                    self.toggle_animation();
                } else if id == self.model_player.stop && self.mode == Mode::Models {
                    self.animation_playing = false;
                    self.animation_elapsed = 0.0;
                    self.animation_started = Instant::now();
                    self.refresh_animation_label();
                } else if id == self.model_player.previous_clip && self.mode == Mode::Models {
                    self.previous_animation();
                } else if id == self.model_player.next_clip && self.mode == Mode::Models {
                    self.next_animation();
                } else if id == self.model_player.step_back && self.mode == Mode::Models {
                    self.seek_animation(-0.1);
                } else if id == self.model_player.step_forward && self.mode == Mode::Models {
                    self.seek_animation(0.1);
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
                } else if id == self.dialog_player.play && self.mode == Mode::Dialogs {
                    self.toggle_dialog_animation();
                } else if id == self.dialog_player.stop && self.mode == Mode::Dialogs {
                    self.dialog_animation_playing = false;
                    self.dialog_animation_elapsed = 0.0;
                    self.dialog_animation_started = Instant::now();
                    self.ui.set_animation_frame(0);
                    self.refresh_dialog_animation_label();
                } else if id == self.dialog_player.previous && self.mode == Mode::Dialogs {
                    self.step_dialog_frame(-1);
                } else if id == self.dialog_player.next && self.mode == Mode::Dialogs {
                    self.step_dialog_frame(1);
                } else if id == self.play {
                    if self.mode == Mode::Audio {
                        self.play_selected();
                    } else {
                        self.toggle_animation();
                    }
                } else if id == self.stop {
                    if self.mode == Mode::Audio {
                        self.player.stop();
                        self.set_status("Stopped");
                    } else {
                        self.animation_playing = false;
                        self.animation_elapsed = 0.0;
                        self.ui.set_text(self.play, "Play");
                        self.refresh_animation_label();
                    }
                } else if let Some((mode, _)) =
                    self.render_buttons.iter().find(|(_, button)| *button == id)
                {
                    self.set_render_mode(*mode);
                } else {
                    self.dialog_event(event);
                }
            }
            Event::Changed(id) if id == self.model_player.seek && self.mode == Mode::Models => {
                if let Some(position) = self.ui.value(id) {
                    self.seek_animation_to(position);
                }
            }
            Event::Changed(id) if id == self.dialog_player.seek && self.mode == Mode::Dialogs => {
                if let Some(frame) = self.ui.value(id) {
                    let Some((_, count, fps)) = self.ui.animation_info() else {
                        return;
                    };
                    if count > 0 {
                        let frame = (frame.round() as usize).min(count - 1);
                        self.dialog_animation_playing = false;
                        self.dialog_animation_elapsed = frame as f32 / fps.max(1) as f32;
                        self.dialog_animation_started = Instant::now()
                            - std::time::Duration::from_secs_f32(self.dialog_animation_elapsed);
                        self.ui.set_animation_frame(frame);
                        self.refresh_dialog_animation_label();
                    }
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
        if self.model_window_dragging && self.mode == Mode::Models {
            let area = self.ui.node(self.main).rect;
            let rect = self.ui.node(self.model_player.window).rect;
            let width = rect[2] - rect[0];
            let height = rect[3] - rect[1];
            self.model_window_position = Some([
                (x - self.model_drag_offset[0]).clamp(area[0], (area[2] - width).max(area[0])),
                (y - self.model_drag_offset[1]).clamp(area[1], (area[3] - height).max(area[1])),
            ]);
        }
        if self.dialog_window_dragging && self.mode == Mode::Dialogs {
            let area = self.ui.node(self.main).rect;
            let rect = self.ui.node(self.dialog_player.window).rect;
            let width = rect[2] - rect[0];
            let height = rect[3] - rect[1];
            self.dialog_window_position = Some([
                (x - self.dialog_drag_offset[0]).clamp(area[0], (area[2] - width).max(area[0])),
                (y - self.dialog_drag_offset[1]).clamp(area[1], (area[3] - height).max(area[1])),
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
        let audio_header = self.ui.node(self.audio_player.drag_handle).rect;
        let start_audio_drag =
            down && self.mode == Mode::Audio && rect_contains(audio_header, x, y);
        let model_header = self.ui.node(self.model_player.drag_handle).rect;
        let start_model_drag = down
            && self.mode == Mode::Models
            && self.ui.node(self.model_player.window).visible()
            && rect_contains(model_header, x, y);
        let dialog_header = self.ui.node(self.dialog_player.drag_handle).rect;
        let start_dialog_drag = down
            && self.mode == Mode::Dialogs
            && self.ui.node(self.dialog_player.window).visible()
            && rect_contains(dialog_header, x, y);
        let event = self.ui.pointer_button(down, self.root);
        if start_audio_drag && event.is_none() {
            let window = self.ui.node(self.audio_player.window).rect;
            self.audio_window_dragging = true;
            self.audio_drag_offset = [x - window[0], y - window[1]];
        }
        if start_model_drag && event.is_none() {
            let window = self.ui.node(self.model_player.window).rect;
            self.model_window_dragging = true;
            self.model_drag_offset = [x - window[0], y - window[1]];
        }
        if start_dialog_drag && event.is_none() {
            let window = self.ui.node(self.dialog_player.window).rect;
            self.dialog_window_dragging = true;
            self.dialog_drag_offset = [x - window[0], y - window[1]];
        }
        if !down {
            self.audio_window_dragging = false;
            self.model_window_dragging = false;
            self.dialog_window_dragging = false;
        }
        let in_view = rect_contains(self.ui.node(self.main).rect, x, y);
        self.dragging = down
            && event.is_none()
            && in_view
            && self.mode == Mode::Models
            && !self.model_window_dragging;
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

    pub(super) fn dialog_event(&mut self, event: Event) {
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

    pub(super) fn handle_pending(&mut self, renderer: &mut Renderer) {
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
