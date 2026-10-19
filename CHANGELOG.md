# Changelog

All notable changes to Un-Banished are listed here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/). Before 1.0 there are no compatibility promises.

The version is the engine's own (`banished --version`). The Banished version in the main menu
(1.0.7, build 170910) is the original game's and does not change.

## [Unreleased]

Version 0.2.0 in development. The focus is performance and efficiency without changing the
game's behaviour; the plan is in [docs/PERFORMANCE.md](docs/PERFORMANCE.md).

### Changed

- The engine version is now 0.2.0 (shown as `0.2.0-<build>` until the release).
- No freeze when the town appears after loading: the ambient sounds, the first music track
  and the decals are now prepared behind the loading screen (about 0.5 s on a large town).
- Faster loading on computers with more than one CPU core (native builds): resources, sounds,
  textures, meshes and the terrain's surface image are decoded on all cores. Starting the game,
  opening the main menu, starting a new game and loading a save take 20–55 % less time. The
  browser build still loads on one thread.

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
