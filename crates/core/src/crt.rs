//! The float functions of the Microsoft C runtime, as the game calls them: [`sinf`], [`cosf`],
//! [`acosf`] and [`atan2f`].
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

/// `atan(k / 256)` for k = 16..=256, each the double just below the exact value (the
/// Runtime's table at `0x1801724a0`; computed here, see the test).
const ATAN_TABLE: [u64; 241] = [
    0x3faff55bb72cfde9,
    0x3fb0f99ea71d52a6,
    0x3fb1f86dbf082d58,
    0x3fb2f719318a4a9a,
    0x3fb3f59f0e7c559d,
    0x3fb4f3fd677292fb,
    0x3fb5f2324fd2d7b2,
    0x3fb6f03bdcea4b0c,
    0x3fb7ee182602f10e,
    0x3fb8ebc54478fb28,
    0x3fb9e94153cfdcf1,
    0x3fbae68a71c722b8,
    0x3fbbe39ebe6f07c3,
    0x3fbce07c5c3cca32,
    0x3fbddd21701eba6e,
    0x3fbed98c2190043a,
    0x3fbfd5ba9aac2f6d,
    0x3fc068d584212b3d,
    0x3fc0e6adccf40881,
    0x3fc1646541060850,
    0x3fc1e1fafb043726,
    0x3fc25f6e171a535c,
    0x3fc2dcbdb2fba1ff,
    0x3fc359e8edeb99a3,
    0x3fc3d6eee8c6626c,
    0x3fc453cec6092a9e,
    0x3fc4d087a9da4f17,
    0x3fc54d18ba11570a,
    0x3fc5c9811e3ec269,
    0x3fc645bfffb3aa73,
    0x3fc6c1d4898933d8,
    0x3fc73dbde8a7d201,
    0x3fc7b97b4bce5b02,
    0x3fc8350be398ebc7,
    0x3fc8b06ee2879c28,
    0x3fc92ba37d050271,
    0x3fc9a6a8e96c8626,
    0x3fca217e601081a5,
    0x3fca9c231b403279,
    0x3fcb1696574d780b,
    0x3fcb90d7529260a2,
    0x3fcc0ae54d768466,
    0x3fcc84bf8a742e6d,
    0x3fccfe654e1d5395,
    0x3fcd77d5df205736,
    0x3fcdf110864c9d9d,
    0x3fce6a148e96ec4d,
    0x3fcee2e1451d980c,
    0x3fcf5b75f92c80dd,
    0x3fcfd3d1fc40dbe4,
    0x3fd025fa510665b5,
    0x3fd061eea03d6290,
    0x3fd09dc597d86362,
    0x3fd0d97ee509acb3,
    0x3fd1151a362431c9,
    0x3fd150973a9ce546,
    0x3fd18bf5a30bf178,
    0x3fd1c735212dd883,
    0x3fd2025567e47c95,
    0x3fd23d562b381041,
    0x3fd278372057ef45,
    0x3fd2b2f7fd9b5fe2,
    0x3fd2ed987a823cfe,
    0x3fd328184fb58951,
    0x3fd362773707ebcb,
    0x3fd39cb4eb76157b,
    0x3fd3d6d129271134,
    0x3fd410cbad6c7d32,
    0x3fd44aa436c2af09,
    0x3fd4845a84d0c21b,
    0x3fd4bdee586890e6,
    0x3fd4f75f73869978,
    0x3fd530ad9951cd49,
    0x3fd569d88e1b4cd7,
    0x3fd5a2e0175e0f4e,
    0x3fd5dbc3fbbe768d,
    0x3fd614840309cfe1,
    0x3fd64d1ff635c1c5,
    0x3fd685979f5fa6fd,
    0x3fd6bdeac9cbd76c,
    0x3fd6f61941e4def0,
    0x3fd72e22d53aa2a9,
    0x3fd7660752817501,
    0x3fd79dc6899118d1,
    0x3fd7d5604b63b3f7,
    0x3fd80cd46a14b1d0,
    0x3fd84422b8df95d7,
    0x3fd87b4b0c1ebedb,
    0x3fd8b24d394a1b25,
    0x3fd8e92916f5cde8,
    0x3fd91fde7cd0c662,
    0x3fd9566d43a34907,
    0x3fd98cd5454d6b18,
    0x3fd9c3165cc58107,
    0x3fd9f93066168001,
    0x3fda2f233e5e530b,
    0x3fda64eec3cc23fc,
    0x3fda9a92d59e98cf,
    0x3fdad00f5422058b,
    0x3fdb056420ae9343,
    0x3fdb3a911da65c6c,
    0x3fdb6f962e737efb,
    0x3fdba473378624a5,
    0x3fdbd9281e528191,
    0x3fdc0db4c94ec9ef,
    0x3fdc42191ff11eb6,
    0x3fdc76550aad71f8,
    0x3fdcaa6872f3631b,
    0x3fdcde53432c1350,
    0x3fdd121566b7f2ad,
    0x3fdd45aec9ec862b,
    0x3fdd791f5a1226f4,
    0x3fddac670561bb4f,
    0x3fdddf85bb026974,
    0x3fde127b6b0744af,
    0x3fde4548066cf51a,
    0x3fde77eb7f175a34,
    0x3fdeaa65c7cf28c4,
    0x3fdedcb6d43f8434,
    0x3fdf0ede98f393cf,
    0x3fdf40dd0b541417,
    0x3fdf72b221a4e495,
    0x3fdfa45dd3029258,
    0x3fdfd5e0175fdf83,
    0x3fe0039c73c1a40b,
    0x3fe01c341e82422d,
    0x3fe034b709250488,
    0x3fe04d25314342e5,
    0x3fe0657e94db30cf,
    0x3fe07dc3324e9b38,
    0x3fe095f30861a58f,
    0x3fe0ae0e1639866c,
    0x3fe0c6145b5b43da,
    0x3fe0de05d7aa6f7c,
    0x3fe0f5e28b67e295,
    0x3fe10daa77307a0d,
    0x3fe1255d9bfbd2a8,
    0x3fe13cfbfb1b056e,
    0x3fe1548596376469,
    0x3fe16bfa6f5137e1,
    0x3fe1835a88be7c13,
    0x3fe19aa5e5299f99,
    0x3fe1b1dc87904284,
    0x3fe1c8fe7341f64f,
    0x3fe1e00babdefeb3,
    0x3fe1f7043557138a,
    0x3fe20de813e823b1,
    0x3fe224b74c1d192a,
    0x3fe23b71e2cc9e6a,
    0x3fe25217dd17e501,
    0x3fe268a940696da6,
    0x3fe27f261273d1b3,
    0x3fe2958e59308e30,
    0x3fe2abe21aded073,
    0x3fe2c2215e024465,
    0x3fe2d84c2961e48b,
    0x3fe2ee628406cbca,
    0x3fe30464753b090a,
    0x3fe31a52048874be,
    0x3fe3302b39b78856,
    0x3fe345f01cce37bb,
    0x3fe35ba0b60eccce,
    0x3fe3713d0df6c503,
    0x3fe386c52d3db11e,
    0x3fe39c391cd41719,
    0x3fe3b198e5e2564a,
    0x3fe3c6e491c78dc4,
    0x3fe3dc1c2a188504,
    0x3fe3f13fb89e96f4,
    0x3fe4064f47569f48,
    0x3fe41b4ae06fea41,
    0x3fe430328e4b26d5,
    0x3fe445065b795b55,
    0x3fe459c652badc7f,
    0x3fe46e727efe4715,
    0x3fe4830aeb5f7bfd,
    0x3fe4978fa3269ee1,
    0x3fe4ac00b1c71762,
    0x3fe4c05e22de94e4,
    0x3fe4d4a8023414e8,
    0x3fe4e8de5bb6ec04,
    0x3fe4fd013b7dd17e,
    0x3fe51110adc5ed81,
    0x3fe5250cbef1e9fa,
    0x3fe538f57b89061e,
    0x3fe54ccaf0362c8f,
    0x3fe5608d29c70c34,
    0x3fe5743c352b33b9,
    0x3fe587d81f732fba,
    0x3fe59b60f5cfab9d,
    0x3fe5aed6c5909517,
    0x3fe5c2399c244260,
    0x3fe5d58987169b18,
    0x3fe5e8c6941043cf,
    0x3fe5fbf0d0d5cc49,
    0x3fe60f084b46e05e,
    0x3fe6220d115d7b8d,
    0x3fe634ff312d1f3b,
    0x3fe647deb8e20b8f,
    0x3fe65aabb6c07b02,
    0x3fe66d663923e086,
    0x3fe6800e4e7e2857,
    0x3fe692a40556fb6a,
    0x3fe6a5276c4b0575,
    0x3fe6b798920b3d98,
    0x3fe6c9f7855c3198,
    0x3fe6dc44551553ae,
    0x3fe6ee7f10204aef,
    0x3fe700a7c5784633,
    0x3fe712be84295198,
    0x3fe724c35b4fae7b,
    0x3fe736b65a172dff,
    0x3fe748978fba8e0f,
    0x3fe75a670b82d8d8,
    0x3fe76c24dcc6c6c0,
    0x3fe77dd112ea22c7,
    0x3fe78f6bbd5d315e,
    0x3fe7a0f4eb9c19a2,
    0x3fe7b26cad2e50fd,
    0x3fe7c3d311a6092b,
    0x3fe7d528289fa093,
    0x3fe7e66c01c114fd,
    0x3fe7f79eacb97898,
    0x3fe808c03940694a,
    0x3fe819d0b7158a4c,
    0x3fe82ad036000005,
    0x3fe83bbec5cdee22,
    0x3fe84c9c7653f7ea,
    0x3fe85d69576cc2c5,
    0x3fe86e2578f87ae5,
    0x3fe87ed0eadc5a2a,
    0x3fe88f6bbd023118,
    0x3fe89ff5ff57f1f7,
    0x3fe8b06fc1cf3dfe,
    0x3fe8c0d9145cf49d,
    0x3fe8d13206f8c4ca,
    0x3fe8e17aa99cc05d,
    0x3fe8f1b30c44f167,
    0x3fe901db3eeef187,
    0x3fe911f35199833b,
    0x3fe921fb54442d18,
];

#[cfg(test)]
mod tests {
    use super::*;

    /// The table against the Runtime's own, when the binary is at hand.
    #[test]
    fn the_atan_table_is_the_runtimes() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../game-data/original/Runtime-steam-x64.dll"
        );
        let Ok(img) = std::fs::read(path) else {
            return;
        };
        // .rdata: virtual 0x16f000 at file offset 0x16d400 (the DLL's section table).
        let va = 0x1801724a0u64 - 0x180000000;
        let pe = u32::from_le_bytes(img[0x3c..0x40].try_into().unwrap()) as usize;
        let n = u16::from_le_bytes(img[pe + 6..pe + 8].try_into().unwrap()) as usize;
        let opt = u16::from_le_bytes(img[pe + 20..pe + 22].try_into().unwrap()) as usize;
        let mut off = None;
        for k in 0..n {
            let o = pe + 24 + opt + 40 * k;
            let rd =
                |i: usize| u32::from_le_bytes(img[o + i..o + i + 4].try_into().unwrap()) as u64;
            let (vsize, rva, raw) = (rd(8), rd(12), rd(20));
            if rva <= va && va < rva + vsize {
                off = Some((va - rva + raw) as usize);
            }
        }
        let off = off.unwrap();
        for (k, &want) in ATAN_TABLE.iter().enumerate() {
            let got = u64::from_le_bytes(img[off + 8 * k..off + 8 * k + 8].try_into().unwrap());
            assert_eq!(got, want, "entry {k}");
        }
    }

    #[test]
    fn plain_values() {
        assert_eq!(sinf(0.0), 0.0);
        assert_eq!(cosf(0.0), 1.0);
        assert_eq!(acosf(1.0), 0.0);
        assert!((atan2f(1.0, 1.0) - std::f32::consts::FRAC_PI_4).abs() < 1e-7);
    }
}
