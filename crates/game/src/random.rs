//! `System::Random` (Runtime-steam-x64.dll 0x1800ca560..0x1800ca8b0): MT19937.
//!
//! Layout matches the engine object: a `u32` index followed by 624 state words; `Serialize` writes
//! the index then the state, which is how saves store it.

const N: usize = 624;
const M: usize = 397;

#[derive(Clone, PartialEq, Eq)]
pub struct Random {
    index: u32,
    state: [u32; N],
}

impl std::fmt::Debug for Random {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Random")
            .field("index", &self.index)
            .finish_non_exhaustive()
    }
}

impl Random {
    /// `Random::Random(unsigned int)` (0x1800ca740).
    pub fn new(seed: u32) -> Random {
        let mut state = [0u32; N];
        state[0] = seed;
        for i in 1..N {
            let prev = state[i - 1];
            state[i] = (prev ^ (prev >> 30))
                .wrapping_mul(0x6c07_8965)
                .wrapping_add(i as u32);
        }
        Random { index: 0, state }
    }

    /// Restore from serialized form (index, then 624 words).
    pub fn from_parts(index: u32, state: [u32; N]) -> Random {
        Random { index, state }
    }

    pub fn index(&self) -> u32 {
        self.index
    }

    pub fn state(&self) -> &[u32; N] {
        &self.state
    }

    /// `NextGroup` (0x1800ca640).
    fn next_group(&mut self) {
        for i in 0..N {
            let y = (self.state[i] & 0x8000_0000) | (self.state[(i + 1) % N] & 0x7fff_ffff);
            let mut v = self.state[(i + M) % N] ^ (y >> 1);
            if y & 1 != 0 {
                v ^= 0x9908_b0df;
            }
            self.state[i] = v;
        }
    }

    /// `Rand()` (0x1800ca800).
    pub fn rand(&mut self) -> u32 {
        if self.index == 0 {
            self.next_group();
        }
        let mut y = self.state[self.index as usize];
        y ^= y >> 11;
        y ^= (y & 0xff3a_58ad) << 7;
        y ^= (y & 0xffff_df8c) << 15;
        self.index = if self.index + 1 == N as u32 {
            0
        } else {
            self.index + 1
        };
        y ^ (y >> 18)
    }

    /// `Rand(unsigned int)` (0x1800ca8b0): `[0, n)`, or 0 when `n == 0` (consumes nothing then).
    pub fn rand_below(&mut self, n: u32) -> u32 {
        if n == 0 {
            return 0;
        }
        self.rand() % n
    }

    /// `Rand(int, int)` (0x1800ca5e0): inclusive range.
    pub fn rand_range(&mut self, lo: i32, hi: i32) -> i32 {
        let r = self.rand();
        lo.wrapping_add((r % ((hi.wrapping_sub(lo) as u32).wrapping_add(1))) as i32)
    }

    /// `RandTol(int)` (0x1800ca610): `[-t, t]`.
    pub fn rand_tol(&mut self, t: i32) -> i32 {
        let r = self.rand();
        (r % ((t as u32).wrapping_mul(2).wrapping_add(1))) as i32 - t
    }

    /// `RandFloat()` (0x1800ca5b0): `[0, 1)` as `(float)u * 2^-32`.
    pub fn rand_float(&mut self) -> f32 {
        self.rand() as f32 * 2.328_306_4e-10
    }

    /// `RandFloatTol()` (0x1800ca870): `1 - (float)u * 2^-32 * 2`, in `(-1, 1]`.
    pub fn rand_float_tol(&mut self) -> f32 {
        1.0 - self.rand() as f32 * 2.328_306_4e-10 * 2.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_reference_mt19937() {
        // Reference MT19937 outputs for the canonical default seed 5489.
        let mut r = Random::new(5489);
        assert_eq!(r.rand(), 3_499_211_612);
        assert_eq!(r.rand(), 581_869_302);
        assert_eq!(r.rand(), 3_890_346_734);
        for _ in 3..9999 {
            r.rand();
        }
        assert_eq!(r.rand(), 4_123_659_995); // 10000th output
    }
}

#[cfg(test)]
mod golden {
    //! Values produced by the original Runtime DLL under emulation (tools/emu/check_random.py).
    use super::*;

    #[test]
    fn matches_original_engine() {
        let mut r = Random::new(290);
        let v: Vec<u32> = (0..5).map(|_| r.rand()).collect();
        assert_eq!(
            v,
            [1174298092, 3342169725, 1127261350, 2332665128, 3191663389]
        );
        assert_eq!(
            [r.rand_tol(0xffffff), r.rand_tol(0xffffff)],
            [11962091, -9300278]
        );
        assert_eq!(
            r.rand_float_tol().to_bits(),
            0.20040464401245117f32.to_bits()
        );
        assert_eq!(
            r.rand_float_tol().to_bits(),
            0.34909939765930176f32.to_bits()
        );
    }
}
