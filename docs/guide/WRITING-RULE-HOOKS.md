# Writing a rule hook

A rule hook is a small program that runs inside Un-Banished after every update of the town. It
can read the date, the number of citizens and the stock, roll the game's dice, and post lines to
the event log. This page builds a complete example, a harvest festival every autumn, from an
empty folder to a line in your town's event log.

Rule hooks are an Un-Banished extension: Banished 1.0.7 has nothing like them, and a vanilla run
(`--vanilla`) never loads them. How they work inside the game is in
[Rule hooks](../develop/RULE-HOOKS.md).

## What you need

- Rust, with the WebAssembly target: `rustup target add wasm32-unknown-unknown`.
- Un-Banished on the desktop. The browser version does not load rule hooks yet.

Any language that compiles to WebAssembly works, as long as the module uses no floating point.
This guide uses Rust.

## The example: a harvest festival

The whole mod is in the repository, in `examples/rule-hooks/harvest-festival`. On the first
update of Autumn, if the town has people and food, it posts "The town holds its harvest
festival." One year in four, decided by the game's own dice, it adds a second line about a
bountiful harvest.

### 1. The project

```bash
cargo new --lib harvest-festival
```

In `Cargo.toml`, make it a WebAssembly library, and keep it small:

```toml
[lib]
crate-type = ["cdylib"]

[profile.release]
opt-level = "s"
lto = true
panic = "abort"
```

### 2. The game's functions

A rule hook calls the game through functions in the import module `unbanished`. Declare the ones
you use:

```rust
#![no_std]

#[link(wasm_import_module = "unbanished")]
extern "C" {
    fn time_in_year() -> i64;
    fn citizens() -> i32;
    fn stock(slot: i32) -> i32;
    fn random_below(n: i32) -> i32;
    fn post_event(severity: i32, text: *const u8, len: i32);
}
```

`#![no_std]` keeps the module small and keeps out anything the game does not give, such as
files or the clock. The full list of functions is in
[Rule hooks](../develop/RULE-HOOKS.md#api-version-1).

### 3. Say which API you were made for

Every rule hook exports `api_version`. The game refuses a hook made for another version:

```rust
#[no_mangle]
pub extern "C" fn api_version() -> i32 {
    1
}
```

### 4. Do something after each update

The game calls `after_iteration` after every update, 60 times a second of game time. Game time is
in microseconds: a month is 300,000,000 and one update is 16,666. The game's
twelve months run from Early Spring (0) to Late Winter (11); Autumn is month 7, so its first
update starts at 7 × 300,000,000:

```rust
const FOOD: i32 = 5;
const MONTH: i64 = 300_000_000;
const AUTUMN: i64 = 7 * MONTH;
const ITERATION: i64 = 16_666;

fn post(severity: i32, text: &str) {
    unsafe { post_event(severity, text.as_ptr(), text.len() as i32) }
}

#[no_mangle]
pub extern "C" fn after_iteration() {
    let now = unsafe { time_in_year() };
    if !(AUTUMN..AUTUMN + ITERATION).contains(&now) {
        return;
    }
    let (food, people) = unsafe { (stock(FOOD), citizens()) };
    if people == 0 || food == 0 {
        return;
    }
    post(1, "The town holds its harvest festival.");
    if unsafe { random_below(4) } == 0 {
        post(4, "A bountiful harvest: the festival goes on into the night.");
    }
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    core::arch::wasm32::unreachable()
}
```

Severity colours the line as the game's own events: 0 white, 1 yellow, 2 blue, 3 red, 4 orange.

### 5. Build it and put it in place

```bash
cargo build --release --target wasm32-unknown-unknown
```

Copy `target/wasm32-unknown-unknown/release/harvest_festival.wasm` into the `Hooks` folder of
your Banished data, beside `Save`:

| System | Folder |
|---|---|
| Windows | `Documents\Banished\Hooks` |
| macOS and Linux | `~/Documents/Banished/Hooks` |

Start a town, or load one. The game's log, in the terminal you started it from, says
`rule hooks: harvest_festival`. When Autumn comes, the festival is in the event log.

## The rules

The game checks the first three when it loads a hook, and refuses the hook, with the reason in
the log, if one is broken. Then the town runs with no rule hooks at all.

1. **No floating point.** Use integers only (`i32`, `i64`). Floats can give different results on
   different computers.
2. **Only the game's functions.** A hook may import nothing but the `unbanished` functions: no
   files, no clock, no network.
3. **Say the API version** with `api_version`.
4. **Keep nothing between calls.** A save does not keep a hook's memory. A hook that counts
   something in a global gives other results after a save is loaded or when a player joins.
   Decide everything from what the game gives you each time.
5. **Keep it short.** Each call may run about two million instructions. A hook that runs longer
   is stopped and switched off for the rest of the game.

### Random numbers

Use `random_below` for every random choice. It rolls the game's own dice, so every player in a
multiplayer game, and every replay, rolls the same. Each roll also moves the dice on, so a hook
that rolls changes everything that happens after it: which tree is cut, who is born. That is
expected; it happens the same way on every computer.

## Saves and multiplayer

- A save made with rule hooks lists them among its mods, by name and checksum. The original game
  reports them as missing mods.
- In multiplayer every player needs the same `Hooks` folder: the same files, with the same names.
  A player whose hooks differ from the host's is not let into the room, and is told:
  "this game's rule hooks differ from the host's: use the same Hooks folder".

## Trying a hook quickly

To see what a hook does to a town, play a save for a while with and without it and compare the
digests:

```bash
target/release/unbanished --sim-digest save-game/MyTown.sav 1800 600 game-data/original/WinData
```

The same save with the same hooks always gives the same digests. A hook can also be written in
the WebAssembly text format and turned into a `.wasm` file with the workspace's small tool:

```bash
cargo run -p unbanished-hooks --example wat2wasm my-hook.wat my-hook.wasm
```

`tools/test/hooks/every-minute.wat` is a short example.
