//! Playing a model's animations and a dialog's animated images: play, pause, step, seek and their
//! labels.

use super::*;

impl Tool {
    pub(super) fn animation_duration(&self) -> f32 {
        self.animation_clips
            .get(self.animation_index)
            .map(|clip| clip.frames.len() as f32 / clip.sample_rate.max(0.001))
            .unwrap_or(0.0)
    }

    pub(super) fn refresh_animation_label(&mut self) {
        if let Some(clip) = self.animation_clips.get(self.animation_index) {
            let duration = self.animation_duration();
            let position = if clip.looping && duration > 0.0 {
                self.animation_elapsed.rem_euclid(duration)
            } else {
                self.animation_elapsed
            };
            self.model_player.set_clip(
                &mut self.ui,
                &clip.name,
                self.animation_index,
                self.animation_clips.len(),
                duration,
            );
            self.model_player.set_position(&mut self.ui, position);
            self.model_player
                .set_playing(&mut self.ui, self.animation_playing);
        } else {
            self.model_player.set_clip(&mut self.ui, "", 0, 0, 0.0);
            self.model_player.set_position(&mut self.ui, 0.0);
            self.model_player.set_playing(&mut self.ui, false);
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
                    self.model_player.set_playing(&mut self.ui, false);
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
            self.ui.update_containers();
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

    pub(super) fn refresh_dialog_animation_label(&mut self) {
        if let Some((frame, count, fps)) = self.ui.animation_info() {
            self.dialog_player.set_frame(
                &mut self.ui,
                frame,
                count,
                fps,
                self.dialog_animation_playing,
            );
        } else {
            self.dialog_player.set_frame(&mut self.ui, 0, 0, 0, false);
        }
    }

    pub(super) fn toggle_dialog_animation(&mut self) {
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
        self.refresh_dialog_animation_label();
    }

    pub(super) fn step_dialog_frame(&mut self, delta: isize) {
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
        self.refresh_dialog_animation_label();
    }

    pub(super) fn dialog_reference(name: &str) -> String {
        let stem = name.trim_end_matches(".crs");
        let (path, object) = stem.rsplit_once('_').unwrap_or((stem, "resource"));
        format!("{path}.rsc:{object}")
    }

    pub(super) fn toggle_animation(&mut self) {
        if self.animation_clips.is_empty() {
            return;
        }
        self.animation_playing = !self.animation_playing;
        if self.animation_playing {
            self.animation_started =
                Instant::now() - std::time::Duration::from_secs_f32(self.animation_elapsed);
        }
        self.model_player
            .set_playing(&mut self.ui, self.animation_playing);
    }

    pub(super) fn next_animation(&mut self) {
        if self.animation_clips.is_empty() {
            return;
        }
        self.animation_index = (self.animation_index + 1) % self.animation_clips.len();
        self.animation_elapsed = 0.0;
        self.animation_playing = false;
        self.refresh_animation_label();
    }

    pub(super) fn previous_animation(&mut self) {
        if self.animation_clips.is_empty() {
            return;
        }
        self.animation_index =
            (self.animation_index + self.animation_clips.len() - 1) % self.animation_clips.len();
        self.animation_elapsed = 0.0;
        self.animation_playing = false;
        self.refresh_animation_label();
    }

    pub(super) fn seek_animation(&mut self, seconds: f32) {
        self.animation_elapsed =
            (self.animation_elapsed + seconds).clamp(0.0, self.animation_duration());
        if self.animation_playing {
            self.animation_started =
                Instant::now() - std::time::Duration::from_secs_f32(self.animation_elapsed);
        }
        self.refresh_animation_label();
    }

    pub(super) fn seek_animation_to(&mut self, seconds: f32) {
        self.animation_elapsed = seconds.clamp(0.0, self.animation_duration());
        if self.animation_playing {
            self.animation_started =
                Instant::now() - std::time::Duration::from_secs_f32(self.animation_elapsed);
        }
        self.refresh_animation_label();
    }
}
