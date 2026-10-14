//! Inverse MDCT via a DCT-IV computed with a half-size complex FFT.

use std::f64::consts::PI;

#[derive(Clone, Copy, Default)]
struct C {
    re: f32,
    im: f32,
}

impl C {
    fn mul(self, o: C) -> C {
        C {
            re: self.re * o.re - self.im * o.im,
            im: self.re * o.im + self.im * o.re,
        }
    }
}

fn expi(theta: f64) -> C {
    C {
        re: theta.cos() as f32,
        im: theta.sin() as f32,
    }
}

/// Iterative radix-2 forward FFT (`X[k] = sum x[j] e^(-2 pi i jk / n)`).
struct Fft {
    n: usize,
    twiddle: Vec<C>,
    bitrev: Vec<u32>,
}

impl Fft {
    fn new(n: usize) -> Self {
        assert!(n.is_power_of_two());
        let bits = n.trailing_zeros();
        let bitrev = (0..n as u32)
            .map(|i| {
                if bits == 0 {
                    0
                } else {
                    i.reverse_bits() >> (32 - bits)
                }
            })
            .collect();
        let twiddle = (0..n / 2)
            .map(|k| expi(-2.0 * PI * k as f64 / n as f64))
            .collect();
        Fft { n, twiddle, bitrev }
    }

    fn run(&self, x: &mut [C]) {
        for i in 0..self.n {
            let j = self.bitrev[i] as usize;
            if i < j {
                x.swap(i, j);
            }
        }
        let mut len = 2;
        while len <= self.n {
            let step = self.n / len;
            for start in (0..self.n).step_by(len) {
                for k in 0..len / 2 {
                    let a = x[start + k];
                    let b = x[start + k + len / 2].mul(self.twiddle[k * step]);
                    x[start + k] = C {
                        re: a.re + b.re,
                        im: a.im + b.im,
                    };
                    x[start + k + len / 2] = C {
                        re: a.re - b.re,
                        im: a.im - b.im,
                    };
                }
            }
            len *= 2;
        }
    }
}

/// Unnormalised IMDCT of `m` coefficients to `2m` samples, with WMA's sign convention:
/// `y[n] = -sum_k X[k] cos(pi/m (n + 1/2 + m/2)(k + 1/2))`.
pub struct Imdct {
    m: usize,
    fft: Fft,
    pre: Vec<C>,
    post: Vec<C>,
    buf: Vec<C>,
    dct: Vec<f32>,
}

impl Imdct {
    pub fn new(m: usize) -> Self {
        let h = m / 2;
        let mf = m as f64;
        Imdct {
            m,
            fft: Fft::new(h),
            pre: (0..h).map(|j| expi(-PI * j as f64 / mf)).collect(),
            post: (0..h).map(|q| expi(-PI * (q as f64 + 0.25) / mf)).collect(),
            buf: vec![C::default(); h],
            dct: vec![0.0; m],
        }
    }

    /// DCT-IV: `u[n] = sum_k X[k] cos(pi/m (n + 1/2)(k + 1/2))`.
    fn dct4(&mut self, x: &[f32]) {
        let m = self.m;
        for j in 0..m / 2 {
            let v = C {
                re: x[2 * j],
                im: x[m - 1 - 2 * j],
            };
            self.buf[j] = v.mul(self.pre[j]);
        }
        self.fft.run(&mut self.buf);
        for q in 0..m / 2 {
            let y = self.buf[q].mul(self.post[q]);
            self.dct[2 * q] = y.re;
            self.dct[m - 1 - 2 * q] = -y.im;
        }
    }

    pub fn run(&mut self, x: &[f32], out: &mut [f32]) {
        let m = self.m;
        let h = m / 2;
        self.dct4(&x[..m]);
        let u = &self.dct;
        for n in 0..h {
            out[n] = -u[n + h];
        }
        for n in h..m + h {
            out[n] = u[m + h - 1 - n];
        }
        for n in m + h..2 * m {
            out[n] = u[n - m - h];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_direct_formula() {
        for m in [128usize, 256, 2048] {
            let x: Vec<f32> = (0..m)
                .map(|k| ((k * 7919 % 101) as f32 - 50.0) / 13.0)
                .collect();
            let mut out = vec![0.0; 2 * m];
            Imdct::new(m).run(&x, &mut out);
            for (n, &got) in out.iter().enumerate() {
                let want: f64 = -(0..m)
                    .map(|k| {
                        f64::from(x[k])
                            * (PI / m as f64 * (n as f64 + 0.5 + m as f64 / 2.0) * (k as f64 + 0.5))
                                .cos()
                    })
                    .sum::<f64>();
                assert!(
                    (f64::from(got) - want).abs() < 1e-2,
                    "m={m} n={n} {got} vs {want}"
                );
            }
        }
    }
}
