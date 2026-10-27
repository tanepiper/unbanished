//! The float functions of the Microsoft C runtime, as the game calls them: [`sinf`], [`cosf`],
//! [`acosf`] and [`atan2f`], and the compiler's float-to-integer conversion, [`truncate`].
//!
//! These functions are not correctly rounded, so their results can differ from Rust's `f32::sin`
//! and its relatives in the last bit. That is enough to change the game's results. So this module
//! ports the C runtime's own algorithms, operation by operation, and simulation code uses them.
//! `tools/emu/crt_math.py` checks them against the original functions. `powf`, `floorf` and
//! `sqrtf` are correctly rounded in both, so Rust's own functions give the same results.
//!
//! ```
//! use unbanished_core::crt;
//!
//! let s = crt::sinf(0.5);
//! let angle = crt::atan2f(1.0, 2.0);
//! assert!(s > 0.47 && angle > 0.46);
//! ```
//!
//! Original: `FUN_140137160` (`sinf`, the Runtime's copy at `0x18012a620` is the same),
//! `FUN_140136560` (`cosf`, Runtime `0x1801295a0`), `FUN_140136090` (`acosf`), Runtime
//! `0x180128f10` (`atan2f`, behind `Vector3::Heading`).

// The constants below are the runtime's own, written as read from its data. Some are close to
// the standard library's constants, or have more digits than an f32 or f64 holds; they stay as
// read, so each one can be checked against the address it came from.
#![allow(clippy::approx_constant, clippy::excessive_precision)]

mod atan_table;
use atan_table::ATAN_TABLE;

const fn d(bits: u64) -> f64 {
    f64::from_bits(bits)
}

// FUN_140137160's constants (`0x140183190..0x1401832a0`).
const S_ONE: f64 = 1.0;
const S_HALF: f64 = 0.5;
const S_SIXTH: f64 = 0.16666666666666666;
const S_2_PI: f64 = 0.6366197723675814;
const S_PIO2_1: f64 = 1.5707963267341256;
const S_PIO2_1T: f64 = 6.077100506506192e-11;
const S_PIO2_2: f64 = 6.077100506303966e-11;
const S_PIO2_2T: f64 = 2.0222662487959506e-21;
const S_BIG: u64 = 0x411e_8480_0000_0000; // 500000.0
const S_S1: f64 = -0.16666666666666666;
const S_S2: f64 = 0.008333333333333333;
const S_S3: f64 = -0.0001984126984126984;
const S_S4: f64 = 2.7557319223985893e-06;
const S_C0: f64 = -0.5;
const S_C1: f64 = 0.041666666666666664;
const S_C2: f64 = -0.0013888888888888887;
const S_C3: f64 = 2.4801587301587298e-05;
const S_C4: f64 = -2.755731922398589e-07;

// FUN_140136560's polynomial (`0x140182610..0x140182648`).
const C_S1: f64 = -0.16666666666666666;
const C_S2: f64 = 0.00833333333333095;
const C_S3: f64 = -0.00019841269836761127;
const C_S4: f64 = 2.7557316103728802e-06;
const C_C1: f64 = 0.041666666666666664;
const C_C2: f64 = -0.0013888888888887398;
const C_C3: f64 = 2.480158729876704e-05;
const C_C4: f64 = -2.755731727234419e-07;

/// The `π/2` reduction both use: the region and the reduced argument, and the exponent the
/// small cases are told by (taken before the extra precision step).
fn reduce(x: f64, ax: u64) -> (u32, f64, u64) {
    let region = ((x * S_2_PI) + S_HALF) as i32;
    let dr = region as f64;
    let r1 = x - S_PIO2_1 * dr;
    let mut r = r1 - S_PIO2_1T * dr;
    let er = (r.to_bits() << 1) >> 53;
    if 15 < (ax >> 52) as i64 - er as i64 {
        let t = S_PIO2_2 * dr;
        let r2 = r1 - t;
        let tail = S_PIO2_2T * dr - ((r1 - r2) - t);
        r = r2 - tail;
    }
    (region as u32, r, er)
}

fn sin_poly(r: f64, x2: f64, s1: f64, s2: f64, s3: f64, s4: f64) -> f64 {
    let x4 = x2 * x2;
    let p = (s4 * x2 + s3) * x4 + (s2 * x2 + s1);
    p * (r * x2) + r
}

/// The sine of `x` (radians), as the C runtime's `sinf` computes it.
///
/// Original: `FUN_140137160`.
pub fn sinf(x: f32) -> f32 {
    if x.to_bits() & 0x7f80_0000 == 0x7f80_0000 {
        return f32::NAN;
    }
    let xd = x as f64;
    let ax = xd.to_bits() & 0x7fff_ffff_ffff_ffff;
    if ax <= 0x3fe9_21fb_5444_2d18 {
        if ax < 0x3f80_0000_0000_0000 {
            if ax < 0x3f20_0000_0000_0000 {
                return x;
            }
            return (xd - ((xd * xd) * xd) * S_SIXTH) as f32;
        }
        let x2 = xd * xd;
        return sin_poly(xd, x2, S_S1, S_S2, S_S3, S_S4) as f32;
    }
    let neg = xd.to_bits() != ax;
    let v = if neg { 0.0 - xd } else { xd };
    let (region, r, er) = if ax < S_BIG {
        reduce(v, ax)
    } else {
        // `__remainder_piby2f_inline`: past 500000, which the game never passes.
        return (xd.sin()) as f32;
    };
    let odd = region & 1 != 0;
    let mut out = if er >= 0x3f2 {
        let x2 = r * r;
        if odd {
            let x4 = x2 * x2;
            let p = ((S_C4 * x2 + S_C3) * x4 + (S_C2 * x2 + S_C1)) * x4;
            (S_C0 * x2 + S_ONE) + p
        } else {
            sin_poly(r, x2, S_S1, S_S2, S_S3, S_S4)
        }
    } else if er > 0x3de {
        let x2 = r * r;
        if odd {
            S_ONE - x2 * S_HALF
        } else {
            r - (S_SIXTH * r) * x2
        }
    } else if odd {
        S_ONE
    } else {
        r
    };
    let q = (region >> 1) & 1;
    if q != neg as u32 {
        out = 0.0 - out;
    }
    out as f32
}

/// The cosine of `x` (radians), as the C runtime's `cosf` computes it.
///
/// Original: `FUN_140136560`.
pub fn cosf(x: f32) -> f32 {
    if x.to_bits() & 0x7f80_0000 == 0x7f80_0000 {
        return f32::NAN;
    }
    let xd = x as f64;
    let ax = xd.to_bits() & 0x7fff_ffff_ffff_ffff;
    let (region, r, er) = if ax <= 0x3fe9_21fb_5444_2d18 {
        (0, xd, 0x3f2)
    } else {
        let v = if xd.to_bits() != ax { 0.0 - xd } else { xd };
        if ax < S_BIG {
            reduce(v, ax)
        } else {
            // `__remainder_piby2f_inline`: past 500000, which the game never passes.
            return (xd.cos()) as f32;
        }
    };
    let odd = region & 1 != 0;
    let x2 = r * r;
    let mut out = if er >= 0x3f2 {
        let x4 = x2 * x2;
        if odd {
            ((C_S4 * x2 + C_S3) * x4 + (C_S2 * x2 + C_S1)) * (x2 * r) + r
        } else {
            let p = ((C_C4 * x2 + C_C3) * x4 + (C_C2 * x2 + C_C1)) * x4;
            p - (S_HALF * x2 - S_ONE)
        }
    } else if er > 0x3de {
        if odd {
            r - (S_SIXTH * r) * x2
        } else {
            S_ONE - x2 * S_HALF
        }
    } else if odd {
        r
    } else {
        S_ONE
    };
    if (region.wrapping_add(1)) & 2 != 0 {
        out = 0.0 - out;
    }
    out as f32
}

/// The arc cosine of `x`, in radians, as the C runtime's `acosf` computes it.
///
/// Original: `FUN_140136090`.
pub fn acosf(x: f32) -> f32 {
    const P0: f32 = -0.013381929136812687;
    const P1: f32 = 0.003961374517530203;
    const P2: f32 = 0.05652986839413643;
    const P3: f32 = 0.18416160345077515;
    const Q0: f32 = 1.1049696207046509;
    const Q1: f32 = 0.8364112973213196;
    const PIO2_F: f32 = 1.5707963705062866;
    const PI_F: f32 = 3.1415927410125732;
    const PIO2_LO: f64 = 6.123233995736766e-17;
    const PIO2: f64 = 1.5707963267948966;
    const PI: f64 = 3.141592653589793;
    let ux = x.to_bits();
    let negative = ux & 0x8000_0000 != 0;
    let e = ((ux >> 23) & 0xff) as i32 - 127;
    if ux & 0x7fff_ffff > 0x7f80_0000 {
        return f32::from_bits(ux | 0x0040_0000);
    }
    if e < -26 {
        return PIO2_F;
    }
    if 0 <= e {
        return if x == 1.0 {
            0.0
        } else if x == -1.0 {
            PI_F
        } else {
            f32::NAN
        };
    }
    let mut ax = if negative { -x } else { x };
    let transform = -1 <= e;
    let (r, s) = if transform {
        let r = (1.0 - ax) * 0.5;
        let s = r.sqrt();
        ax = s;
        (r, s)
    } else {
        (ax * ax, 0.0)
    };
    let mut p = P0 - r * P1;
    p *= r;
    p -= P2;
    p *= r;
    p += P3;
    p *= r;
    let q = Q0 - r * Q1;
    let u = p / q;
    if transform {
        if negative {
            let t = (((u * ax) as f64 - PIO2_LO) + s as f64) * 2.0;
            (PI - t) as f32
        } else {
            let c = f32::from_bits(s.to_bits() & 0xffff_0000);
            let a = (2.0 * ax) * u;
            let b = ((r - c * c) / (c + s)) * 2.0;
            b + a + c * 2.0
        }
    } else {
        let t = (x as f64) - (PIO2_LO - (u * x) as f64);
        (PIO2 - t) as f32
    }
}

/// The angle of the point `(x, y)`, in radians from -π to π, as the C runtime's `atan2f`
/// computes it.
///
/// Original: Runtime `0x180128f10`.
pub fn atan2f(y: f32, x: f32) -> f32 {
    const SIGN: u64 = 0x8000_0000_0000_0000;
    const ABS: u64 = 0x7fff_ffff_ffff_ffff;
    const INF: u64 = 0x7ff0_0000_0000_0000;
    const PI: f64 = 3.141592653589793;
    const PIO2: f64 = 1.5707963267948966;
    const PIO4: f64 = 0.7853981633974483;
    const PI3O4: f64 = 2.356194490192345;
    let (yd, xd) = (y as f64, x as f64);
    let (yb, xb) = (yd.to_bits(), xd.to_bits());
    let (ysign, xsign) = (yb & SIGN != 0, xb & SIGN != 0);
    let (ay, ax) = (yb & ABS, xb & ABS);
    let diff = ((yb >> 52) & 0x7ff) as i32 - ((xb >> 52) & 0x7ff) as i32;
    let signed = |v: f64, neg: bool| (if neg { -v } else { v }) as f32;
    if ax > INF {
        return f32::from_bits(x.to_bits() | 0x0040_0000);
    }
    if ay > INF {
        return f32::from_bits(y.to_bits() | 0x0040_0000);
    }
    if ay == 0 {
        return if xsign { signed(PI, ysign) } else { yd as f32 };
    }
    if ax == 0 && ysign {
        return signed(PIO2, true);
    }
    if 26 < diff {
        return signed(PIO2, ysign);
    }
    if diff < -13 && !xsign {
        if diff < -150 {
            return if ysign { -0.0 } else { 0.0 };
        }
        if diff >= -126 {
            return (yd / xd) as f32;
        }
        // A result too small for a normal float: scaled by 2^100 and unscaled by hand.
        let f = (d(0x4630_0000_0000_0000) * yd) / xd;
        let sign = f.to_bits() & SIGN;
        let a = f.to_bits() & ABS;
        let e = ((a >> 52) & 0x7ff) as i32 - 100;
        let bits = if 0 < e {
            ((e as u64) << 52) | (a & 0x800f_ffff_ffff_ffff)
        } else {
            let m = (a & 0x801f_ffff_ffff_ffff) | 0x0010_0000_0000_0000;
            let n = 1 - e;
            if n <= 0x36 {
                let m = m >> (n - 1);
                (m >> 1) + (m & 1)
            } else {
                0
            }
        };
        return f64::from_bits(bits | sign) as f32;
    }
    if diff < -26 && xsign {
        return signed(PI, ysign);
    }
    if ay == INF && ax == INF {
        return signed(if xsign { PI3O4 } else { PIO4 }, ysign);
    }
    let (mut small, mut big) = (f64::from_bits(ay), f64::from_bits(ax));
    let swap = small > big;
    if swap {
        std::mem::swap(&mut small, &mut big);
    }
    let t = small / big;
    let mut r = if t > 0.0625 {
        let u = (t * 256.0 + 0.5) as i32;
        let j = u as f64;
        let dd = (small * 256.0 - j * big) / (j * small + big * 256.0);
        let a = dd + d(ATAN_TABLE[(u - 16) as usize]);
        a - ((dd * dd) * dd) * 0.33333333333224097
    } else if 0.0001 > t {
        t
    } else {
        let x2 = t * t;
        let p = 0.3333333333333317 - (0.19999999999393223 - x2 * 0.1428571356180717) * x2;
        t - p * (x2 * t)
    };
    if swap {
        r = PIO2 - r;
    }
    if xsign {
        r = PI - r;
    }
    if ysign {
        r = -r;
    }
    r as f32
}

/// `(int)v` as the compiler writes it, with `cvttss2si`: it rounds towards zero, and a NaN or
/// a value outside the `i32` range gives `i32::MIN`. Rust's `v as i32` gives 0 for a NaN and
/// the nearest limit for a value out of range, so ported code that converts a float the game
/// could leave NaN uses this.
///
/// ```
/// use unbanished_core::crt::truncate;
///
/// assert_eq!(truncate(-2.7), -2);
/// assert_eq!(truncate(f32::NAN), i32::MIN);
/// assert_eq!(truncate(3e9), i32::MIN);
/// ```
pub fn truncate(v: f32) -> i32 {
    if v.is_nan() || v >= 2_147_483_648.0 || v < -2_147_483_648.0 {
        i32::MIN
    } else {
        v as i32
    }
}

#[cfg(test)]
mod tests;
