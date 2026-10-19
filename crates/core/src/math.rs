//! Engine math helpers (`Runtime-steam-x64.dll` `Math::Scalar`, `Math::Vector2/3`) with the
//! original's precision: the C runtime's `sinf`/`cosf`/`atan2f` ([`crate::crt`]), `sqrtf`
//! (correctly rounded), and the operations in the original's order.

/// `DAT_1801647ec`: degrees to radians as an f32.
pub const DEG_TO_RAD: f32 = 0.017453292;
/// `DAT_180164878`: radians to degrees.
pub const RAD_TO_DEG: f32 = 57.29578;

/// The C runtime's `sinf` (`FUN_140137160`, the Runtime's the same).
pub fn sinf(x: f32) -> f32 {
    crate::crt::sinf(x)
}

/// The C runtime's `cosf` (`FUN_140136560`, the Runtime's the same).
pub fn cosf(x: f32) -> f32 {
    crate::crt::cosf(x)
}

/// `Math::Vector3::Length` (Runtime `0x180022280`): `sqrtf((x·x + y·y) + z·z)`.
pub fn length3(v: [f32; 3]) -> f32 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

/// `Math::Vector3::Normalize` (Runtime `0x180022300`): times `1 / length`; a zero vector is
/// returned as it is. Also the length.
pub fn normalize3_len(v: [f32; 3]) -> ([f32; 3], f32) {
    let l = length3(v);
    if l == 0.0 {
        return (v, l);
    }
    let inv = 1.0 / l;
    ([v[0] * inv, v[1] * inv, v[2] * inv], l)
}

pub fn normalize3(v: [f32; 3]) -> [f32; 3] {
    normalize3_len(v).0
}

/// `Math::Vector2::Normalize` (Runtime `0x18006d0d0`).
pub fn normalize2(v: [f32; 2]) -> [f32; 2] {
    let l = (v[0] * v[0] + v[1] * v[1]).sqrt();
    if l == 0.0 {
        return v;
    }
    let inv = 1.0 / l;
    [v[0] * inv, v[1] * inv]
}

/// `Math::Vector3::FromYawPitch` (Runtime `0x180022990`): `(−cos p · sin y, cos p · cos y,
/// sin p)` of the angles in degrees.
pub fn from_yaw_pitch(yaw: f32, pitch: f32) -> [f32; 3] {
    let (ry, rp) = (yaw * DEG_TO_RAD, pitch * DEG_TO_RAD);
    let (sy, sp, cy, cp) = (sinf(ry), sinf(rp), cosf(ry), cosf(rp));
    [-cp * sy, cp * cy, sp]
}

/// `Math::Vector3::Heading` (Runtime `0x18007d3d0`): `atan2f(−x, y)` in degrees.
pub fn heading(x: f32, y: f32) -> f32 {
    crate::crt::atan2f(-x, y) * RAD_TO_DEG
}

/// `Math::Scalar::SinCos` on an angle in degrees: `(sin, cos)`.
pub fn sin_cos_deg(degrees: f32) -> (f32, f32) {
    let r = degrees * DEG_TO_RAD;
    (sinf(r), cosf(r))
}

/// `Math::Transform`: a rotation quaternion `(x, y, z, w)` and an offset, as a ModelComponent
/// keeps its placement (the entity transform `+0x148`, `+0x158`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transform {
    pub q: [f32; 4],
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

/// The Runtime's quaternion product (`0x18001eb60`), its terms in the original's order.
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
    /// `Math::Transform::FromYawPitchOffset` (Runtime `0x18007e540`): the half angles' sines
    /// and cosines, `q = (−cy·sp, −sy·sp, −cp·sy, cp·cy)`.
    pub fn from_yaw_pitch_offset(yaw: f32, pitch: f32, offset: [f32; 3]) -> Transform {
        let hy = (yaw * 0.5) * DEG_TO_RAD;
        let hp = (pitch * 0.5) * DEG_TO_RAD;
        let (sy, sp, cy, cp) = (sinf(hy), sinf(hp), cosf(hy), cosf(hp));
        Transform {
            q: [-(cy * sp), -(sy * sp), -(cp * sy), cp * cy],
            p: offset,
        }
    }

    /// The rotation a CharacterComponent's update writes (`FUN_14010e990`): `(+0, +0,
    /// −sinf(h), cosf(h))` with `h = (yaw × DEG_TO_RAD) × 0.5`, rounded in that order (a model's
    /// `FromYawPitchOffset` halves first, and its zeros are negative).
    pub fn character_rotation(yaw: f32) -> [f32; 4] {
        let h = (yaw * DEG_TO_RAD) * 0.5;
        [0.0, 0.0, -sinf(h), cosf(h)]
    }

    /// A placed entity's model transform: its yaw, pitch 0, its position.
    pub fn of_model(yaw: f32, position: [f32; 3]) -> Transform {
        Transform::from_yaw_pitch_offset(yaw, 0.0, position)
    }

    /// `Math::Quaternion::Rotate` (Runtime `0x18001ea20`): `q · (v, 0) · q̄`.
    pub fn rotate(&self, v: [f32; 3]) -> [f32; 3] {
        let q = self.q;
        let conj = [-q[0], -q[1], -q[2], q[3]];
        let t = qmul(q, [v[0], v[1], v[2], 0.0]);
        let r = qmul(t, conj);
        [r[0], r[1], r[2]]
    }

    /// `Math::Transform::operator*(Vector3)` (Runtime `0x18001eae0`): rotated, then offset.
    pub fn apply(&self, v: [f32; 3]) -> [f32; 3] {
        let r = self.rotate(v);
        [r[0] + self.p[0], self.p[1] + r[1], r[2] + self.p[2]]
    }

    /// `Math::Transform::Inverse` (Runtime `0x18005e620`): the conjugate, and the negated
    /// offset turned by it.
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

    /// A model-space heading turned into the world (`FUN_140128d50`: `Heading` of the rotated
    /// `FromYawPitch(yaw, 0)`).
    pub fn heading_of(&self, local_yaw: f32) -> f32 {
        let v = self.rotate(from_yaw_pitch(local_yaw, 0.0));
        heading(v[0], v[1])
    }
}
