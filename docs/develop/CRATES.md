# Apps and crates

The Un-Banished workspace has four programs and eleven libraries. Each one has a README with its
purpose, how to use it, and examples.

## Programs

| Program | Package | What it does |
|---|---|---|
| [`banished`](../../apps/game/README.md) | `banished-app` | The game, for the desktop and the browser. It also makes headless captures for tests. |
| [`banished-viewer`](../../apps/viewer/README.md) | `banished-viewer` | Browses the game's models, animations, textures, sounds and dialogs. |
| [`banished-cli`](../../apps/cli/README.md) | `banished-cli` | Command-line tools for the game data: it decodes every sound to MP3, and searches and installs mods from Nexus Mods. |
| [`banished-mods`](../../apps/modmanager/README.md) | `banished-modmanager` | The mod manager, a small desktop program: turn mods on and off, set their order, keep mod lists, install from Nexus Mods, and start the game. Not part of the original game. |

<p align="center">
  <img src="https://tanepiper.github.io/unbanished/assets/screenshots/mod-manager.webp" alt="The mod manager, banished-mods, with five mods, three of them on" width="720">
</p>

## Libraries

| Crate | What it contains | Depends on |
|---|---|---|
| [`banished-core`](../../crates/core/README.md) | The exact foundations: the C runtime's float maths, the game's random number generator, Perlin noise, transforms, game time, the calendar, picking | nothing |
| [`banished-terrain`](../../crates/terrain/README.md) | Map generation from a seed, the terrain mesh, rivers, ground height and cell types | core, formats |
| [`banished-path`](../../crates/path/README.md) | The A* pathfinder, walkable regions and the sector graph | core, terrain |
| [`banished-game`](../../crates/game/README.md) | The game rules: citizens, jobs, buildings, farming, trade, disasters, and the town | core, terrain, path, formats |
| [`banished-formats`](../../crates/formats/README.md) | Readers for packages, resources, meshes, textures, sounds, dialogs and saves | nothing |
| [`banished-ui`](../../crates/ui/README.md) | The game's interface system: elements, layout, text, controls, input | formats |
| [`banished-gfx`](../../crates/gfx/README.md) | Rendering shared by the game and the viewer: texture upload, the interface renderer, materials, skinning | formats, ui, wgpu |
| [`banished-audio`](../../crates/audio/README.md) | Sound decoding and playback: the 64-channel device and the sound scene | formats, wma |
| [`banished-wma`](../../crates/wma/README.md) | A decoder for Windows Media Audio v2, the codec of the game's sounds | nothing |
| [`banished-mods`](../../crates/mods/README.md) | The mod list (`IO::Modification`) shared by the game and the mod manager: the `.pkm` packages, which are enabled, their order and conflicts, and the settings file | formats |
| [`banished-nexus`](../../crates/nexus/README.md) | Optional Nexus Mods support, not part of the original game: searching Banished mods, download links with the player's API key, and the `.pkm` packages in a download | nothing (zip, serde_json; ureq with the `native` feature) |

`banished-game` re-exports `banished-core`, `banished-terrain` and `banished-path`. For
example, `banished_game::random` is the same module as `banished_core::random`, and
`banished_game::terrain` is the `banished_terrain` crate.

## How they fit together

| Program | Uses |
|---|---|
| `banished` | game, formats, ui, gfx, audio, mods, nexus |
| `banished-viewer` | game, formats, ui, gfx, audio, wma |
| `banished-cli` | formats, nexus |
| `banished-mods` | mods, nexus |

The libraries form layers. A crate uses only crates in the layers below it:

1. `banished-core`, `banished-formats`, `banished-wma`, `banished-nexus`: no dependencies on
   other crates of the workspace.
2. `banished-terrain`, `banished-ui`, `banished-audio`, `banished-mods`.
3. `banished-path`, `banished-gfx`.
4. `banished-game`.

The two crates with no dependencies at all, `banished-core` and `banished-formats`, can be used
on their own. `banished-wma` also has no dependencies.

## Rules for each kind of code

| Kind of code | Where it goes | Rules |
|---|---|---|
| Exact maths, random numbers, game time | `crates/core` | No game rules, no data files |
| Game rules and the simulation | `crates/game`, `crates/terrain`, `crates/path` | No graphics, windows or sound; deterministic; tested against the original |
| Reading files | `crates/formats` | No dependencies |
| The interface system | `crates/ui` | General interface code; no rules of the game |
| Engine loops, game controllers, dialogs, rendering | `apps/game` | Must also work in the browser (WebGL 2, one thread) |

The [API documentation](API.md) lists every public item of the libraries.
