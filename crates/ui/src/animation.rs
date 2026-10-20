//! The interface's own clock: animated images, the containers' wheel and the edit caret.

use super::*;

impl Ui {
    /// True if the current tree contains an animated image with frames and a positive frame rate.
    pub fn has_animations(&self) -> bool {
        self.nodes.iter().any(|node| {
            matches!(
                &node.kind,
                Kind::AnimatedImage {
                    sprites,
                    frames_per_second,
                    ..
                } if sprites.len() > 1 && *frames_per_second > 0
            )
        })
    }

    /// Select each animated image's frame at the supplied elapsed time in seconds.
    /// The game's UI step: each element's own update (`AnimatedImageElement::Update`, the
    /// containers' wheel) by `dt` seconds.
    pub fn update(&mut self, dt: f32) {
        self.update_containers();
        self.update_caret(dt);
        for node in &mut self.nodes {
            if let Kind::AnimatedImage {
                sprites,
                frames_per_second,
                frame,
                clock,
                ..
            } = &mut node.kind
            {
                // `AnimatedImageElement::Update`: one loop lasts frames / rate seconds; past
                // it the clock goes back by one loop, and the frame is its share of the loop.
                if sprites.is_empty() || *frames_per_second <= 0 {
                    continue;
                }
                *clock += dt as f64;
                let length = sprites.len() as f32 / *frames_per_second as f32;
                let mut t = *clock as f32;
                if length <= t {
                    t -= length;
                    *clock = t as f64;
                }
                *frame = ((t / length * sprites.len() as f32) as usize).min(sprites.len() - 1);
            }
        }
    }

    /// The viewer's preview: every animation at `elapsed_seconds`.
    pub fn update_animations(&mut self, elapsed_seconds: f32) {
        for node in &mut self.nodes {
            if let Kind::AnimatedImage {
                sprites,
                frames_per_second,
                frame,
                ..
            } = &mut node.kind
            {
                if !sprites.is_empty() && *frames_per_second > 0 {
                    *frame = ((elapsed_seconds.max(0.0) * *frames_per_second as f32).floor()
                        as usize)
                        % sprites.len();
                }
            }
        }
    }

    /// Current frame, frame count, and frame rate for the first animated image in the tree.
    pub fn animation_info(&self) -> Option<(usize, usize, i32)> {
        self.nodes.iter().find_map(|node| match &node.kind {
            Kind::AnimatedImage {
                sprites,
                frames_per_second,
                frame,
                ..
            } if !sprites.is_empty() => Some((*frame, sprites.len(), *frames_per_second)),
            _ => None,
        })
    }

    /// Select the same frame in every animated image, wrapping each image's own frame count.
    pub fn set_animation_frame(&mut self, frame: usize) {
        for node in &mut self.nodes {
            if let Kind::AnimatedImage {
                sprites,
                frame: current,
                ..
            } = &mut node.kind
            {
                if !sprites.is_empty() {
                    *current = frame % sprites.len();
                }
            }
        }
    }
}
