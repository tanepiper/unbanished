//! Picking: which triangle a ray hits, as the engine finds the object under the pointer.
//!
//! Original: `Picking::Scene::Intersect`, Runtime `0x180126f50` (the triangle test).

/// Test a ray against one triangle. Returns the distance along the ray when the ray hits the
/// triangle no farther than `maximum`.
///
/// Only front faces count. A hit on an edge, at distance zero or exactly at the current maximum
/// counts too, and an equal distance replaces the earlier hit, as in the original's order.
pub fn triangle(
    origin: [f32; 3],
    direction: [f32; 3],
    vertices: [[f32; 3]; 3],
    maximum: f32,
) -> Option<f32> {
    let [a, b, c] = vertices;
    let e = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let f = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    let d = direction;
    let p = [
        f[2] * d[1] - f[1] * d[2],
        f[0] * d[2] - f[2] * d[0],
        f[1] * d[0] - f[0] * d[1],
    ];
    let det = p[1] * e[1] + p[0] * e[0] + p[2] * e[2];
    let s = [origin[0] - a[0], origin[1] - a[1], origin[2] - a[2]];
    let q = [
        s[1] * e[2] - s[2] * e[1],
        s[2] * e[0] - s[0] * e[2],
        s[0] * e[1] - s[1] * e[0],
    ];
    let u = s[1] * p[1] + s[0] * p[0] + s[2] * p[2];
    let v = q[1] * d[1] + q[0] * d[0] + q[2] * d[2];
    if !(0.0 < det && 0.0 <= u && u <= det && 0.0 <= v && v + u <= det) {
        return None;
    }
    let t = (q[1] * f[1] + q[0] * f[0] + q[2] * f[2]) * (1.0 / det);
    (0.0 <= t && t <= maximum).then_some(t)
}

/// The hit triangle's unit face normal as `Intersect` gives it: `(b − a) × (c − a)` scaled by
/// `1 / sqrtf(y² + x² + z²)` (left as it is when that length is zero).
pub fn face_normal(vertices: [[f32; 3]; 3]) -> [f32; 3] {
    let [a, b, c] = vertices;
    let e = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let f = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    let n = [
        f[2] * e[1] - f[1] * e[2],
        f[0] * e[2] - f[2] * e[0],
        f[1] * e[0] - f[0] * e[1],
    ];
    let len = (n[1] * n[1] + n[0] * n[0] + n[2] * n[2]).sqrt();
    if len == 0.0 {
        return n;
    }
    let inv = 1.0 / len;
    [inv * n[0], inv * n[1], inv * n[2]]
}

/// The point at distance `t` along a ray.
pub fn point(origin: [f32; 3], direction: [f32; 3], t: f32) -> [f32; 3] {
    [
        t * direction[0] + origin[0],
        t * direction[1] + origin[1],
        t * direction[2] + origin[2],
    ]
}
