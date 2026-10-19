//! Engine math helpers (`Runtime-steam-x64.dll` `Math::Scalar`, `Math::Vector2/3`) with the
//! original's precision: the C runtime's `sinf`/`cosf`/`atan2f` ([`crate::crt`]), `sqrtf`
//! (correctly rounded), and the operations in the original's order.

// DAT_1801647ec
/// Degrees to radians as an f32.
pub const DEG_TO_RAD: f32 = 0.017453292;
// DAT_180164878
/// Radians to degrees.
pub const RAD_TO_DEG: f32 = 57.29578;

/// The sine of `x` (radians), with the C runtime's rounding ([`crate::crt::sinf`]).
///
/// Original: `FUN_140137160`.
pub fn sinf(x: f32) -> f32 {
    crate::crt::sinf(x)
}

/// The cosine of `x` (radians), with the C runtime's rounding ([`crate::crt::cosf`]).
///
/// Original: `FUN_140136560`.
pub fn cosf(x: f32) -> f32 {
    crate::crt::cosf(x)
}

/// The length of a 3D vector: `sqrtf((x·x + y·y) + z·z)`, added in that order.
///
/// Original: `Math::Vector3::Length`, Runtime `0x180022280`.
pub fn length3(v: [f32; 3]) -> f32 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

/// A 3D vector scaled to length 1 (times `1 / length`), and its length. A zero vector is returned
/// as it is.
///
/// Original: `Math::Vector3::Normalize`, Runtime `0x180022300`.
pub fn normalize3_len(v: [f32; 3]) -> ([f32; 3], f32) {
    let l = length3(v);
    if l == 0.0 {
        return (v, l);
    }
    let inv = 1.0 / l;
    ([v[0] * inv, v[1] * inv, v[2] * inv], l)
}

/// A 3D vector scaled to length 1. A zero vector is returned as it is.
pub fn normalize3(v: [f32; 3]) -> [f32; 3] {
    normalize3_len(v).0
}

/// A 2D vector scaled to length 1. A zero vector is returned as it is.
///
/// Original: `Math::Vector2::Normalize`, Runtime `0x18006d0d0`.
pub fn normalize2(v: [f32; 2]) -> [f32; 2] {
    let l = (v[0] * v[0] + v[1] * v[1]).sqrt();
    if l == 0.0 {
        return v;
    }
    let inv = 1.0 / l;
    [v[0] * inv, v[1] * inv]
}

/// The direction of a yaw and a pitch, in degrees: `(−cos p · sin y, cos p · cos y, sin p)`.
///
/// Original: `Math::Vector3::FromYawPitch`, Runtime `0x180022990`.
pub fn from_yaw_pitch(yaw: f32, pitch: f32) -> [f32; 3] {
    let (ry, rp) = (yaw * DEG_TO_RAD, pitch * DEG_TO_RAD);
    let (sy, sp, cy, cp) = (sinf(ry), sinf(rp), cosf(ry), cosf(rp));
    [-cp * sy, cp * cy, sp]
}

/// The heading of a 2D direction, in degrees: `atan2f(−x, y)`.
///
/// Original: `Math::Vector3::Heading`, Runtime `0x18007d3d0`.
pub fn heading(x: f32, y: f32) -> f32 {
    crate::crt::atan2f(-x, y) * RAD_TO_DEG
}

/// The sine and cosine of an angle in degrees.
///
/// Original: `Math::Scalar::SinCos`.
pub fn sin_cos_deg(degrees: f32) -> (f32, f32) {
    let r = degrees * DEG_TO_RAD;
    (sinf(r), cosf(r))
}

// The entity transform: ModelComponent +0x148 (rotation), +0x158 (offset).
/// The engine's transform: a rotation (a quaternion) and an offset. A placed model keeps its
/// position this way.
///
/// Original: `Math::Transform`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transform {
    /// The rotation, a unit quaternion `(x, y, z, w)`.
    pub q: [f32; 4],
    /// The offset, applied after the rotation.
    pub p: [f32; 3],
}

impl Default for Transform {
    /// The identity.
    fn default() -> Transform {
        Transform {
            q: [0.0, 0.0, 0.0, 1.0],
            p: [0.0; 3],
        }
    }
}

/// The product of two quaternions, its terms in the original's order.
///
/// Original: Runtime `0x18001eb60`.
fn qmul(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    let [ax, ay, az, aw] = a;
    let [bx, by, bz, bw] = b;
    [
        ((ax * bw + bx * aw) + az * by) - ay * bz,
        ((ay * bw - az * bx) + by * aw) + bz * ax,
        ((az * bw + ay * bx) - by * ax) + bz * aw,
        ((bw * aw - ax * bx) - ay * by) - bz * az,
    ]
}

impl Transform {
    /// A transform from a yaw, a pitch (degrees) and an offset. The rotation uses the sines and
    /// cosines of the half angles: `q = (−cy·sp, −sy·sp, −cp·sy, cp·cy)`.
    ///
    /// Original: `Math::Transform::FromYawPitchOffset`, Runtime `0x18007e540`.
    pub fn from_yaw_pitch_offset(yaw: f32, pitch: f32, offset: [f32; 3]) -> Transform {
        let hy = (yaw * 0.5) * DEG_TO_RAD;
        let hp = (pitch * 0.5) * DEG_TO_RAD;
        let (sy, sp, cy, cp) = (sinf(hy), sinf(hp), cosf(hy), cosf(hp));
        Transform {
            q: [-(cy * sp), -(sy * sp), -(cp * sy), cp * cy],
            p: offset,
        }
    }

    /// The rotation a CharacterComponent's update writes: `(+0, +0, −sinf(h), cosf(h))` with `h =
    /// (yaw × DEG_TO_RAD) × 0.5`, rounded in that order (a model's `FromYawPitchOffset` halves
    /// first, and its zeros are negative).
    ///
    /// Original: `FUN_14010e990`.
    pub fn character_rotation(yaw: f32) -> [f32; 4] {
        let h = (yaw * DEG_TO_RAD) * 0.5;
        [0.0, 0.0, -sinf(h), cosf(h)]
    }

    /// A placed entity's model transform: its yaw, pitch 0, its position.
    pub fn of_model(yaw: f32, position: [f32; 3]) -> Transform {
        Transform::from_yaw_pitch_offset(yaw, 0.0, position)
    }

    /// `v` turned by the rotation: `q · (v, 0) · q̄`.
    ///
    /// Original: `Math::Quaternion::Rotate`, Runtime `0x18001ea20`.
    pub fn rotate(&self, v: [f32; 3]) -> [f32; 3] {
        let q = self.q;
        let conj = [-q[0], -q[1], -q[2], q[3]];
        let t = qmul(q, [v[0], v[1], v[2], 0.0]);
        let r = qmul(t, conj);
        [r[0], r[1], r[2]]
    }

    /// A point moved by the transform: rotated, then offset.
    ///
    /// Original: `Math::Transform::operator*(Vector3)`, Runtime `0x18001eae0`.
    pub fn apply(&self, v: [f32; 3]) -> [f32; 3] {
        let r = self.rotate(v);
        [r[0] + self.p[0], self.p[1] + r[1], r[2] + self.p[2]]
    }

    /// The inverse transform: the conjugate rotation, and the negated offset turned by it.
    ///
    /// Original: `Math::Transform::Inverse`, Runtime `0x18005e620`.
    pub fn inverse(&self) -> Transform {
        let q = self.q;
        let conj = Transform {
            q: [-q[0], -q[1], -q[2], q[3]],
            p: [0.0; 3],
        };
        Transform {
            q: conj.q,
            p: conj.rotate([-self.p[0], -self.p[1], -self.p[2]]),
        }
    }

    /// A heading in the model's own space, turned into the world: the heading of
    /// `from_yaw_pitch(local_yaw, 0)` after the rotation.
    ///
    /// Original: `FUN_140128d50`.
    pub fn heading_of(&self, local_yaw: f32) -> f32 {
        let v = self.rotate(from_yaw_pitch(local_yaw, 0.0));
        heading(v[0], v[1])
    }
}
