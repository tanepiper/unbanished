# Changelog

All notable changes to Un-Banished are listed here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/). Before 1.0 there are no compatibility promises.

The version is the engine's own (`banished --version`). The Banished version in the main menu
(1.0.7, build 170910) is the original game's and does not change.

## [Unreleased]

### Added

- The original Banished can run in the project's own emulated PC (`tools/reference/vm/`), so
  agents can watch it, drive it and make saves without a Windows PC.

## [0.5.0] - 2026-10-23

A town now runs in the original's order and keeps more of what the original saves. Each step
updates citizens, buildings, trees and animals in the original's order. A loaded game keeps
each character's animation. Many rules that drew random numbers in another order, or treated edge
cases differently, now give the original's results.

### Fixed

- Trees that spring up in the same moment a pile is left behind are no longer swapped with it in
  the game's update order. The tree was updated as a building site and the pile as a tree, and
  every entity made after them got the wrong slot.
- A walker starts moving one update later, when its path has come back, as in the original.
  Citizens and animals were one update early, so their steps and footstep sounds came early too.
- Destroyed buildings, picked-up piles and cleared resources leave the entity manager's
  turns at once, as in the original. Before, they kept their turn, so the rest of a town's
  buildings, trees and animals were updated less often the longer a game ran.
- Each citizen draws for its animation sounds just before it moves, as in the original,
  instead of all citizens' sounds first.
- A new game's deer get the same ages and lifespans as in the original. Each animal's lifespan
  is drawn when the animal is made, before its position and age. Livestock bought from a
  merchant are now up to half their lifespan old, not newborn.
- A new game's herds pick the same places as in the original: the starting barn and house hold
  their ground from the start, so deer are not placed under them.
- Saves of wild trees, rocks and herbs keep the original's "keep piles" flag set.
- A herd's deer alternate between male and female, as in the original. Every deer of a herd
  had the same sex before.
- The starting adults count as having finished school, as in the original.
- Saves link each citizen to its character and status icon, and new citizens and work actions
  get the original's default values.
- Standing deer and citizens keep the heading they started with, as in the original, instead
  of turning to face north.
- Saves of a new game hold each model's materials and the links between an entity's parts, as
  the original's do.
- Saves of fishing docks keep each fisher in their slot. Empty slots were moved to the front
  when the game was saved.
- Older saves (2015) load the citizens who were eating from their own hands; their actions were
  dropped before. Their models keep the rotations they were saved with.
- A healthy citizen's disease check no longer draws a random number when the odds are 0 or
  less, as in the original; this changes which random numbers later events get.
- A sick citizen passes the disease to one healthy citizen nearby at each spread, as in the
  original, instead of to every healthy citizen in range. Epidemics now grow more slowly.
- A citizen fetching food or firewood for home sets out with the same walking speeds as in the
  original. The port chose the goods before it made the walks, so the random numbers came in
  another order.
- Citizens' tools (axe, pick, hoe, hammer, anvil, saw) and the livestock make their sounds again.
  Each sound event draws a random number in the original, so towns now take the same draws.
- New wild herds take the same random numbers as in the original: the deer's model picks its
  variant before the animal's place and heading are drawn.
- A trading post's merchants get women's and men's names as in the original; the port had the
  two the wrong way round.
- Each simulation step runs in the original's order: everything that updates every step first
  (characters, buildings' states), then the citizens, then the deer population, then buildings and
  piles, then trees, crops and animals. The port ran trees and animals first and citizens last.
- A citizen who gets very hungry eats from their home's food first, as in the original, and only
  walks off to eat if still hungry.
- Vendors and traders carry 500 at a time, as in the original; every citizen's load now follows
  their profession. The port kept everyone at the citizen's 100.
- A loaded game keeps each character's animation: the clip it was playing, its time and the
  shared clocks the walks follow. Walks under way go on as saved, and men and women share the
  same walking clocks, as in the original. Saves write the animation back.

### Added

- More of the original measured under the emulator: every kind of Random call, the Perlin
  noise, vector normalising, the shuffles, the recipe pick, a citizen's health and happiness,
  an orchard's ripening and the seasonal values, NaN and edge cases among the inputs.
- `tools/triage/nan_jumps.py` lists a function's float comparisons and which way a NaN goes at
  each one; issue #25 tracks the review of every simulation function with it.
- `.cargo/config.toml` keeps every game started with cargo silent, and the browser game takes
  `&mute`.
- Tests, the compatibility tools and the browser tests are silent: `UNBANISHED_MUTE=1` keeps the
  game's sound at volume 0, and the test browsers start muted.
- Function labels made from the port's own citations (`labels/names-from-port.tsv`): the
  decompile tools now name 703 original functions, up from 226. The label files moved from
  `tools/triage/` to `labels/`.
- GameFrame's Claude Code plugin is enabled for the project: its skills, its 18 agents and its
  gates, which stop game material and decompiled code from reaching git. `tools/gf.sh` runs its
  tools from here, and `docs/CHECKLIST.md` gives the project's state in GameFrame's format.
- A comparison of the project with GameFrame's standard, and what GameFrame's tools found in the
  game's files ([docs/develop/GAMEFRAME.md](docs/develop/GAMEFRAME.md)).

## [0.4.1] - 2026-10-21

Ready-made downloads of the game for Windows, macOS and Linux. The program is now called
Un-Banished More, and `--vanilla` replaces `--pure`.

### Added

- Each release has ready-made downloads: a zip with the game for Windows, macOS (Apple silicon
  and Intel) and Linux.
- Extensions still in development are `unstable-*` cargo features, which no release contains.

### Changed

- The program is **Un-Banished More**: every stable extension, as before. `unbanished --version`
  says so.
- Pure Banished is now called vanilla: `--vanilla` (in the browser, `?vanilla`) replaces
  `--pure`, and plays the original game and its own mods only.

### Fixed

- A new citizen starts with the original's tool status (-1) and clothing status (4), instead of 0.
  The tool and coat achievements count these citizens as the original does, and saves hold the
  same values ([docs/CLASS-FIELDS.md](docs/CLASS-FIELDS.md)).

## [0.4.0] - 2026-10-21

Rule hooks: mods can change how a town plays, the same way on every computer. Multiplayer now
has a server: it keeps each room's town, lets players join a game that has started, and can run
the town itself. The desktop game can be the server. Also Pure Banished (the original game with
no extensions), a browser version that keeps your game files, and faster loading.

### Added

- Rule hooks: small WebAssembly programs in the `Hooks` folder beside your saves that change
  how the town plays, the same way on every computer. Saves list them, every player of a
  multiplayer room needs the same ones, and Pure Banished never loads them.
  [Writing a rule hook](docs/guide/WRITING-RULE-HOOKS.md) builds an example, a harvest festival
  every autumn.

- [How Un-Banished is built](docs/ARCHITECTURE.md): the core and its extensions, the layers,
  one update step by step, multiplayer and the server. It and the extensions guide are on the
  website.

- Multiplayer: the server can run each room's game itself (`simulate = DIR`, with the operator's
  own copy of the game data). Its game then decides every check, and a player whose town differs
  goes on from the server's.

- Multiplayer: the desktop game can be the server. Host with the server address left at
  `ws://127.0.0.1:7780`, and if no server runs there the game starts one and shows the address
  the other players enter.

- Multiplayer: a server can keep each room's town between games (its `rooms` setting names the
  folder). The host's game gives it the town every minute; a host who opens the room again later
  goes on from it.

- Multiplayer: a player can join a game that has already started, while its host is in it. A
  player who left can come back the same way. Everybody reloads the town as it is, and the new
  player starts from it.

- For contributors and agents: `tools/worktree-setup.sh` links the game data, saves, Ghidra
  analysis and Python environment into a git worktree in one step; `tools/re-setup.sh` checks
  and installs the reverse-engineering toolchain and writes `.env`, which every tool now reads
  instead of fixed install paths; `tools/ghidra/run.sh` runs one-off Ghidra jobs; `.mcp.json`
  starts REA in Claude Code; a `harness` test suite checks the tools and the skills; and the
  emulator gained C runtime stand-ins and clearer errors. The generic version of this harness,
  for any game, is https://github.com/tanepiper/reverse-engineer-games.

- **Pure Banished**: `unbanished --pure` (in the browser, `?pure` in the address) plays only the
  original game and its own mods, with everything Un-Banished adds turned off for that run.

- The browser version keeps the game files you choose, in this browser only, so you choose them
  once (#17). A checkbox under **Choose files** turns it off. With the files kept, the start page
  shows **Start the game**, a **Help** button for the steps, and a manager with three tabs: the
  kept game files (and **Forget**), the mods (on and off, their order, remove and add) and the
  saves (download, delete and import `.sav` files).

- `tools/videos.sh SAVE.sav filters` shows the orbit video in all four looks side by side: as
  drawn, sepia, grey and psychedelic. New videos for the website: the river flight in sepia and
  the trader's boat in grey.

- **The probe**, a command language to look at and drive a running game: find interface
  elements by name, text, kind or dialog, click them, and read the town (population,
  professions, buildings, animals, stock). In the browser it is the console object `unbanished`
  (`await unbanished.town()`); headless runs take `--probe "COMMAND@T"` and `--probe-out FILE`.
  See [docs/develop/PROBE.md](docs/develop/PROBE.md).

- **Test suites** with known outcomes: `tools/test/run.py` runs them in tiers (`quick`,
  `standard`, `long`). New suites check probe answers against baselines, and drive the browser
  version in headless Chrome and Firefox: the start page, the manager, Exit and Quit, and a new
  game that must match the desktop version's.

### Changed

- Multiplayer: every player counts achievements from none won in a multiplayer game, so players
  with different achievements no longer see their towns reloaded at every check. An achievement
  won in a multiplayer game is still kept in the player's own settings.

- The multiplayer relay is now `unbanished-server` (folder `apps/server`). It reads a settings
  file (`--config FILE`), answers `GET /health` on its port, can cap the open rooms, and comes
  with a Dockerfile. Its README shows how to put TLS in front of it with Caddy for browser
  players.

- Multiplayer protocol version 2: the server checks that the players still play the same game.
  Every player sends it a digest of its game; when they differ, the server keeps the game most
  players have (the host's when it is a tie) and asks a player who has it to send the others its
  save. Before, the host checked, and its own game was always the one kept.
- Multiplayer protocol version 1: a game tells the relay its version, game data and mods first.
  A relay or a host of another version, or with other game data or mods, refuses it and says
  why. Rooms can have a password, and a game can list the open rooms. Update the relay with the
  game: they must speak the same protocol.

- The extensions (multiplayer, recordings, the probe, the mod sites' Browse tabs and the
  cinematic camera) are cargo features, on by default. `--no-default-features` builds the game without them, and a test
  checks that it then plays exactly as the full build. See
  [docs/develop/EXTENSIONS.md](docs/develop/EXTENSIONS.md).

- The code is renamed to `unbanished`: the crates are `unbanished-*` and the programs
  `unbanished`, `unbanished-mods`, `unbanished-cli` and `unbanished-relay`. Environment variables
  are `UNBANISHED_*` (rename `BANISHED_*` in your own scripts). Your settings, kept game files and
  saves stay where they are.

- The running town is its own crate, `unbanished-sim` (`crates/sim`): the town, its map, the save
  as the game reads it and the mod lists, with no drawing, sound or interface. The game app draws
  it. Its `Simulation` holds the game's deterministic state, loads a save, runs each frame
  and writes what a save keeps, with no window: a test checks it against the whole game.
  Nothing in the game changes.

- The game's code is easier to find: `apps/game/src/main.rs` is split into `game.rs` (the
  `Game`), `app.rs` (the window) and `native.rs` (the commands without a window), and the
  browser page's style and scripts are in `apps/game/web/`. Nothing in the game changes.
- More large files are split by topic, with no change to the game: the interface library
  (`lib.rs` and `controls.rs`), the game loop (`gameplay.rs`: loading, one frame, raw input and
  script commands), a character's actions (`character.rs`), the construction state machine
  (`construction.rs`), the viewer (`tool.rs`) and the model renderer (`models.rs`, whose shader is
  now `models/models.wgsl`).
- Loading a save is faster: a town's buildings, people and trees find their parts in the save
  through a table made once, instead of searching each time. A large town (15 MB save) loads
  in 536 ms instead of 827 ms; the town is the same.

### Fixed

- In the browser, **Options → Video → Resolution** showed the smallest size (50 % of the page)
  when none had been chosen, although the game drew at the full page; pressing Ok could then
  halve it. It now shows the full page size, as the game draws it.
- The browser version starts with WebGL 2 when the browser offers WebGPU but cannot give the
  page a WebGPU canvas (for example headless Firefox, or a blocked graphics card). It used to
  stop on "Reading 4 package(s)…".

- A save imported on the browser's start page could be lost: the page reloaded before the save
  was written to the browser's storage.
- **Exit** in the browser version no longer leaves the page frozen (#18). The installed app
  closes its window; in a browser tab the game goes back to its start page, and on the Play page
  to the **Start the game** button.
- Scroll bars now fill the side of their list, as in the original. They were only as long as
  their smallest size, so the thumb was tiny and sat in the middle of the list.

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

[Unreleased]: https://github.com/tanepiper/unbanished/compare/v0.5.0...HEAD
[0.5.0]: https://github.com/tanepiper/unbanished/compare/v0.4.1...v0.5.0
[0.4.1]: https://github.com/tanepiper/unbanished/compare/v0.4.0...v0.4.1
[0.4.0]: https://github.com/tanepiper/unbanished/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/tanepiper/unbanished/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/tanepiper/unbanished/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/tanepiper/unbanished/releases/tag/v0.1.0
