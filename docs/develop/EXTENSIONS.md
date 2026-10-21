# Extensions: what Un-Banished adds, and how to leave it out

Un-Banished is two things. The **core** is Banished 1.0.7, rebuilt one to one: the simulation
and its original interface, drawing and sound. An **extension** is anything Un-Banished adds
that the original does not have, such as the probe or the Nexus Mods browser. This page lists
the extensions, shows how to build the game without them, and explains how to write a new one.
The plan behind it is in [ARCHITECTURE-PLAN.md](../ARCHITECTURE-PLAN.md).

Three rules hold for every extension:

1. **It can be left out.** Each extension is a cargo feature of the game (`apps/game`). All of
   them are on by default. With none of them, the game builds and plays exactly as with all of
   them: the `vanilla` test suite checks this.
2. **It never changes the core's results.** An extension may read the game and draw on the
   screen. It may send the game a player's command, as a player would. It may not change a rule,
   a number or the order of random draws. Anything that does is a `Deviation:` and only happens
   in games where the extension is in use.
3. **The core never depends on it.** The simulation crate, `unbanished-sim`, depends on no
   drawing, window, interface, sound or network crate. The `checks` test suite fails if it does.

## The extensions

| Feature | What it adds | Where the code is |
|---|---|---|
| `cinematic` | The camera for capture videos (`--camera`, `--camera-follow`), native only | `apps/game/src/view/cinematic.rs` |
| `probe` | The probe: read and drive a running game (`--probe`, the browser's `window.unbanished`) | `apps/game/src/probe.rs`, `apps/game/src/gameplay/probe.rs`, [PROBE.md](PROBE.md) |
| `mod-sites` | A Browse tab for each mod site (Nexus Mods, and banishedinfo.com natively) in the Mods window | `apps/game/src/ui/mod_browse.rs`, `apps/game/src/platform/nexus.rs`, `crates/nexus`, `crates/banishedinfo` |
| `replay` | Recordings: `--record` writes a game's commands and digests, `--replay` plays them back | `apps/game/src/gameplay/recording.rs`, `crates/session` |
| `multiplayer` | Lockstep games through a relay, the Multiplayer window and its main-menu button, and their tests (`--lockstep-test`, `--net-test`, the browser's `?net=`). It turns on `replay` | `apps/game/src/gameplay/{lockstep,network}.rs`, `apps/game/src/ui/multiplayer.rs`, `crates/session`, `crates/net` |

The browser's start page and its manager are the web page itself, so they are not a cargo
feature.

## Building without extensions

Leave all of them out:

```bash
cargo build --release -p unbanished-app --no-default-features
```

Leave out one, and keep the others: name the ones to keep.

```bash
cargo build --release -p unbanished-app --no-default-features --features "cinematic,probe"
```

The same flags work for the browser build (`--target wasm32-unknown-unknown`). Without
`mod-sites`, the Mods window hides its Browse tab, as the original's build without Steam does.
Without `probe`, `--probe` does nothing and `window.unbanished` answers that the game was built
without it.

## Pure Banished: every extension off while the game runs

A build with extensions can still play without them. Start it with `--pure` (in the browser,
add `?pure` to the address) and the run is Pure Banished: only the core and the original's own
mods (`.pkm` files). Each extension's door asks `platform::mode::pure()` once and stays shut:

| Extension | In a Pure Banished run |
|---|---|
| `multiplayer` | The main menu has no Multiplayer button |
| `mod-sites` | The Mods window hides its Browse tab, as without the feature |
| `probe` | Every command answers `Pure Banished: the probe is off` |
| `replay` | `--record` records nothing |
| `cinematic` | `--camera` and `--camera-follow` do nothing |

The test commands (`--replay`, `--lockstep-test`, `--net-test`) are not play, so they still
run. A new extension checks `pure()` at its door too.

## The doors into the game

An extension reaches the game only through a few doors. Each is small and owned by the core
or the game app. Use one of these. If none fits, add a new door to this list, rather than
reaching into the game's code.

| Door | What it is for | Where |
|---|---|---|
| Player commands | Do what a player does: every change goes through one `PlayerCommand` | `unbanished_sim::command`, `GameplayLoop::execute` |
| Step events | See what an iteration did: models changed, sounds, terrain reshaped | `unbanished_sim::simulation::StepEvents`, `FrameEvents` |
| Timing | Time the parts of a step (the frame profiler uses it) | `unbanished_sim::timing` |
| World queries | Read the town as JSON (the probe uses it) | `GameLoop::probe_world` |
| Camera | Draw a frame from another camera (the cinematic camera uses it) | `Game::cinematic` |
| Mods window tabs | Add tabs to the Mods window (the mod sites use it) | `apps/game/src/ui/mod_sites.rs` |
| The game loop's extensions | A multiplayer game's and a recording's state, and the few questions the game loop asks them (begin an update, route a command, record it) | `apps/game/src/gameplay/extensions.rs` and its `extensions/` files |
| Sessions | Decide when a player's commands are carried out: at once, or in their lockstep slot | `unbanished_session::session::Session` |
| Strings | Give the simulation the game's text in the player's language | `unbanished_sim::strings::Strings` |

## Example: a new extension

Say you want to log how long each frame's simulation takes. It only reads the clock, so it
cannot change the game. Here is the whole extension.

1. Add a feature in `apps/game/Cargo.toml`, and add it to `default` if it should be on:

   ```toml
   [features]
   default = ["cinematic", "mod-sites", "probe", "sim-timing"]
   # Logs how long the simulation's parts take each frame.
   sim-timing = []
   ```

2. Put the code in its own file, `apps/game/src/platform/sim_timing.rs`, and use the Timing door:

   ```rust
   //! Logs how long the simulation's parts take (the `sim-timing` extension).

   struct Logged(&'static str, std::time::Instant);

   impl Drop for Logged {
       fn drop(&mut self) {
           log::debug!("{} took {:?}", self.0, self.1.elapsed());
       }
   }

   /// Starts logging the simulation's parts on this thread.
   pub fn start() {
       unbanished_sim::timing::set(|name| Box::new(Logged(name, std::time::Instant::now())));
   }
   ```

3. Declare the module only with the feature, in `apps/game/src/platform/mod.rs`, and call
   `platform::sim_timing::start()` once at start-up (in `main`), also only with the feature:

   ```rust
   #[cfg(feature = "sim-timing")]
   pub mod sim_timing;
   ```

4. Test it:

   ```bash
   tools/test/run.py features      # the game builds without it, with no warnings
   tools/test/run.py vanilla       # without it, every scenario plays exactly as with it
   ```

Keep an extension's code in its own files. A `#[cfg(feature = "...")]` belongs on the module and
on the one line that calls into it, not spread through the game's code. If you need many of
them, the extension needs a door: add one, as `mod_sites.rs` does for the Mods window. Another
way is a stand-in: `ui/multiplayer_off.rs` has the Multiplayer window's type with no value, so
the main menu's code is the same in both builds and the window simply never opens.

## Testing

| Suite | Tier | What it checks |
|---|---|---|
| `checks` | quick | `unbanished-sim` depends on no drawing, window, interface, sound or network crate |
| `features` | standard | The game builds with no extensions, natively and for the browser, with no warnings |
| `vanilla` | long | The build with no extensions plays every compatibility scenario exactly as the full build: the same saves and the same frames |

See [TESTING.md](TESTING.md) for how to run them.
