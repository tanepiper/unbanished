//! Engine math helpers (`Runtime-steam-x64.dll` `Math::Scalar`) with the original's precision.
//! The CRT `sinf`/`cosf` are evaluated in double precision and rounded, which matches the MSVC
//! x64 runtime for the arguments the game uses (checked against emulation).

/// `DAT_1801647ec`: degrees to radians as an f32.
pub const DEG_TO_RAD: f32 = 0.017453292;

pub fn sinf(x: f32) -> f32 {
    (x as f64).sin() as f32
}

pub fn cosf(x: f32) -> f32 {
    (x as f64).cos() as f32
}

/// `Math::Scalar::SinCos` on an angle in degrees: `(sin, cos)`.
pub fn sin_cos_deg(degrees: f32) -> (f32, f32) {
    let r = degrees * DEG_TO_RAD;
    (sinf(r), cosf(r))
}
