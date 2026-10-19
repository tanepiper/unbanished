//! Skeleton maths for `GraphicsMesh` bones.
//!
//! Convention (verified against all eight skinned meshes, combining weighted geometry with
//! rigid node geometry when comparing stored bounds):
//! - stored quaternions are conjugated relative to the column-vector convention used here;
//! - world = parent_world * local;
//! - skinning matrix = world * inverse_bind, where `inv_rotation`/`inv_translation` is the inverse bind.
//!
//! Skinned vertices are stored in a per-mesh frame; the bind-pose palette maps them to model space.

use crate::types::Bone;

/// A bone transform: a rotation and a translation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transform {
    /// Unit quaternion (x, y, z, w), column-vector convention.
    pub rotation: [f32; 4],
    /// The translation.
    pub translation: [f32; 3],
}

fn qmul(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    let [ax, ay, az, aw] = a;
    let [bx, by, bz, bw] = b;
    [
        aw * bx + ax * bw + ay * bz - az * by,
        aw * by - ax * bz + ay * bw + az * bx,
        aw * bz + ax * by - ay * bx + az * bw,
        aw * bw - ax * bx - ay * by - az * bz,
    ]
}

fn conj(q: [f32; 4]) -> [f32; 4] {
    [-q[0], -q[1], -q[2], q[3]]
}

fn rotate(q: [f32; 4], v: [f32; 3]) -> [f32; 3] {
    let p = qmul(qmul(q, [v[0], v[1], v[2], 0.0]), conj(q));
    [p[0], p[1], p[2]]
}

impl Transform {
    /// The transform that changes nothing.
    pub const IDENTITY: Transform = Transform {
        rotation: [0.0, 0.0, 0.0, 1.0],
        translation: [0.0; 3],
    };

    /// `self * other` (apply `other` first).
    pub fn mul(&self, other: &Transform) -> Transform {
        let t = rotate(self.rotation, other.translation);
        Transform {
            rotation: qmul(self.rotation, other.rotation),
            translation: [
                t[0] + self.translation[0],
                t[1] + self.translation[1],
                t[2] + self.translation[2],
            ],
        }
    }

    /// A point moved by the transform: rotated, then translated.
    pub fn apply(&self, v: [f32; 3]) -> [f32; 3] {
        let r = rotate(self.rotation, v);
        [
            r[0] + self.translation[0],
            r[1] + self.translation[1],
            r[2] + self.translation[2],
        ]
    }

    /// Column-major 4x4 matrix.
    pub fn to_matrix(&self) -> [f32; 16] {
        let [x, y, z, w] = self.rotation;
        let [tx, ty, tz] = self.translation;
        [
            1.0 - 2.0 * (y * y + z * z),
            2.0 * (x * y + z * w),
            2.0 * (x * z - y * w),
            0.0,
            2.0 * (x * y - z * w),
            1.0 - 2.0 * (x * x + z * z),
            2.0 * (y * z + x * w),
            0.0,
            2.0 * (x * z + y * w),
            2.0 * (y * z - x * w),
            1.0 - 2.0 * (x * x + y * y),
            0.0,
            tx,
            ty,
            tz,
            1.0,
        ]
    }
}

/// Bone-local bind transform.
pub fn local_bind(bone: &Bone) -> Transform {
    Transform {
        rotation: conj(bone.rotation),
        translation: bone.translation,
    }
}

/// The inverse of a bone's bind transform, for skinning.
pub fn inverse_bind(bone: &Bone) -> Transform {
    Transform {
        rotation: conj(bone.inv_rotation),
        translation: bone.inv_translation,
    }
}

/// World transforms for the given local transforms (bones are stored parents-first).
pub fn world_transforms(bones: &[Bone], locals: &[Transform]) -> Vec<Transform> {
    let mut world: Vec<Transform> = Vec::with_capacity(bones.len());
    for (bone, local) in bones.iter().zip(locals) {
        let w = match usize::try_from(bone.parent).ok().and_then(|p| world.get(p)) {
            Some(parent) => parent.mul(local),
            None => *local,
        };
        world.push(w);
    }
    world
}

/// Skinning palette (world * inverse bind) for the given local pose.
pub fn palette(bones: &[Bone], locals: &[Transform]) -> Vec<Transform> {
    world_transforms(bones, locals)
        .iter()
        .zip(bones)
        .map(|(w, b)| w.mul(&inverse_bind(b)))
        .collect()
}

/// Palette for the bind pose.
pub fn bind_palette(bones: &[Bone]) -> Vec<Transform> {
    let locals: Vec<Transform> = bones.iter().map(local_bind).collect();
    palette(bones, &locals)
}

/// Skin 32-byte `ModelSkinLayout` vertices (i16x4 position /512, i16x4 texcoords, unorm8x4
/// normal, 4 unused bytes, unorm8x4 weights, u8x4 bone indices) with a palette into the 24-byte
/// static `ModelLayout` (position, texcoords, normal, zeroed user data). Used to draw characters
/// in a fixed pose without GPU skinning.
pub fn skin_to_static(vertices: &[u8], palette: &[Transform]) -> Vec<u8> {
    use crate::types::{SKINNED_STRIDE, STATIC_STRIDE};
    let mut out = Vec::with_capacity(vertices.len() / SKINNED_STRIDE * STATIC_STRIDE);
    for v in vertices.chunks_exact(SKINNED_STRIDE) {
        let i16_at = |o: usize| i16::from_le_bytes([v[o], v[o + 1]]) as f32;
        let p = [i16_at(0) / 512.0, i16_at(2) / 512.0, i16_at(4) / 512.0];
        let n = [
            v[16] as f32 / 127.5 - 1.0,
            v[17] as f32 / 127.5 - 1.0,
            v[18] as f32 / 127.5 - 1.0,
        ];
        let (mut sp, mut sn) = ([0.0f32; 3], [0.0f32; 3]);
        for k in 0..4 {
            let w = v[24 + k] as f32 / 255.0;
            if w == 0.0 {
                continue;
            }
            let t = palette
                .get(v[28 + k] as usize)
                .copied()
                .unwrap_or(Transform::IDENTITY);
            let q = t.apply(p);
            let r = t.apply([0.0; 3]);
            let m = t.apply(n);
            for a in 0..3 {
                sp[a] += q[a] * w;
                sn[a] += (m[a] - r[a]) * w;
            }
        }
        let len = (sn[0] * sn[0] + sn[1] * sn[1] + sn[2] * sn[2])
            .sqrt()
            .max(1e-6);
        for value in sp {
            out.extend_from_slice(&((value * 512.0).round() as i16).to_le_bytes());
        }
        out.extend_from_slice(&v[6..16]);
        for value in sn {
            out.push(((value / len * 0.5 + 0.5) * 255.0).round() as u8);
        }
        out.push(v[19]);
        out.extend_from_slice(&[0; 4]);
    }
    out
}
