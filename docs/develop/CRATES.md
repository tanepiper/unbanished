# Apps and crates

The Un-Banished workspace has five programs and thirteen libraries. Each one has a README with its
purpose, how to use it, and examples.

## Programs

| Program | Package | What it does |
|---|---|---|
| [`unbanished`](../../apps/game/README.md) | `unbanished-app` | The game, for the desktop and the browser. It also makes headless captures for tests. |
| [`unbanished-viewer`](../../apps/viewer/README.md) | `unbanished-viewer` | Browses the game's models, animations, textures, sounds and dialogs. |
| [`unbanished-cli`](../../apps/cli/README.md) | `unbanished-cli` | Command-line tools for the game data: it decodes every sound to MP3, and searches and installs mods from Nexus Mods and banishedinfo.com. |
| [`unbanished-mods`](../../apps/modmanager/README.md) | `unbanished-modmanager` | The mod manager, a small desktop program: turn mods on and off, set their order, keep mod lists, install from Nexus Mods and banishedinfo.com, and start the game. Not part of the original game. |
| [`unbanished-relay`](../../apps/relay/README.md) | `unbanished-relay` | The multiplayer relay: a small WebSocket server with rooms, which passes the players' data on. Not part of the original game. See [Multiplayer](../USER-GUIDE.md#multiplayer). |

<p align="center">
  <img src="https://tanepiper.github.io/unbanished/assets/screenshots/mod-manager.webp" alt="The mod manager, unbanished-mods, with five mods, three of them on" width="720">
</p>

## Libraries

| Crate | What it contains | Depends on |
|---|---|---|
| [`unbanished-core`](../../crates/core/README.md) | The exact foundations: the C runtime's float maths, the game's random number generator, Perlin noise, transforms, game time, the calendar, picking | nothing |
| [`unbanished-terrain`](../../crates/terrain/README.md) | Map generation from a seed, the terrain mesh, rivers, ground height and cell types | core, formats |
| [`unbanished-path`](../../crates/path/README.md) | The A* pathfinder, walkable regions and the sector graph | core, terrain |
| [`unbanished-game`](../../crates/game/README.md) | The game rules: citizens, jobs, buildings, farming, trade, disasters, and the town | core, terrain, path, formats |
| [`unbanished-formats`](../../crates/formats/README.md) | Readers for packages, resources, meshes, textures, sounds, dialogs and saves | nothing |
| [`unbanished-ui`](../../crates/ui/README.md) | The game's interface system: elements, layout, text, controls, input | formats |
| [`unbanished-gfx`](../../crates/gfx/README.md) | Rendering shared by the game and the viewer: texture upload, the interface renderer, materials, skinning | formats, ui, wgpu |
| [`unbanished-audio`](../../crates/audio/README.md) | Sound decoding and playback: the 64-channel device and the sound scene | formats, wma |
| [`unbanished-wma`](../../crates/wma/README.md) | A decoder for Windows Media Audio v2, the codec of the game's sounds | nothing |
| [`unbanished-mods`](../../crates/mods/README.md) | The mod list (`IO::Modification`) shared by the game and the mod manager: the `.pkm` packages, which are enabled, their order and conflicts, and the settings file | formats |
| [`unbanished-banishedinfo`](../../crates/banishedinfo/README.md) | Optional [banishedinfo.com](https://banishedinfo.com/mods/browse) support, not part of the original game: listing and searching the site's mods, their versions, and installing them (desktop only) | nexus (its installer); ureq with the `native` feature |
| [`unbanished-net`](../../crates/net/README.md) | Multiplayer networking, not part of the original game: the protocol between a game and the relay, and a WebSocket connection that works natively and in the browser | nothing (postcard; tungstenite with the `native` feature) |
| [`unbanished-nexus`](../../crates/nexus/README.md) | Optional Nexus Mods support, not part of the original game: searching Banished mods, download links with the player's API key, and the `.pkm` packages in a download | nothing (zip, serde_json; ureq with the `native` feature) |

`unbanished-game` re-exports `unbanished-core`, `unbanished-terrain` and `unbanished-path`. For
example, `unbanished_game::random` is the same module as `unbanished_core::random`, and
`unbanished_game::terrain` is the `unbanished_terrain` crate.

## How they fit together

| Program | Uses |
|---|---|
| `unbanished` | game, formats, ui, gfx, audio, mods, nexus, banishedinfo, net |
| `unbanished-viewer` | game, formats, ui, gfx, audio, wma |
| `unbanished-cli` | formats, nexus, banishedinfo |
| `unbanished-mods` | mods, nexus, banishedinfo |
| `unbanished-relay` | net |

The libraries form layers. A crate uses only crates in the layers below it:

1. `unbanished-core`, `unbanished-formats`, `unbanished-wma`, `unbanished-nexus`, `unbanished-net`: no
   dependencies on other crates of the workspace.
2. `unbanished-terrain`, `unbanished-ui`, `unbanished-audio`, `unbanished-mods`, `unbanished-banishedinfo`.
3. `unbanished-path`, `unbanished-gfx`.
4. `unbanished-game`.

The two crates with no dependencies at all, `unbanished-core` and `unbanished-formats`, can be used
on their own. `unbanished-wma` also has no dependencies.

## Rules for each kind of code

| Kind of code | Where it goes | Rules |
|---|---|---|
| Exact maths, random numbers, game time | `crates/core` | No game rules, no data files |
| Game rules and the simulation | `crates/game`, `crates/terrain`, `crates/path` | No graphics, windows or sound; deterministic; tested against the original |
| Reading files | `crates/formats` | No dependencies |
| The interface system | `crates/ui` | General interface code; no rules of the game |
| Engine loops, game controllers, dialogs, rendering | `apps/game` | Must also work in the browser (WebGL 2, one thread) |

The [API documentation](API.md) lists every public item of the libraries.
