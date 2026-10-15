//! `System::Random` (Runtime-steam-x64.dll 0x1800ca560..0x1800ca8b0): MT19937.
//!
//! Layout matches the engine object: a `u32` index followed by 624 state words; `Serialize` writes
//! the index then the state, which is how saves store it.

/// Opt-in log of draws for parity checks against the original (`tools/emu/rng_trace.py`,
/// compared with `tools/triage/rngdiff.py`). Only outermost draws are recorded (`rand_float`
/// calling `rand` logs once), with the Rust caller's location.
pub mod trace {
    use std::cell::{Cell, RefCell};
    use std::panic::Location;

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Draw {
        /// `Rand`, `Rand(n)`, `Rand(lo,hi)`, `RandTol(t)`, `RandFloat`, `RandFloatTol`.
        pub kind: String,
        pub at: &'static Location<'static>,
    }

    thread_local! {
        static LOG: RefCell<Option<Vec<Draw>>> = const { RefCell::new(None) };
        static DEPTH: Cell<u32> = const { Cell::new(0) };
    }

    /// Start recording on this thread (clears any previous log).
    pub fn start() {
        LOG.with(|l| *l.borrow_mut() = Some(Vec::new()));
    }

    /// Stop recording and return the draws.
    pub fn take() -> Vec<Draw> {
        LOG.with(|l| l.borrow_mut().take()).unwrap_or_default()
    }

    /// Write draws as `kind<TAB>caller` lines (the format `rngdiff.py` reads).
    pub fn write(path: &std::path::Path, draws: &[Draw]) -> std::io::Result<()> {
        let text: String = draws
            .iter()
            .map(|d| format!("{}\t{}:{}\n", d.kind, d.at.file(), d.at.line()))
            .collect();
        std::fs::write(path, text)
    }

    pub(super) struct Guard;

    impl Drop for Guard {
        fn drop(&mut self) {
            DEPTH.with(|d| d.set(d.get() - 1));
        }
    }

    /// Record one draw unless already inside another draw.
    #[track_caller]
    pub(super) fn enter(kind: impl FnOnce() -> String) -> Guard {
        let at = Location::caller();
        let outer = DEPTH.with(|d| {
            let v = d.get();
            d.set(v + 1);
            v == 0
        });
        if outer {
            LOG.with(|l| {
                if let Some(log) = l.borrow_mut().as_mut() {
                    log.push(Draw { kind: kind(), at });
                }
            });
        }
        Guard
    }
}

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
    #[track_caller]
    pub fn rand(&mut self) -> u32 {
        let _t = trace::enter(|| "Rand".into());
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
    #[track_caller]
    pub fn rand_below(&mut self, n: u32) -> u32 {
        let _t = trace::enter(|| format!("Rand({n})"));
        if n == 0 {
            return 0;
        }
        self.rand() % n
    }

    /// `Rand(int, int)` (0x1800ca5e0): inclusive range.
    #[track_caller]
    pub fn rand_range(&mut self, lo: i32, hi: i32) -> i32 {
        let _t = trace::enter(|| format!("Rand({lo},{hi})"));
        let r = self.rand();
        lo.wrapping_add((r % ((hi.wrapping_sub(lo) as u32).wrapping_add(1))) as i32)
    }

    /// `RandTol(int)` (0x1800ca610): `[-t, t]`.
    #[track_caller]
    pub fn rand_tol(&mut self, t: i32) -> i32 {
        let _t = trace::enter(|| format!("RandTol({t})"));
        let r = self.rand();
        (r % ((t as u32).wrapping_mul(2).wrapping_add(1))) as i32 - t
    }

    /// `RandFloat()` (0x1800ca5b0): `[0, 1)` as `(float)u * 2^-32`.
    #[track_caller]
    pub fn rand_float(&mut self) -> f32 {
        let _t = trace::enter(|| "RandFloat".into());
        self.rand() as f32 * 2.328_306_4e-10
    }

    /// `RandFloatTol()` (0x1800ca870): `1 - (float)u * 2^-32 * 2`, in `(-1, 1]`.
    #[track_caller]
    pub fn rand_float_tol(&mut self) -> f32 {
        let _t = trace::enter(|| "RandFloatTol".into());
        1.0 - self.rand() as f32 * 2.328_306_4e-10 * 2.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trace_records_outermost_draws_only() {
        let mut r = Random::new(1);
        trace::start();
        r.rand_float();
        r.rand_below(7);
        let log = trace::take();
        assert_eq!(
            log.iter().map(|d| d.kind.as_str()).collect::<Vec<_>>(),
            ["RandFloat", "Rand(7)"]
        );
        assert!(log[0].at.file().ends_with("random.rs"));
        r.rand();
        assert!(trace::take().is_empty());
    }

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
