# Changelog

All notable changes to Un-Banished are listed here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/). Before 1.0 there are no compatibility promises.

The version is the engine's own (`banished --version`). The Banished version in the main menu
(1.0.7, build 170910) is the original game's and does not change.

## [Unreleased]

### Added

- The browser version keeps the game files you choose, in this browser only, so you choose them
  once (#17). A checkbox under **Choose files** turns it off. With the files kept, the start page
  shows **Start the game**, a **Help** button for the steps, and a manager with three tabs: the
  kept game files (and **Forget**), the mods (on and off, their order, remove and add) and the
  saves (download, delete and import `.sav` files).

- `tools/videos.sh SAVE.sav filters` shows the orbit video in all four looks side by side: as
  drawn, sepia, grey and psychedelic. New videos for the website: the river flight in sepia and
  the trader's boat in grey.

## [0.3.0] - 2026-10-20

Multiplayer: several players play one town together, on the desktop or in the browser, through
a small relay, while single-player games stay exactly as before. Also mods from banishedinfo.com,
a browser version that installs as an app, and the fix for the first tutorial.

### Changed

- Dialogs lay out faster: each element is measured once per frame instead of again for every
  level of nesting. The Mods menu with both Browse tabs went from 70 ms to 7 ms a frame. Every
  dialog looks exactly as before.
- Mod sites in the Mods menu: a Browse tab for each site, shown by default: Nexus Mods and, on
  the desktop, banishedinfo.com (it sends no CORS headers, so the browser cannot reach it).
  Searching needs no account; installing from Nexus Mods needs your own API key.

### Fixed

- The first tutorial (Getting Started) stopped at its first house: clicking the Housing menu
  opened it, but the tutorial did not see the click, so the Wood House stayed greyed out (#11).
  A click on a toolbar menu button, or its key (F3), now counts. Every earlier version, in the
  browser and on the desktop, has this problem.
- The browser version: the interface is sharper on high-resolution screens with a fractional
  pixel ratio, such as 4K tablets and Samsung DeX: every image and letter now starts on a whole
  screen pixel (#12). Full screen works in the game's own tab (Options, Fullscreen), and the
  Play page's full-screen frame fills the screen (#14).

### Added

- Multiplayer from the main menu: a Multiplayer button opens a window to host a room on a relay
  or join one with its code. The host starts the game from one of its saves, and every player
  plays the same town; the speed and the pause are shared, and a line at the foot of the screen
  shows the players (step 6). The button is not in the original game.
- Multiplayer: while the game waits for another player, the camera and the windows keep
  working, and the line at the foot of the screen names the player it waits for. A player who
  leaves no longer stops the game: the others play on without them. A command given while the
  game waited was lost; it now goes into the next turn.
- A multiplayer relay, `banished-relay`, with room codes, and the game's WebSocket connection to
  it, natively and in the browser (`banished-net`). Native and browser players have played one
  game through it and stayed identical (step 5).
- Lockstep, the core of multiplayer: several players' games stay identical by
  exchanging only their commands. `banished --lockstep-test` runs players in one process over a
  simulated network (step 3). The host compares the players' games every 600 updates and, when
  they differ, everybody reloads the host's save and goes on (step 4).
- Mods from banishedinfo.com on the desktop: browse, search and install them in the game's
  Browse tab, in the mod manager and with `banished-cli banishedinfo`.
- Mod previews show in the Browse tabs and the mod manager: PNG, JPEG and WebP (Nexus Mods
  sends WebP).
- The browser version can be installed as an app (Chrome, Edge, Android), and then opens in its
  own window and starts without a network (#13).
- The browser version's Options choose a resolution for the 3D world (100, 75 or 50 % of the
  page) and a frame-rate limit (30 to 120 per second) (#15). Captures can test the resolution
  natively with `--scene-scale PART`.
- Recordings: `--record FILE` writes every player command of a game loaded from a save, and
  `banished --replay SAVE FILE` plays it back without a window and checks that the game comes
  out the same. Every player action now goes through one command type, with no change to the
  game (steps 1 and 2 of [docs/MULTIPLAYER-PLAN.md](docs/MULTIPLAYER-PLAN.md)).
- `banished --sim-digest SAVE` and the browser build's `?digest=` run a save without a window
  and print digests of the game state, to check that native and browser builds simulate
  identically. They do, on the saves tried ([docs/CROSS-PLATFORM.md](docs/CROSS-PLATFORM.md)).
- Test tools: a `tools/compat` scenario that plays through the interface (`actions`), and
  `replay.sh`, `lockstep.sh`, `network.sh` and `multiplayer.sh`. Captures can press and release
  the mouse (`--mouse-down`, `--mouse-up`), run in real time (`--realtime`) and type into an
  edit box (`--text NAME=TEXT@SECONDS`).
- The home page montage has titles: it opens on "Un-Banished", names each shot, and ends on a
  card. `tools/videos.sh` adds titles with FFmpeg's `drawtext` when the installed FFmpeg has it
  (see Titles in [docs/develop/VIDEOS.md](docs/develop/VIDEOS.md)).

## [0.2.0] - 2026-10-19

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

[Unreleased]: https://github.com/tanepiper/unbanished/compare/v0.3.0...HEAD
[0.3.0]: https://github.com/tanepiper/unbanished/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/tanepiper/unbanished/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/tanepiper/unbanished/releases/tag/v0.1.0
