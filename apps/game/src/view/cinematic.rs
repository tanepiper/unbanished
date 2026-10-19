//! A camera for videos made with headless captures: keyframed paths (`--camera`) and a camera
//! that rides with a citizen, a trader boat or an animal (`--camera-follow`).
//!
//! It changes only what is drawn. The game camera, the simulation and saves stay as they are:
//! [`Game::draw`](crate::Game::draw) builds the frame from this camera instead of the game's
//! ([`SceneView::frame_from`](crate::view::scene::SceneView::frame_from)), and culling, shadows,
//! the sky and the weather follow the frame. Times are capture seconds, as for `--script`.
//!
//! Deviation: not in the original; a capture tool for documentation videos.

use std::collections::HashMap;

use banished_formats::pkg::Archive;
use glam::{Mat4, Vec3};

use crate::gfx::models::placement_matrix;
use crate::loops::{GameLoop, View};
use crate::start::StartingTown;

/// One point of a camera path: where the camera is, what it looks at, and when.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Keyframe {
    pub eye: Vec3,
    pub target: Vec3,
    pub at: f32,
}

impl Keyframe {
    /// `EYE_X,EYE_Y,EYE_Z>TARGET_X,TARGET_Y,TARGET_Z@SECONDS`.
    pub fn parse(text: &str) -> Option<Keyframe> {
        let (points, at) = text.rsplit_once('@')?;
        let (eye, target) = points.split_once('>')?;
        Some(Keyframe {
            eye: vector(eye)?,
            target: vector(target)?,
            at: at.trim().parse().ok()?,
        })
    }
}

fn vector(text: &str) -> Option<Vec3> {
    let v: Vec<f32> = text
        .split(',')
        .map(|w| w.trim().parse().ok())
        .collect::<Option<_>>()?;
    (v.len() == 3).then(|| Vec3::new(v[0], v[1], v[2]))
}

/// The camera on a path through `keys` (sorted by time) at `t`: none before the first
/// keyframe, the last one after the end. Between keyframes the eye and the target follow a
/// Catmull-Rom curve through the points, so the camera does not turn sharply at a keyframe;
/// the path starts from rest and comes to rest (ease in and out), and keeps its speed at the
/// keyframes between.
pub fn path(keys: &[Keyframe], t: f32) -> Option<(Vec3, Vec3)> {
    let first = keys.first()?;
    if t < first.at {
        return None;
    }
    let last = keys.last()?;
    if t >= last.at || keys.len() == 1 {
        return Some((last.eye, last.target));
    }
    let i = keys.windows(2).position(|w| t < w[1].at).unwrap_or(0);
    let (a, b) = (keys[i], keys[i + 1]);
    let s = ((t - a.at) / (b.at - a.at).max(1e-6)).clamp(0.0, 1.0);
    let (opens, closes) = (i == 0, i + 2 == keys.len());
    // Time shaping with a slope of 0 at the path's ends and 1 at the keyframes between.
    let s = match (opens, closes) {
        (true, true) => s * s * (3.0 - 2.0 * s),
        (true, false) => s * s * (2.0 - s),
        (false, true) => 1.0 - (1.0 - s) * (1.0 - s) * (1.0 + s),
        (false, false) => s,
    };
    let point = |f: fn(&Keyframe) -> Vec3| {
        let (p1, p2) = (f(&a), f(&b));
        let p0 = if opens {
            2.0 * p1 - p2
        } else {
            f(&keys[i - 1])
        };
        let p3 = if closes {
            2.0 * p2 - p1
        } else {
            f(&keys[i + 2])
        };
        catmull_rom(p0, p1, p2, p3, s)
    };
    Some((point(|k| k.eye), point(|k| k.target)))
}

fn catmull_rom(p0: Vec3, p1: Vec3, p2: Vec3, p3: Vec3, s: f32) -> Vec3 {
    let (s2, s3) = (s * s, s * s * s);
    0.5 * (2.0 * p1
        + (p2 - p0) * s
        + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * s2
        + (3.0 * p1 - p0 - 3.0 * p2 + p3) * s3)
}

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

/// What stands near a followed subject, so that the camera can keep a roof, a tree or a bridge
/// from coming between it and the subject. Each object is first a box (its model's bounds), then,
/// where the line of sight crosses the box, its picking mesh (the shape clicks hit).
#[derive(Default)]
pub struct Clearance {
    /// Each mesh's bounds (lowest and highest corner), read once.
    bounds: HashMap<String, Option<(Vec3, Vec3)>>,
    /// The picking meshes, read once.
    picking: crate::tools::selection::Selection,
}

/// How far around the subject objects are looked at, in map cells.
const NEAR_OBJECTS: f32 = 40.0;
/// The most the camera rises, and the step it rises by.
const MOST_LIFT: f32 = 8.0;
const LIFT_STEP: f32 = 0.25;

impl Clearance {
    fn mesh_bounds(&mut self, archive: &Archive, mesh: &str) -> Option<(Vec3, Vec3)> {
        *self.bounds.entry(mesh.to_string()).or_insert_with(|| {
            let bytes = archive
                .read_ref(&crate::start::normalise(mesh).replace('\\', "/"))
                .ok()?;
            let m = banished_formats::types::graphics_mesh(&bytes).ok()?;
            Some((Vec3::from(m.bounds_min), Vec3::from(m.bounds_max)))
        })
    }

    /// The boxes of the placed objects with a model within [`NEAR_OBJECTS`] of `centre`, each
    /// in its object's own frame.
    fn boxes(&mut self, archive: &Archive, town: &StartingTown, centre: Vec3) -> Vec<Obstacle> {
        let mut out = Vec::new();
        for (i, p) in town.placed.iter().enumerate().filter(|(_, p)| p.alive) {
            let at = Vec3::from(p.position);
            if (at.truncate() - centre.truncate()).length() > NEAR_OBJECTS {
                continue;
            }
            let Some(mesh) = town.templates[p.template]
                .model
                .as_ref()
                .and_then(|m| m.variants.get(p.variant))
                .and_then(|v| v.meshes.get(p.mesh))
                .and_then(|m| m.as_deref())
            else {
                continue;
            };
            // Objects drawn at no size (not grown yet) have no box.
            if p.scale[..3].iter().any(|&s| s.abs() < 1e-3) {
                continue;
            }
            let Some((lo, hi)) = self.mesh_bounds(archive, mesh) else {
                continue;
            };
            let to_local = Mat4::from_cols_array(&placement_matrix(
                p.position,
                [p.scale[0], p.scale[1], p.scale[2]],
                p.yaw,
            ))
            .inverse();
            out.push(Obstacle {
                placed: i,
                to_local,
                lo,
                hi,
            });
        }
        out
    }
}

impl Clearance {
    /// Whether something stands between `subject` and `eye`: an object whose picking mesh the
    /// line crosses, or, for one without a picking mesh, whose box blocks it ([`box_blocks`]).
    fn blocked(
        &mut self,
        archive: &Archive,
        town: &StartingTown,
        boxes: &[Obstacle],
        subject: Vec3,
        eye: Vec3,
    ) -> bool {
        boxes.iter().any(|b| {
            if !b.crosses(subject, eye) {
                return false;
            }
            self.picking
                .blocks(archive, town, b.placed, subject, eye)
                .unwrap_or_else(|| box_blocks(b, subject, eye))
        })
    }
}

/// An object's box: its mesh bounds, and the transform from the world into its frame.
#[derive(Clone, Copy, Debug)]
struct Obstacle {
    /// The placed object.
    placed: usize,
    to_local: Mat4,
    lo: Vec3,
    hi: Vec3,
}

impl Obstacle {
    fn holds(&self, p: Vec3) -> bool {
        inside(self.to_local.transform_point3(p), self.lo, self.hi)
    }

    /// Large enough to walk in (more than 10 × 10 cells).
    fn open(&self) -> bool {
        let size = self.hi - self.lo;
        size.x * size.y > 100.0
    }

    fn crosses(&self, a: Vec3, b: Vec3) -> bool {
        let m = self.to_local;
        crosses(
            m.transform_point3(a),
            m.transform_point3(b),
            self.lo,
            self.hi,
        )
    }
}

/// Whether the segment from `a` to `b` passes through the box `lo`..`hi` (the slab test).
fn crosses(a: Vec3, b: Vec3, lo: Vec3, hi: Vec3) -> bool {
    let d = b - a;
    let (mut near, mut far) = (0.0f32, 1.0f32);
    for k in 0..3 {
        if d[k].abs() < 1e-6 {
            if a[k] < lo[k] || a[k] > hi[k] {
                return false;
            }
            continue;
        }
        let (t0, t1) = ((lo[k] - a[k]) / d[k], (hi[k] - a[k]) / d[k]);
        near = near.max(t0.min(t1));
        far = far.min(t0.max(t1));
        if near > far {
            return false;
        }
    }
    true
}

fn inside(p: Vec3, lo: Vec3, hi: Vec3) -> bool {
    p.cmpge(lo).all() && p.cmple(hi).all()
}

/// Whether a box blocks the line of sight from `subject` to `eye`. A large box that holds the
/// subject is open ground it walks in (a market): it does not count. A small one (a house the
/// subject brushes past, a dock) only keeps the eye out of it.
fn box_blocks(b: &Obstacle, subject: Vec3, eye: Vec3) -> bool {
    match b.holds(subject) {
        true => !b.open() && b.holds(eye),
        false => b.crosses(subject, eye),
    }
}

/// How far the eye must rise for nothing to stand between it and `subject`. None when no rise
/// up to [`MOST_LIFT`] clears the view (a citizen under a tree's crown).
fn lift(mut blocked: impl FnMut(Vec3, Vec3) -> bool, subject: Vec3, eye: Vec3) -> Option<f32> {
    let mut lift = 0.0;
    while lift <= MOST_LIFT {
        if !blocked(subject, eye + Vec3::Z * lift) {
            return Some(lift);
        }
        lift += LIFT_STEP;
    }
    None
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
mod tests {
    use super::*;

    fn key(x: f32, at: f32) -> Keyframe {
        Keyframe {
            eye: Vec3::new(x, 0.0, 10.0),
            target: Vec3::new(x, 10.0, 0.0),
            at,
        }
    }

    #[test]
    fn parses_the_options() {
        assert_eq!(
            Keyframe::parse("1,2,3>4,5,6@7.5"),
            Some(Keyframe {
                eye: Vec3::new(1.0, 2.0, 3.0),
                target: Vec3::new(4.0, 5.0, 6.0),
                at: 7.5,
            })
        );
        assert_eq!(Keyframe::parse("1,2>4,5,6@7"), None);
        assert_eq!(
            Subject::parse("profession Fish 2"),
            Some(Subject::Profession("fish".into(), 2))
        );
        assert_eq!(
            Subject::parse("animal deer"),
            Some(Subject::Animal(Some("deer".into()), 0))
        );
        assert_eq!(Subject::parse("animal 3"), Some(Subject::Animal(None, 3)));
        assert_eq!(Subject::parse("boat 1"), Some(Subject::Boat(1)));
        let f = Follow::parse("boat 0@12-30").unwrap();
        assert_eq!((f.start, f.end), (12.0, 30.0));
        assert_eq!(Follow::parse("citizen 4@3").unwrap().end, f32::INFINITY);
        let rig = Rig::parse("10,6").unwrap();
        assert_eq!(
            (rig.back, rig.up, rig.side),
            (10.0, 6.0, Rig::default().side)
        );
        assert!(Rig::parse("1,2,3,4,5,6").is_none());
    }

    #[test]
    fn a_path_passes_through_its_keyframes_and_rests_at_its_ends() {
        let keys = [key(0.0, 1.0), key(10.0, 2.0), key(30.0, 4.0)];
        assert_eq!(path(&keys, 0.5), None);
        for k in &keys {
            let (eye, target) = path(&keys, k.at).unwrap();
            assert!((eye - k.eye).length() < 1e-4 && (target - k.target).length() < 1e-4);
        }
        assert_eq!(path(&keys, 9.0).unwrap().0, keys[2].eye);
        // Slow at the start and the end, moving at the keyframe between.
        let x = |t: f32| path(&keys, t).unwrap().0.x;
        assert!(x(1.01) - x(1.0) < 0.01);
        assert!(x(4.0) - x(3.99) < 0.02);
        assert!(x(2.01) - x(1.99) > 0.15);
        // Never turns back.
        let mut previous = x(1.0);
        for i in 1..=300 {
            let now = x(1.0 + i as f32 * 0.01);
            assert!(now >= previous - 1e-4, "at {i}");
            previous = now;
        }
    }

    #[test]
    fn the_camera_rises_over_what_hides_the_subject() {
        fn boxes(all: &[Obstacle]) -> impl FnMut(Vec3, Vec3) -> bool + '_ {
            move |a, e| all.iter().any(|b| box_blocks(b, a, e))
        }
        let at = |lo: Vec3, hi: Vec3| Obstacle {
            placed: 0,
            to_local: Mat4::IDENTITY,
            lo,
            hi,
        };
        let roof = at(Vec3::new(-2.0, -6.0, 0.0), Vec3::new(2.0, -2.0, 3.0));
        let subject = Vec3::new(0.0, 0.0, 1.0);
        assert!(roof.crosses(subject, Vec3::new(0.0, -8.0, 3.0)));
        assert!(!roof.crosses(subject, Vec3::new(0.0, 8.0, 3.0)));
        // From 8 behind at 3 up, the line clears the roof's edge nearest the subject (y −2, z 3)
        // from z 9.
        let l = lift(boxes(&[roof]), subject, Vec3::new(0.0, -8.0, 3.0)).unwrap();
        assert!((l - 6.0).abs() <= LIFT_STEP, "{l}");
        // Too high a wall to rise over: the camera stays.
        let wall = at(Vec3::new(-2.0, -6.0, 0.0), Vec3::new(2.0, -2.0, 30.0));
        assert_eq!(
            lift(boxes(&[wall]), subject, Vec3::new(0.0, -8.0, 3.0)),
            None
        );
        // A large box holding the subject (a market) is open ground; a small one (a house it
        // brushes past) keeps the eye out.
        let market = at(Vec3::new(-6.0, -6.0, 0.0), Vec3::new(6.0, 6.0, 7.0));
        let eye = Vec3::new(0.0, -4.0, 3.0);
        assert_eq!(lift(boxes(&[market]), subject, eye), Some(0.0));
        let house = at(Vec3::new(-3.0, -5.0, 0.0), Vec3::new(3.0, 1.0, 4.0));
        assert_eq!(lift(boxes(&[house]), subject, eye), Some(1.0 + LIFT_STEP));
        // A turned box is tested in its own frame: a long bridge at 45° misses a point its
        // axis-aligned bounds would hold.
        let bridge = Obstacle {
            placed: 0,
            to_local: Mat4::from_rotation_z(-std::f32::consts::FRAC_PI_4),
            lo: Vec3::new(-10.0, -1.0, 0.0),
            hi: Vec3::new(10.0, 1.0, 2.0),
        };
        assert!(bridge.holds(Vec3::new(5.0, 5.0, 1.0)));
        assert!(!bridge.holds(Vec3::new(5.0, -5.0, 1.0)));
    }

    #[test]
    fn the_rig_sits_behind_and_above_looking_ahead() {
        let rig = Rig {
            back: 4.0,
            up: 2.0,
            side: 1.0,
            ahead: 8.0,
            look_up: 1.0,
        };
        // Heading 0 faces +y; heading 90 faces −x.
        let (eye, target) = rig.place(Vec3::ZERO, 0.0);
        assert!((eye - Vec3::new(1.0, -4.0, 2.0)).length() < 1e-5);
        assert!((target - Vec3::new(0.0, 8.0, 1.0)).length() < 1e-5);
        let (eye, target) = rig.place(Vec3::ZERO, 90.0);
        assert!((eye - Vec3::new(4.0, 1.0, 2.0)).length() < 1e-5);
        assert!((target - Vec3::new(-8.0, 0.0, 1.0)).length() < 1e-5);
    }
}
