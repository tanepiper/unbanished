//! The game's random number generator: a Mersenne Twister (MT19937).
//!
//! The whole simulation draws from one [`Random`], seeded with the map seed. A port must draw the
//! same numbers in the same order as the original, so every call here matches one call of the
//! engine's `System::Random`.
//!
//! The state is the same as the engine object's: a `u32` index and 624 state words. A save stores
//! the index, then the words ([`Random::from_parts`] restores them).
//!
//! Measured: `tools/emu/random_calls.py` replays 18,000 calls of every kind on 300 seeds (edge
//! arguments among them: `Rand(0)`, reversed and wide ranges, negative tolerances), and every
//! result matches the original's.
//!
//! ```
//! use unbanished_core::random::Random;
//!
//! let mut rng = Random::new(1234);
//! let die = rng.rand_range(1, 6);
//! assert!((1..=6).contains(&die));
//! ```
//!
//! Original: `System::Random`, Runtime `0x1800ca560` to `0x1800ca8b0`.

/// An optional log of draws, to compare the port's draws with the original's.
///
/// The original's draws are recorded in the emulator (`tools/emu/rng_trace.py`), and
/// `tools/triage/rngdiff.py` finds the first draw that differs. Only the outermost draw of a call
/// is recorded (`rand_float` calling `rand` logs once), with the location of the Rust caller.
pub mod trace {
    use std::cell::{Cell, RefCell};
    use std::panic::Location;

    #[derive(Debug, Clone, PartialEq, Eq)]
    /// One recorded draw.
    pub struct Draw {
        /// `Rand`, `Rand(n)`, `Rand(lo,hi)`, `RandTol(t)`, `RandFloat`, `RandFloatTol`.
        pub kind: String,
        /// The Rust code that made the draw.
        pub at: &'static Location<'static>,
    }

    thread_local! {
        static LOG: RefCell<Option<Vec<Draw>>> = const { RefCell::new(None) };
        static DEPTH: Cell<u32> = const { Cell::new(0) };
        static REPLAY: RefCell<std::collections::VecDeque<u32>> =
            const { RefCell::new(std::collections::VecDeque::new()) };
    }

    /// While recording, make `skips[k]` plain draws before the `k`-th recorded draw. They stand
    /// for draws the original made from the same generator outside the code being compared (its
    /// sounds), so both generators give the same numbers to the rest.
    pub fn replay(skips: impl IntoIterator<Item = u32>) {
        REPLAY.with(|r| *r.borrow_mut() = skips.into_iter().collect());
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

    pub(super) struct Guard {
        /// Plain draws to make first ([`replay`]).
        pub(super) skip: u32,
    }

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
        let mut skip = 0;
        if outer {
            LOG.with(|l| {
                if let Some(log) = l.borrow_mut().as_mut() {
                    log.push(Draw { kind: kind(), at });
                    skip = REPLAY.with(|r| r.borrow_mut().pop_front()).unwrap_or(0);
                }
            });
        }
        Guard { skip }
    }
}

const N: usize = 624;
const M: usize = 397;

/// The game's random number generator. See the [module documentation](self).
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
    /// A generator seeded with `seed`, as the engine seeds one.
    ///
    /// Original: `Random::Random(unsigned int)`, Runtime `0x1800ca740`.
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

    /// A generator restored from a save: its index, then its 624 state words.
    pub fn from_parts(index: u32, state: [u32; N]) -> Random {
        Random { index, state }
    }

    /// The position of the next word in the state.
    pub fn index(&self) -> u32 {
        self.index
    }

    /// The 624 state words.
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

    /// Plain draws standing for the original's draws outside the compared code
    /// ([`trace::replay`]).
    fn skip(&mut self, n: u32) {
        for _ in 0..n {
            self.rand();
        }
    }

    /// The next number, any `u32`.
    ///
    /// Original: `Rand()`, Runtime `0x1800ca800`.
    #[track_caller]
    pub fn rand(&mut self) -> u32 {
        let g = trace::enter(|| "Rand".into());
        self.skip(g.skip);
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

    /// A number from 0 to `n - 1`. When `n` is 0, it returns 0 and draws nothing.
    ///
    /// Original: `Rand(unsigned int)`, Runtime `0x1800ca8b0`.
    #[track_caller]
    pub fn rand_below(&mut self, n: u32) -> u32 {
        let g = trace::enter(|| format!("Rand({n})"));
        self.skip(g.skip);
        if n == 0 {
            return 0;
        }
        self.rand() % n
    }

    /// A number from `lo` to `hi`, both included.
    ///
    /// Original: `Rand(int, int)`, Runtime `0x1800ca5e0`.
    #[track_caller]
    pub fn rand_range(&mut self, lo: i32, hi: i32) -> i32 {
        let g = trace::enter(|| format!("Rand({lo},{hi})"));
        self.skip(g.skip);
        let r = self.rand();
        lo.wrapping_add((r % ((hi.wrapping_sub(lo) as u32).wrapping_add(1))) as i32)
    }

    /// A number from `-t` to `t`, both included.
    ///
    /// Original: `RandTol(int)`, Runtime `0x1800ca610`.
    #[track_caller]
    pub fn rand_tol(&mut self, t: i32) -> i32 {
        let g = trace::enter(|| format!("RandTol({t})"));
        self.skip(g.skip);
        let r = self.rand();
        (r % ((t as u32).wrapping_mul(2).wrapping_add(1))) as i32 - t
    }

    /// A float from 0 to 1, 1 not included: `(float)u × 2⁻³²`.
    ///
    /// Original: `RandFloat()`, Runtime `0x1800ca5b0`.
    #[track_caller]
    pub fn rand_float(&mut self) -> f32 {
        let g = trace::enter(|| "RandFloat".into());
        self.skip(g.skip);
        self.rand() as f32 * 2.328_306_4e-10
    }

    /// A float from -1 to 1, -1 not included: `1 − (float)u × 2⁻³² × 2`.
    ///
    /// Original: `RandFloatTol()`, Runtime `0x1800ca870`.
    #[track_caller]
    pub fn rand_float_tol(&mut self) -> f32 {
        let g = trace::enter(|| "RandFloatTol".into());
        self.skip(g.skip);
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
    fn replay_draws_before_recorded_draws() {
        let mut expected = Random::new(1);
        expected.rand();
        let a = expected.rand_float();
        expected.rand();
        expected.rand();
        let b = expected.rand_below(7);
        let mut r = Random::new(1);
        trace::start();
        trace::replay([1, 2]);
        assert_eq!(r.rand_float(), a);
        assert_eq!(r.rand_below(7), b);
        assert_eq!(trace::take().len(), 2);
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
