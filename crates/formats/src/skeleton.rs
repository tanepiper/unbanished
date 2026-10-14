//! Skeleton maths for `GraphicsMesh` bones.
//!
//! Convention (established by making every influencing bone of 7/8 skinned meshes agree, and the
//! skinned result match the stored bounds to within quantisation error):
//! - stored quaternions are conjugated relative to the column-vector convention used here;
//! - world = parent_world * local;
//! - skinning matrix = world * inverse_bind, where `inv_rotation`/`inv_translation` is the inverse bind.
//!
//! Skinned vertices are stored in a per-mesh frame; the bind-pose palette maps them to model space.

use crate::types::Bone;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transform {
    /// Unit quaternion (x, y, z, w), column-vector convention.
    pub rotation: [f32; 4],
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
