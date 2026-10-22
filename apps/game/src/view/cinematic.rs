//! A camera for videos made with headless captures: keyframed paths (`--camera`) and a camera
//! that rides with a citizen, a trader boat or an animal (`--camera-follow`).
//!
//! It changes only what is drawn. The game camera, the simulation and saves stay as they are:
//! [`Game::draw`](crate::Game::draw) builds the frame from this camera instead of the game's
//! ([`SceneView::frame_from`](crate::view::scene::SceneView::frame_from)), and culling, shadows,
//! the sky and the weather follow the frame. Times are capture seconds, as for `--script`.
//!
//! Deviation: not in the original; a capture tool for documentation videos.

mod clearance;
mod keyframes;
use clearance::lift;
pub use clearance::Clearance;
#[cfg(test)]
use clearance::{box_blocks, Obstacle, LIFT_STEP};
pub use keyframes::{path, Keyframe};

use std::collections::HashMap;

use glam::{Mat4, Vec3};
use unbanished_formats::pkg::Archive;

use crate::gfx::models::placement_matrix;
use crate::loops::{GameLoop, View};
use crate::start::StartingTown;

/// Who the camera rides with.
#[derive(Clone, Debug, PartialEq)]
pub enum Subject {
    /// The N-th living citizen (as `select citizen N`).
    Citizen(usize),
    /// The N-th living citizen whose profession's name contains the text (`fish`, `hunt`).
    Profession(String, usize),
    /// The N-th trader boat on the map.
    Boat(usize),
    /// The N-th animal, of those whose template's name contains the text if one is given
    /// (`deer`, `cow`).
    Animal(Option<String>, usize),
}

impl Subject {
    /// `citizen N`, `profession NAME N`, `boat N`, `animal N` or `animal NAME N`.
    pub fn parse(text: &str) -> Option<Subject> {
        let words: Vec<&str> = text.split_whitespace().collect();
        let n = |w: Option<&&str>| w.map_or(Some(0), |w| w.parse().ok());
        Some(match words.first().copied()? {
            "citizen" => Subject::Citizen(n(words.get(1))?),
            "profession" => Subject::Profession(words.get(1)?.to_lowercase(), n(words.get(2))?),
            "boat" => Subject::Boat(n(words.get(1))?),
            "animal" => match words.get(1).map(|w| w.parse::<usize>()) {
                Some(Ok(i)) => Subject::Animal(None, i),
                Some(Err(_)) => Subject::Animal(Some(words[1].to_lowercase()), n(words.get(2))?),
                None => Subject::Animal(None, 0),
            },
            _ => return None,
        })
    }

    /// Finds the subject in the town: a key that stays the same while it lives (a citizen's
    /// index, a boat's or an animal's entity).
    fn find(&self, town: &StartingTown) -> Option<usize> {
        match self {
            Subject::Citizen(n) => (0..town.citizens.len())
                .filter(|&i| town.citizens[i].alive)
                .nth(*n),
            Subject::Profession(name, n) => {
                let list = &town.construction.professions.list;
                (0..town.citizens.len())
                    .filter(|&i| town.citizens[i].alive)
                    .filter(|&i| {
                        town.agents.states.get(i).is_some_and(|s| {
                            list.get(s.profession)
                                .is_some_and(|p| p.name.to_lowercase().contains(name.as_str()))
                        })
                    })
                    .nth(*n)
            }
            Subject::Boat(n) => town.wildlife.boats.keys().nth(*n).copied(),
            Subject::Animal(name, n) => town
                .wildlife
                .animals
                .iter()
                .filter(|(_, a)| {
                    name.as_ref().is_none_or(|name| {
                        town.templates
                            .get(a.template)
                            .is_some_and(|t| t.reference.to_lowercase().contains(name.as_str()))
                    })
                })
                .map(|(&e, _)| e)
                .nth(*n),
        }
    }

    /// The subject's position and the heading it faces, in degrees (0 faces +y), while it is
    /// on the map.
    fn pose(&self, town: &StartingTown, key: usize) -> Option<(Vec3, f32)> {
        let character = match self {
            Subject::Citizen(_) | Subject::Profession(..) => {
                if !town.citizens.get(key)?.alive {
                    return None;
                }
                town.agents.characters.get(key)?
            }
            Subject::Boat(_) => &town.wildlife.boats.get(&key)?.character,
            Subject::Animal(..) => &town.wildlife.animals.get(&key)?.character,
        };
        Some((
            Vec3::from(character.position),
            character.yaw - character.desc.yaw_offset,
        ))
    }
}

/// Where the camera sits for a followed subject, in the subject's own frame (units are map
/// cells; a citizen is about 1.8 tall).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rig {
    /// Behind the subject.
    pub back: f32,
    /// Above its feet.
    pub up: f32,
    /// To its right (negative: left).
    pub side: f32,
    /// How far ahead of the subject the camera looks.
    pub ahead: f32,
    /// The height of that point.
    pub look_up: f32,
}

impl Default for Rig {
    /// Over a citizen's right shoulder.
    fn default() -> Rig {
        Rig {
            back: 4.0,
            up: 2.6,
            side: 0.8,
            ahead: 8.0,
            look_up: 1.0,
        }
    }
}

impl Rig {
    /// `BACK,UP[,SIDE[,AHEAD[,LOOK_UP]]]`; missing numbers keep the defaults.
    pub fn parse(text: &str) -> Option<Rig> {
        let mut rig = Rig::default();
        let fields = [
            &mut rig.back,
            &mut rig.up,
            &mut rig.side,
            &mut rig.ahead,
            &mut rig.look_up,
        ];
        let words: Vec<&str> = text.split(',').collect();
        if words.len() > fields.len() {
            return None;
        }
        for (field, word) in fields.into_iter().zip(words) {
            *field = word.trim().parse().ok()?;
        }
        Some(rig)
    }

    /// The eye and target for a subject at `position` facing `heading` degrees.
    fn place(&self, position: Vec3, heading: f32) -> (Vec3, Vec3) {
        let (s, c) = heading.to_radians().sin_cos();
        let forward = Vec3::new(-s, c, 0.0);
        let right = Vec3::new(c, s, 0.0);
        (
            position - forward * self.back + right * self.side + Vec3::Z * self.up,
            position + forward * self.ahead + Vec3::Z * self.look_up,
        )
    }
}

/// A camera riding with one subject from `start` to `end` seconds.
#[derive(Clone, Debug)]
pub struct Follow {
    pub subject: Subject,
    pub start: f32,
    pub end: f32,
    /// The subject once found.
    key: Option<usize>,
    /// The smoothed position and heading.
    smoothed: Option<(Vec3, f32)>,
    /// How far the camera has risen to keep the subject in view (smoothed).
    lift: f32,
    /// The part of the way from the subject to the eye that the camera keeps (smoothed).
    pull: f32,
}

impl Follow {
    /// `SUBJECT@START[-END]`.
    pub fn parse(text: &str) -> Option<Follow> {
        let (subject, span) = text.rsplit_once('@')?;
        let (start, end) = match span.split_once('-') {
            Some((a, b)) => (a.trim().parse().ok()?, b.trim().parse().ok()?),
            None => (span.trim().parse().ok()?, f32::INFINITY),
        };
        Some(Follow {
            subject: Subject::parse(subject)?,
            start,
            end,
            key: None,
            smoothed: None,
            lift: 0.0,
            pull: 1.0,
        })
    }
}

/// The capture's cinematic camera: a path, followed subjects, the field of view, and how
/// smoothly the camera follows.
pub struct Cinematic {
    pub keys: Vec<Keyframe>,
    pub follows: Vec<Follow>,
    pub rig: Rig,
    /// Vertical field of view in degrees (the game's is 40).
    pub fov: f32,
    /// Time constant of the heading's smoothing, in seconds; the position's is a quarter.
    pub smooth: f32,
    /// Rise over what stands between a followed subject and the camera (`--camera-clear 0`
    /// turns it off).
    pub clear: bool,
    clearance: Clearance,
}

impl Cinematic {
    /// The camera options of a capture's command line, or none if it has none:
    /// `--camera KEYFRAME` (repeated), `--camera-follow SUBJECT@START[-END]` (repeated),
    /// `--camera-rig BACK,UP[,SIDE[,AHEAD[,LOOK_UP]]]`, `--camera-fov DEGREES`,
    /// `--camera-smooth SECONDS`.
    pub fn from_args(args: &[String]) -> Option<Cinematic> {
        let values = |flag: &str| {
            args.windows(2)
                .filter(move |w| w[0] == flag)
                .map(|w| w[1].clone())
                .collect::<Vec<_>>()
        };
        let mut keys: Vec<Keyframe> = values("--camera")
            .iter()
            .filter_map(|v| {
                let k = Keyframe::parse(v);
                if k.is_none() {
                    log::warn!("--camera {v}: expected X,Y,Z>X,Y,Z@SECONDS");
                }
                k
            })
            .collect();
        keys.sort_by(|a, b| a.at.total_cmp(&b.at));
        let follows: Vec<Follow> = values("--camera-follow")
            .iter()
            .filter_map(|v| {
                let f = Follow::parse(v);
                if f.is_none() {
                    log::warn!("--camera-follow {v}: expected SUBJECT@START[-END]");
                }
                f
            })
            .collect();
        if keys.is_empty() && follows.is_empty() {
            return None;
        }
        let last = |flag: &str| values(flag).pop();
        Some(Cinematic {
            keys,
            follows,
            rig: last("--camera-rig")
                .and_then(|v| Rig::parse(&v))
                .unwrap_or_default(),
            fov: last("--camera-fov")
                .and_then(|v| v.parse().ok())
                .unwrap_or(40.0),
            smooth: last("--camera-smooth")
                .and_then(|v| v.parse().ok())
                .unwrap_or(0.6),
            clear: last("--camera-clear").is_none_or(|v| v != "0"),
            clearance: Clearance::default(),
        })
    }

    /// Advances the camera to `t` seconds (`dt` since the last step) and returns its eye,
    /// target and field of view, or none for the game's own camera. A followed subject is
    /// looked for until it appears (a boat that has not arrived yet); the camera then stays
    /// where it was last when the subject leaves the map. A follow comes before the path.
    ///
    /// A followed camera keeps what would hide the subject out of the way: it looks at the
    /// objects around, now and a few steps ahead, moves in closer to the subject (to 40% of the
    /// way) or, if that does not clear the view, rises; quickly, and settles back slowly.
    pub fn step(
        &mut self,
        t: f32,
        dt: f32,
        current: &dyn GameLoop,
        archive: &Archive,
    ) -> Option<(Vec3, Vec3, f32)> {
        let town = current.town();
        let a_heading = 1.0 - (-dt / self.smooth.max(1e-3)).exp();
        let a_position = 1.0 - (-dt / (self.smooth * 0.25).max(1e-3)).exp();
        let rig = self.rig;
        let mut pose = None;
        for follow in self.follows.iter_mut() {
            if t < follow.start || t >= follow.end {
                continue;
            }
            if let Some(town) = town {
                if follow.key.is_none() {
                    follow.key = follow.subject.find(town);
                    if let Some(k) = follow.key {
                        log::info!(
                            "--camera-follow {:?}: found at {t:.2} s (key {k})",
                            follow.subject
                        );
                    }
                }
                let now = follow.key.and_then(|k| follow.subject.pose(town, k));
                if let Some((p, h)) = now {
                    log::debug!(
                        "--camera-follow {:?} at {t:.2} s: {p} heading {h}",
                        follow.subject
                    );
                }
                if let Some((p, h)) = now {
                    follow.smoothed = Some(match follow.smoothed {
                        None => (p, h),
                        Some((sp, sh)) => {
                            let turn = (h - sh + 540.0).rem_euclid(360.0) - 180.0;
                            (sp + (p - sp) * a_position, sh + turn * a_heading)
                        }
                    });
                }
            }
            if let Some((p, h)) = follow.smoothed {
                let (eye, target) = rig.place(p, h);
                // The subject's chest, and the way from it to the eye.
                let chest = p + Vec3::Z * 1.2;
                let boom = eye - chest;
                let (mut pull, mut rise) = (1.0f32, 0.0f32);
                if let (true, Some(town)) = (self.clear, town) {
                    let boxes = self.clearance.boxes(archive, town, p);
                    let clearance = &mut self.clearance;
                    let mut blocked =
                        |a: Vec3, b: Vec3| clearance.blocked(archive, town, &boxes, a, b);
                    // Now and a little further on: the camera moves in closer if that clears
                    // the view, else rises.
                    let ahead = Vec3::new(-h.to_radians().sin(), h.to_radians().cos(), 0.0) * 3.0;
                    for step in [Vec3::ZERO, ahead] {
                        let from = chest + step;
                        let closer = (0..=12)
                            .map(|i| 1.0 - i as f32 * 0.05)
                            .find(|&f| !blocked(from, from + boom * f));
                        match closer {
                            Some(f) => pull = pull.min(f),
                            None => {
                                rise =
                                    rise.max(lift(&mut blocked, from, from + boom).unwrap_or(0.0))
                            }
                        }
                    }
                }
                log::debug!("--camera-follow pull {pull} rise {rise} at {t:.2} s");
                let ease = |now: &mut f32, wanted: f32, fast: bool| {
                    let rate = if fast { 0.15 } else { 1.0 };
                    *now += (wanted - *now) * (1.0 - (-dt / rate).exp());
                };
                let (pulling, rising) = (pull < follow.pull, rise > follow.lift);
                ease(&mut follow.pull, pull, pulling);
                ease(&mut follow.lift, rise, rising);
                pose = Some((chest + boom * follow.pull + Vec3::Z * follow.lift, target));
                break;
            }
        }
        let (mut eye, target) = pose.or_else(|| path(&self.keys, t))?;
        // Keep a followed camera above the ground.
        if pose.is_some() {
            if let View::World(view) = current.view() {
                eye.z = eye.z.max(view.terrain.height_at(eye.x, eye.y) + 0.5);
            }
        }
        Some((eye, target, self.fov))
    }
}

#[cfg(test)]
mod tests;
