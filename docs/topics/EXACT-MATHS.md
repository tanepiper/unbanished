# Exact maths and random numbers

Un-Banished must give the same results as the original, to the last bit. This page explains why
that is hard, and what the port does about it. The code is in
[unbanished-core](../../crates/core/README.md).

## Why "close" is not enough

Banished's simulation feeds its results back into itself. The map generator adds noise values,
and the sum decides where a river runs. A citizen compares two distances, and the smaller one
decides where it walks. If a single float has a different last bit, a comparison can go the
other way. After that the map, the paths and, an hour later, the whole town
can differ.

So the port does not aim for "the same within a tolerance". It aims for identical bits, and the
tests compare bits (`f32::to_bits`), not values.

## Float operations in the original's order

Float addition is not associative: `(a + b) + c` can differ from `a + (b + c)`. So the port
writes every sum and product in the original's order, as the disassembly shows it. It also keeps
the original's types: `f32` where the original uses single precision, `f64` where it uses
double, and truncation where it converts with `cvttss2si`.

## The C runtime's functions

The original calls the Microsoft C runtime for `sinf`, `cosf`, `acosf` and `atan2f`. These
functions can give a different last bit from Rust's `f32::sin` and its relatives. So
`unbanished_core::crt` ports the C runtime's own algorithms, and simulation code uses them. The
test `crt_math` compares them with the original functions, run in the emulator.

`unbanished_core::math` builds the engine's vector helpers on top of them: normalising a vector,
the heading of a direction, and the engine's `Transform` (a rotation and an offset).

## Random numbers

The engine's random number generator, `System::Random`, is a Mersenne Twister (MT19937): an
index and 624 words of state. `unbanished_core::random::Random` is the same generator, with the
same seed set-up and the same helper functions:

| Function | Range | Original |
|---|---|---|
| `rand()` | any `u32` | `Rand()` |
| `rand_below(n)` | `0` to `n - 1` | `Rand(unsigned)` |
| `rand_range(lo, hi)` | `lo` to `hi`, both included | `Rand(int, int)` |
| `rand_tol(t)` | `-t` to `t` | `RandTol(int)` |
| `rand_float()` | `0` to `1` | `RandFloat()` |
| `rand_float_tol()` | `-1` to `1` | `RandFloatTol()` |

The game uses **one** generator for the whole simulation, seeded from the map seed. Every draw
moves it forward. So the port must draw the same numbers, in the same order, the same number of
times. One extra draw anywhere changes every later number. Most differences found during the
port were draws in the wrong order or count.

To find such a difference, record the draws of both sides and compare them:
`tools/emu/rng_trace.py` records the original's draws in the emulator, `random::trace` records
the port's, and `tools/triage/rngdiff.py` names the first draw that differs. See
[Emulate for exact numbers](../develop/guides/EMULATE.md#random-draw-parity).

## Sorting

The citizens sort their choices with the C runtime's `qsort`. That sort is not stable. Two equal
items can come out in either order, depending on the algorithm. The order is visible in the
game, so `behaviour::ai::crt_qsort_by` reproduces the same algorithm (median of three, with an
insertion sort below 9 items).

## Game time

The engine counts time in integer microseconds (`System::Time`). Seconds are converted to time
through `f64`, with truncation, and time is converted back to `f32` seconds with rounding. The
calendar keeps integer time; it does not add up rounded float steps. `scheduler::ticks` and
`scheduler::seconds` make the same conversions as the original. One year is 3,600 seconds of game
time.

## No hidden sources of difference

The rules in `crates/game` must not use anything that can change between runs:

- no clock or wall time;
- no iteration over a `HashMap` where the order reaches the result;
- no random numbers except the ported `Random`.

`tools/triage/determinism.sh` checks this: two runs with the same seed must draw the same frame.

## Read more

- [GAME-LOOP-STEP-19.md](../GAME-LOOP-STEP-19.md): the original time units.
- [CITIZEN-AI.md](../CITIZEN-AI.md#shared-random-order): the shared random order in citizen
  decisions.
- [COMPATIBILITY.md](../COMPATIBILITY.md): how versions of Un-Banished are kept identical.
