# Changelog

All notable changes to Un-Banished are listed here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/). Before 1.0 there are no compatibility promises.

The version is the engine's own (`banished --version`). The Banished version in the main menu
(1.0.7, build 170910) is the original game's and does not change.

## [Unreleased]

Boomtown: large towns run about twice as fast and the GPU does half the work, with no change
to the game's results; mods from Nexus Mods and a mod manager; a new loading screen in the
browser; and a blog. The performance work is described in
[docs/PERFORMANCE.md](docs/PERFORMANCE.md).

### Changed

- The engine version is now 0.2.0 (shown as `0.2.0-<build>` until the release).
- No freeze when the town appears after loading: the ambient sounds, the first music track
  and the decals are now prepared behind the loading screen (about 0.5 s on a large town).
- Faster loading on computers with more than one CPU core (native builds): resources, sounds,
  textures, meshes and the terrain's surface image are decoded on all cores. Starting the game,
  opening the main menu, starting a new game and loading a save take 20–55 % less time. The
  browser build still loads on one thread.
- Large towns run about twice as fast. On a town of 1,024 citizens and 28,000 objects at speed
  10, a frame took 94 ms and now takes 43 ms; the simulation part fell from 78 to 29 ms. The
  game's results are unchanged: saves and frames are identical to before.
- The GPU draws about half as much per frame on a large town. Models and terrain tiles that a
  pass cannot see (the shadow map, the water reflection or the screen) are no longer drawn.
  What is drawn is unchanged.
- Rocks and ore further than 50 to 70 units from the camera are drawn with their simpler
  model, as in the original. Before, they always used the detailed one. Nothing else in the
  picture changes.
- The code is reorganised into smaller parts, with no change to the game. The bit-exact
  foundations, the terrain and path finding are now their own crates (`banished-core`,
  `banished-terrain`, `banished-path`); `banished-game` and the game app group their modules
  into folders by topic; the largest files (`gameplay.rs`, `town.rs`, `start.rs`, `agents.rs`,
  `render.rs`, `building_ui.rs` and the resource readers in `banished-formats`) are split by
  topic, and large test modules live in their own files.

### Added

- A mod manager, `banished-mods`: turn mods on and off and set their order before the game
  starts, keep named mod lists, install mods from Nexus Mods, and start the game.
- Optional Nexus Mods support. With your own Nexus Mods API key, the Mods menu's Browse tab
  (the Steam version's Workshop browser) searches and installs Banished mods from Nexus Mods,
  and `banished-cli nexus` does the same from the command line. See "Mods from Nexus Mods" in
  the user guide.
- `tools/compat/`: build any version in a clean checkout, play a fixed set of scenarios (a new
  game, small, medium and large towns, and continuing another version's saves), and check that
  two versions produce identical saves and frames. See
  [docs/COMPATIBILITY.md](docs/COMPATIBILITY.md).
- A `speed N` command for headless captures, and the `savegame_same` and
  `savegame_fields_diff` tools for comparing saves.
- Documentation: the website has new Guide, Develop, Topics and Reference sections. There is a
  README for every app and crate, with tested examples; guides to tracing, emulating, porting and
  verifying; pages on how the game's parts work; and the Rust API documentation at `/api/`.
- The Rust API documentation is written for readers: plain English, every public item
  documented except some struct fields, each crate's README as its first page (its examples run
  as doc tests), and the website's colours. Each ported item still names its original function
  in one last line; offsets and other evidence are in ordinary comments
  (`tools/triage/doc_comments.py` checks this).
- A new loading screen for the browser version: the Un-Banished logo over a scene from the
  game, three short steps to load the game files (where to find `WinData`, a large Choose files
  button, mods), a progress line, and links to the setup guide, problem reports and the source
  code.
- The website's home page shows a 64-second montage of Un-Banished in motion in place of the
  logo: a flight down a river, the trader's boat, a fisher, the market, people at work close
  up, towns in autumn and winter, and a year of seasons. Un-Banished recorded it itself
  (`tools/videos.sh SAVE.sav montage`), and the site's other videos are recorded at
  1920 × 1080 with the highest video settings.
- Documentation for changes meant to change the picture: how to predict what will change and
  prove with `compare.sh`, `imgdiff` heat maps and close-ups that nothing else did
  ([docs/develop/VISUAL-CHANGES.md](docs/develop/VISUAL-CHANGES.md)), with example heat maps
  and close-ups made by `tools/screenshots.sh --visual-change BASE NEW`. The tracing guide now
  explains how to follow a field to where it is written, and the building guide how to work in a
  git worktree.
- A blog on the website with release notes told as stories, with pictures and videos:
  "First Settlers" (0.1.0) and "Boomtown" (0.2.0).
- A cinematic camera for headless captures, separate from the game camera: keyframed paths
  (`--camera EYE>TARGET@T`) and a camera that rides with a citizen, a boat or an animal
  (`--camera-follow`), keeping its view clear of roofs and trees. New videos of people at work,
  the trader's boat and a river flight (`tools/videos.sh`: farmer, labourer, vendor,
  fisher-closeup, boat-shoulder, river-flight). The game, the game camera and saves do not
  change.
- Captures can hold camera keys (`--hold RotateLeft@START-END`) and follow a citizen
  (`select citizen N`, `select profession NAME N`, then `--press buttonFollow`). New
  `tools/videos.sh` modes: `big-town`, `orbit`, `follow-citizen`, `river` and `fishing`.
- Short videos of the game, recorded headlessly with `tools/videos.sh` (see
  [docs/develop/VIDEOS.md](docs/develop/VIDEOS.md)), and more screenshots of the game and the
  viewer.

### Fixed

- Trader boats were not drawn: merchants arrived and traded, but their boat was invisible on
  the river. The boat is now drawn with its hull, cargo, oars and merchant.
- Headless captures could save on a different frame from run to run, because they waited a
  varying number of frames for the save's preview image. They now wait for it, so two runs save
  the same state.

## [0.1.0] - 2026-10-19

The first release: the whole game runs, natively and in the browser, on the player's own copy
of Banished 1.0.7.

### Added

- **Engine.** A new engine in Rust with wgpu. It runs natively on Windows, macOS and Linux, and
  in web browsers through WebAssembly (WebGPU, or WebGL 2 where WebGPU is missing). It reads the
  original `.pkg` packages and contains no original code, art, sound or text.
- **Game files.** Readers for the packages, all 170 resource types, meshes, textures, materials,
  dialogs, sounds (including a WMA decoder) and save games.
- **The game.** New games and saved games; map generation that matches the original bit for bit;
  citizens, households, needs, health and ageing; jobs, professions and production; farming,
  orchards, pastures, hunting, fishing and gathering; construction, roads, bridges and tunnels;
  trade and nomads; disasters (fire, disease, tornadoes); weather and seasons.
- **Interface.** The original's menus, toolbar, building and citizen windows, utility windows,
  event log, notifications, map window, tooltips, options and key bindings.
- **Saves and mods.** The original save format and save folder: towns saved by the original load
  in Un-Banished. Autosave. BanishedKit mods (`.pkm`) through the Mods menu.
- **Tutorials, scenarios and achievements**, as in the original.
- **Audio.** Music, ambience, positional sound effects and the UI sounds.
- **Website.** https://tanepiper.github.io/unbanished/, with a setup guide, how the project was
  made, and a Play page that runs the game in the browser.
- **Research and tools.** Notes on the original's behaviour in `docs/`, with every ported
  function cited; an emulator harness that checks the port against the original's own
  functions; a frame profiler (`BANISHED_PROFILE`) and profiling scripts.

### Fixed

- A construction site could not be selected (#1), and an orchard could not be selected once its
  trees grew (#2).
- Changes in building windows did not show while the game was paused (#3).
- After loading a save, citizens who were walking stood still, so workers did not return to a
  stopped and restarted workplace (#4).
- The browser version ran out of memory in Firefox with WebGL (#7).
- The game paused for about 50 ms every few seconds (#5); per-frame costs and model texture
  memory are also lower.

### Known issues

- There are no ready-made downloads; you build Un-Banished from source (see the setup guide).
- A few behaviours still differ from the original. They are listed in
  [docs/DEVIATIONS.md](docs/DEVIATIONS.md) and in the
  [issues](https://github.com/tanepiper/unbanished/issues).

[Unreleased]: https://github.com/tanepiper/unbanished/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/tanepiper/unbanished/releases/tag/v0.1.0
