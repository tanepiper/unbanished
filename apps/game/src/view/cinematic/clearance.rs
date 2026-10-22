//! Keeping the camera clear of what would hide its subject.

use super::*;

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
pub(super) const NEAR_OBJECTS: f32 = 40.0;

/// The most the camera rises, and the step it rises by.
pub(super) const MOST_LIFT: f32 = 8.0;

pub(super) const LIFT_STEP: f32 = 0.25;

/// An object's box: its mesh bounds, and the transform from the world into its frame.
#[derive(Clone, Copy, Debug)]
pub(super) struct Obstacle {
    /// The placed object.
    pub(super) placed: usize,
    pub(super) to_local: Mat4,
    pub(super) lo: Vec3,
    pub(super) hi: Vec3,
}

/// Whether the segment from `a` to `b` passes through the box `lo`..`hi` (the slab test).
pub(super) fn crosses(a: Vec3, b: Vec3, lo: Vec3, hi: Vec3) -> bool {
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

pub(super) fn inside(p: Vec3, lo: Vec3, hi: Vec3) -> bool {
    p.cmpge(lo).all() && p.cmple(hi).all()
}

/// Whether a box blocks the line of sight from `subject` to `eye`. A large box that holds the
/// subject is open ground it walks in (a market): it does not count. A small one (a house the
/// subject brushes past, a dock) only keeps the eye out of it.
pub(super) fn box_blocks(b: &Obstacle, subject: Vec3, eye: Vec3) -> bool {
    match b.holds(subject) {
        true => !b.open() && b.holds(eye),
        false => b.crosses(subject, eye),
    }
}

/// How far the eye must rise for nothing to stand between it and `subject`. None when no rise
/// up to [`MOST_LIFT`] clears the view (a citizen under a tree's crown).
pub(super) fn lift(
    mut blocked: impl FnMut(Vec3, Vec3) -> bool,
    subject: Vec3,
    eye: Vec3,
) -> Option<f32> {
    let mut lift = 0.0;
    while lift <= MOST_LIFT {
        if !blocked(subject, eye + Vec3::Z * lift) {
            return Some(lift);
        }
        lift += LIFT_STEP;
    }
    None
}

impl Clearance {
    pub(super) fn mesh_bounds(&mut self, archive: &Archive, mesh: &str) -> Option<(Vec3, Vec3)> {
        *self.bounds.entry(mesh.to_string()).or_insert_with(|| {
            let bytes = archive
                .read_ref(&crate::start::normalise(mesh).replace('\\', "/"))
                .ok()?;
            let m = unbanished_formats::types::graphics_mesh(&bytes).ok()?;
            Some((Vec3::from(m.bounds_min), Vec3::from(m.bounds_max)))
        })
    }

    /// The boxes of the placed objects with a model within [`NEAR_OBJECTS`] of `centre`, each
    /// in its object's own frame.
    pub(super) fn boxes(
        &mut self,
        archive: &Archive,
        town: &StartingTown,
        centre: Vec3,
    ) -> Vec<Obstacle> {
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
    pub(super) fn blocked(
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

impl Obstacle {
    pub(super) fn holds(&self, p: Vec3) -> bool {
        inside(self.to_local.transform_point3(p), self.lo, self.hi)
    }

    /// Large enough to walk in (more than 10 × 10 cells).
    pub(super) fn open(&self) -> bool {
        let size = self.hi - self.lo;
        size.x * size.y > 100.0
    }

    pub(super) fn crosses(&self, a: Vec3, b: Vec3) -> bool {
        let m = self.to_local;
        crosses(
            m.transform_point3(a),
            m.transform_point3(b),
            self.lo,
            self.hi,
        )
    }
}
