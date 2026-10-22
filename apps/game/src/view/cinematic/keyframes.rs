//! Camera paths: keyframes and the curve through them.

use super::*;

/// One point of a camera path: where the camera is, what it looks at, and when.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Keyframe {
    pub eye: Vec3,
    pub target: Vec3,
    pub at: f32,
}

pub(super) fn vector(text: &str) -> Option<Vec3> {
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

pub(super) fn catmull_rom(p0: Vec3, p1: Vec3, p2: Vec3, p3: Vec3, s: f32) -> Vec3 {
    let (s2, s3) = (s * s, s * s * s);
    0.5 * (2.0 * p1
        + (p2 - p0) * s
        + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * s2
        + (3.0 * p1 - p0 - 3.0 * p2 + p3) * s3)
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
