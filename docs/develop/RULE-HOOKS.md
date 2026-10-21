# Rule hooks

A rule hook is code from a mod that changes the game at a fixed point of each update. Banished
1.0.7 has nothing like it: its mods change data (models, resources, numbers), never the code that
runs the town. Un-Banished adds rule hooks as an extension, behind the `rule-hooks` feature, and
runs them from WebAssembly plugins with [wasmi](https://github.com/wasmi-labs/wasmi). This page
explains how they work inside the game and what keeps them deterministic. To write one, read
[Writing a rule hook](../guide/WRITING-RULE-HOOKS.md).

Deviation: a game with rule hooks plays differently from the original. With no plugins (the
default) and always in Pure Banished (`--pure`), the step is exactly the original's.

## Where a hook runs

| Point | When | Where in the code |
|---|---|---|
| `after_iteration` | At the end of every iteration of `Simulation::iterate`: after the town, the calendar, the weather and the stock count, before the iteration's step events go to the presentation | `crates/sim/src/simulation/step.rs` (`run_hooks`) |

The game runs 60 iterations a second of game time, more at higher speeds. A hook runs once per
iteration, so it sees every iteration whatever the speed.

## The door in the core

`unbanished_sim::hooks` holds the door. It knows nothing of WebAssembly.

| Item | What it is |
|---|---|
| `RuleHooks` | The trait the simulation calls: `after_iteration(&mut HookContext)` |
| `HookContext` | What a hook may read and do: the year, the month, the time into the year, the citizens alive, the stock slots, a random number from the game's stream, and posting an event |
| `Simulation::hooks` | `Option<Box<dyn RuleHooks>>`, `None` unless the game set one |
| `saved_entry`, `is_saved_hook` | A save's entry for a hook, among the mods it lists |

`unbanished-hooks` (`crates/hooks`) implements `RuleHooks` with WebAssembly plugins. The game app
loads them at the start of a town (`apps/game/src/platform/rule_hooks.rs`).

## The plugins

A plugin is a `.wasm` file in the `Hooks` folder of the player's data (beside `Save`, in
`Documents/Banished`, or the folder `UNBANISHED_USER_DIR` names). The game loads every `.wasm`
file there, in name order, and runs them in that order. The browser version does not load
plugins yet.

When a plugin is loaded, the game checks it, and refuses it with a reason in the log:

| Check | Why | The reason in the log |
|---|---|---|
| No floating point | Float results can differ between computers and between wasm engines | "uses floating point, which rule hooks may not" |
| Imports only the game's functions (module `unbanished`) | A plugin may not reach files, the clock, the network or anything else that differs between computers | "a rule hook may import only `unbanished` functions" |
| Exports `api_version` returning 1 | A plugin made for another version of the API would read the wrong things | "made for rule-hook API 2; this game gives API 1" |

If any plugin is refused, the town runs with no rule hooks at all, so it never runs with half of
a folder.

### API version 1

All functions are in the import module `unbanished`. Every value is an integer.

| Function | Signature | Returns |
|---|---|---|
| `year` | `() -> i32` | The year, from 1 |
| `month` | `() -> i32` | The month, 0 (early spring) to 11 |
| `time_in_year` | `() -> i64` | Game time into the year, in microseconds: a month is 300,000,000, a year 3,600,000,000, one iteration 16,666 |
| `citizens` | `() -> i32` | The citizens alive |
| `stock` | `(slot: i32) -> i32` | The Town Hall's count of a stock slot, or -1 for a slot that does not exist. Slots: 0 wood, 1 stone, 2 iron, 3 firewood, 4 tools, 5 food, 6 herbs, 7 clothes, 8 alcohol, 9 coal, 10 textile, 11–20 the mods' own |
| `random_below` | `(n: i32) -> i32` | A number from 0 to `n - 1` from the game's own random stream (0 when `n` is below 1) |
| `post_event` | `(severity: i32, text: *const u8, len: i32)` | Adds a line to the event log. Severity 0 to 4 colours it as the original's events: white, yellow, blue, red, orange. The text is UTF-8, at most 256 bytes, read from the plugin's exported `memory` |

A plugin exports:

| Export | Needed | What it is |
|---|---|---|
| `api_version() -> i32` | yes | The API version it was made for: 1 |
| `after_iteration()` | no | Called after each iteration |
| `memory` | when it posts events | Its linear memory |

## What keeps it deterministic

Every computer in a multiplayer game, and every replay of a recording, must run a hook to the
same result.

- **Integers only.** wasmi is set up with floats switched off (`Config::floats(false)`), so a
  module that uses any float instruction does not load.
- **The game's random stream.** `random_below` draws from the town's own `Random`. During a call
  the stream is moved into the plugin's store and moved back after it, so a hook's draws come in
  the same place of the stream on every computer. Each draw moves the stream on, so a hook that
  draws changes everything that happens after it, exactly as the original's own draws do.
- **A fixed order.** Plugins run in name order. A plugin's events are posted after its call, in
  the order it posted them.
- **A fixed budget.** Each call has 2,000,000 units of wasmi fuel (about one per instruction).
  A plugin that runs out stops at the same instruction on every computer; the draws it made
  before that stay made. A plugin that runs out, or traps, is switched off for the rest of the
  run, with the reason in the log. Its memory is capped at 16 MiB.
- **wasmi's `deterministic` feature**, and the portable dispatch, so the interpreter is the same
  on every platform.

A plugin's memory is not kept in saves. A plugin that remembers something between calls (a
counter in a global, say) gives other results after a save is loaded, or when a player joins a
running game. A plugin must decide everything from what the game gives it in each call.

## Saves

A save made with rule hooks lists each one among its mods, as `Hooks/NAME.wasm` named
"Rule hook: NAME", with the plugin's CRC-32. The original game reports these as missing mods,
which is right: the town was played with them. Un-Banished's own load dialog does not, since the
plugins load from their folder, not from the mod list.

## Multiplayer

`Hello` (protocol version 4) carries `hooks`: a fingerprint of the plugins (a CRC over each
one's name, length and bytes, in order; 0 with none). A room accepts only games whose fingerprint
matches the host's, and tells a player who differs:

> this game's rule hooks differ from the host's: use the same Hooks folder

The server's own game (`simulate`) loads no plugins, so a room with rule hooks is not simulated
by the server; its players' checks still work, by majority.

## Pure Banished

`--pure` (or `?pure` in the browser) loads no plugins, whatever the `Hooks` folder holds.

## Checks

| Check | What it proves |
|---|---|
| `crates/hooks/src/tests.rs` | A plugin draws from the game's stream and posts events; two runs from one seed agree; floats, other imports and other API versions are refused; a plugin with no end runs out of fuel and is switched off; the fingerprint follows the plugins and their order; the example mod celebrates on the first iteration of October and nowhere else |
| `tools/compat/hooks.sh` (the `hooks` suite) | With a test plugin, a save gives other digests than without it, the same ones twice, and the same as without it under `--pure`; two players with it stay identical through a server; a player without it is refused, with the reason |
| `tools/compat/compare.sh` | With no plugins, the game plays every scenario exactly as before |

## Open questions

- The browser version loads no plugins. It could take them from the browser's storage, as it
  does `.pkm` mods.
- Only one point of the step has a hook. Others (when a citizen is born, when a building is
  finished) would need the same care: a fixed place in the step and in the random stream.
- A hook can only read and post events. Changing the town (adding stock, a citizen's health)
  needs commands a hook may give, carried out at a fixed point.
