//! Choosing a mode and an item: loading the model, texture, sound or dialog into the preview, and
//! stepping through the list.

use super::*;

impl Tool {
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
        self.ui.set_visible(self.model_player.window, false);
        self.ui.set_visible(self.dialog_player.window, false);
        self.ui
            .set_visible(self.info, mode != Mode::Models && mode != Mode::Dialogs);
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
        self.ui.set_visible(self.animation_next, false);
        self.ui.set_visible(self.animation_label, false);
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
        if self.mode == Mode::Models {
            self.animation_clips.clear();
            self.animation_index = 0;
            self.animation_elapsed = 0.0;
            self.animation_playing = false;
            self.ui.set_visible(self.model_player.window, false);
            self.model_player.set_playing(&mut self.ui, false);
        }
        if self.mode == Mode::Dialogs {
            self.dialog_animation_playing = false;
            self.dialog_animation_elapsed = 0.0;
            self.ui.set_visible(self.dialog_player.window, false);
            self.dialog_player.set_frame(&mut self.ui, 0, 0, 0, false);
        }
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
                    self.animation_index = 0;
                    self.ui.set_visible(self.audio_controls, false);
                    self.ui.set_visible(self.model_player.window, true);
                    self.ui.set_text(self.play, "Play");
                    self.refresh_animation_label();
                    self.model_player.set_position(&mut self.ui, 0.0);
                    self.model_player.set_playing(&mut self.ui, false);
                    self.mesh = Some(mesh);
                    self.model_player.set_model_info(&mut self.ui, &format!(
                        "{shown}\n{parts} part(s)   drag to orbit, wheel to zoom\n{} animation clip(s)",
                        self.animation_clips.len()
                    ));
                    self.set_info("");
                }
                Err(e) => {
                    self.ui.set_visible(self.model_player.window, true);
                    self.model_player.set_clip(&mut self.ui, "", 0, 0, 0.0);
                    self.model_player
                        .set_model_info(&mut self.ui, &format!("{shown}\n{e}"));
                    self.set_info("");
                }
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
                        let view = unbanished_gfx::upload_rgba(
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
                        self.dialog_player.set_info(&mut self.ui, &reference);
                        self.dialog = Some(d);
                        self.dialog_animation_started = Instant::now();
                        self.dialog_animation_elapsed = 0.0;
                        self.dialog_animation_playing = self.ui.has_animations();
                        self.ui.set_visible(self.dialog_player.window, true);
                        self.refresh_dialog_animation_label();
                    }
                    None => {
                        self.dialog_player
                            .set_info(&mut self.ui, &format!("{reference}\nUnable to load dialog"));
                        self.dialog_player.set_frame(&mut self.ui, 0, 0, 0, false);
                        self.ui.set_visible(self.dialog_player.window, true);
                    }
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

    pub(super) fn step_audio(&mut self, renderer: &mut Renderer, delta: isize) {
        let was_playing = self.player.is_playing();
        self.step(renderer, delta);
        if was_playing {
            self.play_selected();
        }
    }

    pub(super) fn play_selected(&mut self) {
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
                let pcm = unbanished_audio::decode(&sound, stream.as_deref())?;
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
}
