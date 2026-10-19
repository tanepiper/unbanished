# banished-core

The exact foundations that the rest of Un-Banished builds on. Each part is a port of an engine
function of Banished 1.0.7, and gives the same bits as the original:

| Module | What it contains |
|---|---|
| `crt` | The C runtime's single-precision maths: `sinf`, `cosf`, `acosf`, `atan2f` |
| `math` | Vector helpers, headings, and the engine's `Transform` (a quaternion and an offset) |
| `random` | The game's random number generator, a Mersenne Twister (MT19937) |
| `noise` | The Perlin noise of the terrain generator |
| `scheduler` | Game time (integer microseconds) and the entity manager's update buckets |
| `calendar` | The game's year counter |
| `picking` | Ray–triangle intersection for selecting objects |

The crate has no dependencies. It knows nothing about the game's rules or its data files.
`banished-game` re-exports every module, so `banished_game::random` and `banished_core::random`
are the same module.

## Why it exists

Banished's simulation depends on float rounding. For example, the map generator adds noise
values and the result decides where a river runs. Rust's `f32::sin` and the C runtime's `sinf`
can differ in the last bit, and that is enough to change a map. So simulation code in
Un-Banished uses `crt` and `math`, never Rust's own float functions.

## Use it

```toml
[dependencies]
banished-core = { path = "crates/core" }
```

### Random numbers

`Random` gives the same numbers as the game for the same seed. A new game, the map generator and
the citizens' names all start from one.

```rust
use banished_core::random::Random;

let mut rng = Random::new(1234);
let a = rng.rand();                // any u32
let b = rng.rand_below(10);        // 0 to 9
let c = rng.rand_range(-5, 5);     // -5 to 5, both included
let d = rng.rand_tol(3);           // -3 to 3
let e = rng.rand_float();          // 0 to 1 (1 not included)
let f = rng.rand_float_tol();      // -1 to 1 (-1 not included)
```

The generator's state is the same as the engine object: an index and 624 words. A save stores
it, and `Random::from_parts(index, state)` restores it.

To check that a port draws numbers in the same order as the original, record the draws with
`random::trace` (see [Emulate for exact numbers](https://tanepiper.github.io/unbanished/develop/guides/emulate)):

```rust
use banished_core::random::{trace, Random};

trace::start();
let mut rng = Random::new(7);
rng.rand_below(3);
let draws = trace::take();          // each draw, with its kind and the Rust caller
assert_eq!(draws.len(), 1);
```

### Exact float maths

```rust
use banished_core::{crt, math};

let s = crt::sinf(0.5);                    // the C runtime's sinf, bit for bit
let angle = crt::atan2f(1.0, 2.0);
let unit = math::normalize3([3.0, 0.0, 4.0]);
let heading = math::heading(0.0, 1.0);     // a 2D direction, in degrees
```

`math::Transform` is the engine's transform: a rotation (quaternion) and an offset.
`Transform::of_model(yaw, position)` builds the transform of a placed model, and `apply`,
`rotate` and `inverse` use it.

### Perlin noise

```rust
use banished_core::noise::PerlinNoise;

// persistence, octaves, and the two seeds.
let noise = PerlinNoise::new(0.5, 4.0, 17, 31);
let value = noise.generate(12.5, 40.25);
```

### Game time

The engine counts time in integer microseconds (`System::Time`). Convert to and from seconds as
the original does:

```rust
use banished_core::scheduler::{seconds, ticks};

let t = ticks(1.5);          // 1,500,000
let s = seconds(t);          // 1.5
```

`scheduler::EntityScheduler` reproduces how the game spreads entity updates over frames: five
buckets, each with a quota of entities per frame (`QUOTAS = [50, 15, 8, 5, 2]`).

### The calendar

```rust
use banished_core::calendar::Calendar;
use banished_core::scheduler::ticks;

let mut calendar = Calendar::default();   // year 1
calendar.advance(ticks(3600.0));          // one year of game time
assert_eq!(calendar.year, 2);
```

## The API

The full API documentation is at
<https://tanepiper.github.io/unbanished/api/banished_core/index.html>.

## Tests

The parity tests for this crate are in `crates/game/tests/` (for example `crt_math.rs`,
`transform.rs`, `time_emu.rs` and `picking_emu.rs`). They compare the ports with output recorded
from the original functions in the emulator, and skip when that output is missing.

```bash
cargo test --release -p banished-core
cargo test --release -p banished-game --test crt_math
```

## Related documents

- [Exact maths and random numbers](https://tanepiper.github.io/unbanished/topics/exact-maths)
- [The emulator harness](https://tanepiper.github.io/unbanished/topics/emulator)
