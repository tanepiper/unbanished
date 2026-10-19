//! Skipping the instances a pass cannot see.
//!
//! The original draws only what its draw groups' cull cameras see; this port drew every instance
//! in every pass. Each pass (the sun's shadow map, the water reflection and the scene) now tests
//! each instance's bounding sphere against that pass's view frustum and draws only those that
//! can reach it, in their original order. A sphere is padded for everything the vertex shaders
//! add (wind, leaf cards, skinned poses), so an instance is skipped only when none of its
//! triangles can touch the pass's target: the frames are identical.

use banished_formats::types::{SKINNED_STRIDE, STATIC_STRIDE};
use glam::{Mat4, Vec3, Vec4};

use super::{MaterialKind, RangeData};

/// A group's bounds in its instances' space: a sphere around `centre`, before the instance's
/// scale. Skinned meshes are posed by their bone palettes, so theirs is centred on the
/// placement and twice their bind pose's reach.
#[derive(Clone, Copy, Debug)]
pub(super) struct Bounds {
    centre: Vec3,
    radius: f32,
}

/// The positions of a range's vertices (the first three `i16` of both layouts, in 1/512 units).
fn positions(r: &RangeData) -> impl Iterator<Item = (Vec3, &[u8])> {
    let stride = if r.skinned {
        SKINNED_STRIDE
    } else {
        STATIC_STRIDE
    };
    r.vertices.chunks_exact(stride).map(|v| {
        let at = |o: usize| i16::from_le_bytes([v[o], v[o + 1]]) as f32 / 512.0;
        (Vec3::new(at(0), at(2), at(4)), v)
    })
}

/// A billboard vertex's leaf-card corner (`userData`, pushed out by `corner / 8192` times the
/// instance's scale in `vs_billboard`).
fn corner(r: &RangeData, v: &[u8]) -> f32 {
    if r.kind != MaterialKind::Billboard || r.skinned {
        return 0.0;
    }
    let at = |o: usize| i16::from_le_bytes([v[o], v[o + 1]]) as f32;
    (at(20) * at(20) + at(22) * at(22)).sqrt() / 8192.0
}

impl Bounds {
    /// The bounds of a group's ranges, or `None` without vertices.
    pub(super) fn of(ranges: &[RangeData]) -> Option<Bounds> {
        if ranges.iter().any(|r| r.skinned) {
            let reach = ranges
                .iter()
                .flat_map(|r| positions(r).map(|(p, _)| p.length()))
                .fold(None, |m: Option<f32>, d| Some(m.map_or(d, |m| m.max(d))))?;
            return Some(Bounds {
                centre: Vec3::ZERO,
                radius: reach * 2.0 + 0.5,
            });
        }
        // Rigid ranges: the points after the range's node transform, as `vs_main` places them.
        let rigid = |r: &RangeData| Mat4::from_cols_array(&r.rigid);
        let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        for r in ranges {
            let m = rigid(r);
            for (p, _) in positions(r) {
                let q = m.transform_point3(p);
                lo = lo.min(q);
                hi = hi.max(q);
            }
        }
        if lo.x > hi.x {
            return None;
        }
        let centre = (lo + hi) * 0.5;
        let mut radius = 0.0f32;
        for r in ranges {
            let m = rigid(r);
            for (p, v) in positions(r) {
                radius = radius.max((m.transform_point3(p) - centre).length() + corner(r, v));
            }
        }
        Some(Bounds { centre, radius })
    }

    /// The world sphere of one instance (`xyz` centre, `w` radius). The translation is the
    /// matrix's last column without its `w`, which the character shaders use for the outfit.
    /// The padding covers the billboards' wind (at most 0.025 × scale² along each card axis),
    /// the foliage's wind (under 0.15) and rounding.
    pub(super) fn sphere(&self, m: &[f32; 16]) -> [f32; 4] {
        let c0 = Vec3::new(m[0], m[1], m[2]);
        let c1 = Vec3::new(m[4], m[5], m[6]);
        let c2 = Vec3::new(m[8], m[9], m[10]);
        let centre = c0 * self.centre.x
            + c1 * self.centre.y
            + c2 * self.centre.z
            + Vec3::new(m[12], m[13], m[14]);
        let scale = c0.length().max(c1.length()).max(c2.length());
        let radius = (scale * self.radius + 0.05 * scale * scale + 0.25) * 1.001;
        [centre.x, centre.y, centre.z, radius]
    }
}

/// A pass's view frustum as six planes facing inwards, from its view-projection (wgpu clip
/// space: `-w ≤ x, y ≤ w`, `0 ≤ z ≤ w`).
pub(crate) struct Frustum([Vec4; 6]);

impl Frustum {
    pub(crate) fn new(view_proj: &Mat4) -> Frustum {
        let r = |i| view_proj.row(i);
        let planes = [
            r(3) + r(0),
            r(3) - r(0),
            r(3) + r(1),
            r(3) - r(1),
            r(2),
            r(3) - r(2),
        ];
        Frustum(planes.map(|p| {
            let l = p.truncate().length();
            if l > 0.0 {
                p / l
            } else {
                p
            }
        }))
    }

    /// Whether any part of the sphere can lie inside the frustum.
    pub(super) fn sees(&self, s: &[f32; 4]) -> bool {
        let c = Vec3::new(s[0], s[1], s[2]);
        self.0.iter().all(|p| p.truncate().dot(c) + p.w >= -s[3])
    }

    /// Whether any part of the box `lo..hi` can lie inside the frustum: for each plane, the
    /// box's corner furthest along the plane's normal must not be outside it.
    pub(crate) fn sees_box(&self, lo: Vec3, hi: Vec3) -> bool {
        self.0.iter().all(|p| {
            let n = p.truncate();
            let corner = Vec3::select(n.cmpge(Vec3::ZERO), hi, lo);
            n.dot(corner) + p.w >= -0.01
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A view down -z: x and y from -10 to 10, depth from 1 to 100 (wgpu clip space).
    fn view_proj() -> Mat4 {
        Mat4::from_cols_array(&[
            0.1,
            0.0,
            0.0,
            0.0, //
            0.0,
            0.1,
            0.0,
            0.0, //
            0.0,
            0.0,
            -1.0 / 99.0,
            0.0, //
            0.0,
            0.0,
            -1.0 / 99.0,
            1.0,
        ])
    }

    #[test]
    fn frustum_keeps_spheres_that_touch_it() {
        let f = Frustum::new(&view_proj());
        assert!(f.sees(&[0.0, 0.0, -10.0, 1.0]));
        assert!(!f.sees(&[0.0, 0.0, 10.0, 1.0]), "behind the camera");
        assert!(!f.sees(&[0.0, 0.0, -200.0, 1.0]), "beyond the far plane");
        assert!(!f.sees(&[100.0, 0.0, -10.0, 1.0]), "far to the side");
        // Its centre is outside, but the sphere reaches in.
        assert!(f.sees(&[0.0, 0.0, -101.0, 2.0]));
    }

    #[test]
    fn frustum_keeps_boxes_that_touch_it() {
        let f = Frustum::new(&view_proj());
        assert!(f.sees_box(Vec3::new(-1.0, -1.0, -11.0), Vec3::new(1.0, 1.0, -9.0)));
        assert!(!f.sees_box(Vec3::new(-1.0, -1.0, 9.0), Vec3::new(1.0, 1.0, 11.0)));
        // A large box around the camera reaches into the view.
        assert!(f.sees_box(Vec3::splat(-50.0), Vec3::splat(50.0)));
    }

    #[test]
    fn instance_sphere_follows_the_placement() {
        let b = Bounds {
            centre: Vec3::new(1.0, 0.0, 0.0),
            radius: 2.0,
        };
        let m = Mat4::from_scale_rotation_translation(
            Vec3::splat(3.0),
            glam::Quat::IDENTITY,
            Vec3::new(10.0, 20.0, 30.0),
        );
        let s = b.sphere(&m.to_cols_array());
        assert_eq!(&s[..3], &[13.0, 20.0, 30.0]);
        assert!(s[3] >= 6.0);
    }
}
