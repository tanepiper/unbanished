# banished-formats

Readers for the files of Banished 1.0.7 and its engine: the `.pkg` packages, the compiled
resources (`.crs`) inside them, and the save files (`.sav`). The crate has no dependencies, and
it contains no game data. You give it the bytes of your own copy of the game.

It reads all 170 resource types that the game ships: meshes, animations, textures, materials,
sounds, dialogs, fonts, string tables, building templates, start conditions, terrain profiles,
scenarios and more. It also reads and writes save files.

## Use it

```toml
[dependencies]
banished-formats = { path = "crates/formats" }
```

### Open the game's packages

The game's data is in four packages in the `WinData` folder. An `Archive` merges them. When two
packages hold an entry with the same name, the package with the higher priority wins, as in the
original.

```rust
use banished_formats::pkg::{Archive, Package};
use std::path::Path;

fn open(windata: &Path) -> banished_formats::Result<Archive> {
    let mut archive = Archive::default();
    for name in ["data1.pkg", "patch1.pkg", "patch0.pkg", "data0.pkg"] {
        let bytes = std::fs::read(windata.join(name)).expect("read package");
        archive.add(Package::parse(bytes)?);
    }
    Ok(archive)
}
```

Mods (`.pkm` files) use the same container. Add them with `archive.add_mod(package)`, after the
game's packages, in the order of the Mods menu.

### Read an entry or a resource

Entries have file names such as `Dialog\StringTable_toolBar.crs`. The game refers to the same
resource as `Dialog/StringTable.rsc:toolBar`. `read` takes the entry name; `read_ref` takes the
game's reference.

```rust
use banished_formats::pkg::Archive;

fn read(archive: &Archive) -> banished_formats::Result<Vec<u8>> {
    println!("{} entries", archive.names().len());
    archive.read_ref("Dialog/StringTable.rsc:toolBar") // one resource, decompressed
}
```

### Decode a resource

Each resource type has a function that takes the bytes and returns a typed value. The function
checks the type id first, so it fails with an error if the bytes are another type.

```rust
use banished_formats::pkg::Archive;
use banished_formats::ui::string_table;

fn toolbar_strings(archive: &Archive) -> banished_formats::Result<()> {
    for (key, text) in string_table(&archive.read_ref("Dialog/StringTable.rsc:toolBar")?)? {
        println!("{key} = {text}");
    }
    Ok(())
}
```

A texture, decoded to RGBA for display:

```rust
use banished_formats::pkg::Archive;
use banished_formats::{crc::type_id, crs, texture::decode_rgba, types};

fn first_texture(archive: &Archive) -> banished_formats::Result<Option<(u32, u32, Vec<u8>)>> {
    for name in archive.names().into_iter().filter(|n| n.ends_with(".crs")) {
        let data = archive.read(name)?;
        // The type id is the CRC-32 of the class name in UTF-16LE.
        if crs::header(&data).map(|h| h.type_id) != Ok(type_id(types::ImageBuffer::CLASS)) {
            continue;
        }
        let image = types::image_buffer(&data)?;
        let rgba = decode_rgba(image.format, image.width, image.height, &image.levels[0]);
        return Ok(rgba.map(|rgba| (image.width, image.height, rgba)));
    }
    Ok(None)
}
```

### Read a save

```rust
use banished_formats::save::Save;

fn describe(bytes: &[u8]) -> banished_formats::Result<()> {
    let save = Save::parse(bytes)?;
    let meta = save.metadata()?;
    let header = save.loop_header()?;
    println!("{}: seed {} on {}", meta.name, header.seed, header.terrain);
    println!("{} mods", meta.mods.len());
    Ok(())
}
```

`Save` gives the parts of a save: the metadata (name, preview image, mods), the game loop's
header (seed, terrain, climate, start condition, map size), the map's heights and cells, the
event log and the table of external references. `savegame` decodes the whole game-state
stream, object by object, with the schemas in `savegame_schema`.

## Modules

| Module | What it reads |
|---|---|
| `pkg` | Packages (`.pkg`, `.pkm`): `Package`, `Archive`, entry names and references |
| `lzf`, `stream` | LZF decompression, and block streams (`IO::CompressedStream`) |
| `crs` | The compiled-resource container: `header`, `parse`, and the `Reader` for fields |
| `crc` | CRC-32 and `type_id(class_name)` |
| `types` | Engine resources: `ImageBuffer`, `GraphicsMesh`, `AnimationGroup`, `MaterialInstance`, `VertexLayout`, `SoundBuffer` |
| `texture` | `decode_rgba`: BC1, BC3 and the other image formats, to RGBA8 |
| `skeleton` | Bone maths for skinned meshes |
| `ui` | Dialogs, element descriptions, fonts, sprite sheets, cursors, string tables |
| `game` | Gameplay resources: terrain and climate generators, map and environment descriptions, toolbar items, templates and their component descriptions, particles, and more |
| `audio` | Sound effects, music lists, ambient emitters and environments |
| `weather` | Rain and snow descriptions |
| `scenario` | Achievements, scenarios, goals and tutorials |
| `menu_scene` | The 3D scenes behind the main menu, and the map component they share with saves |
| `names` | The name-generation databases (`.ndb`) for citizens and towns |
| `picking` | Picking geometry for selecting objects |
| `registry` | The loaded resources, listed by type, as the game enumerates them |
| `save` | Save files: container, metadata, header, heights, cells, event log, references |
| `savegame`, `savegame_schema` | The game-state stream of a save, and the schema of every saved object |
| `par` | A parallel map for loading work (all CPU cores natively, one thread in the browser) |
| `error` | `Error` and `Result` |

## Example programs

The crate has small programs that print what a resource or a save contains. They help when you
trace or port something. Each one takes the `WinData` folder (or a save) and a reference.

```bash
cargo run -p banished-formats --example dialog_info -- game-data/original/WinData "Dialog/Toolbar.rsc:resource"
cargo run -p banished-formats --example string_table -- game-data/original/WinData Dialog/StringTable.rsc:toolBar
cargo run -p banished-formats --example template_info -- game-data/original/WinData Template/WoodHouse.rsc
cargo run -p banished-formats --example savegame_same -- a.sav b.sav
```

| Example | What it prints |
|---|---|
| `anim_info` | The clips of an animation group |
| `dialog_info` | A dialog resource: elements, names, ids, templates |
| `image_info` | A texture's header and level sizes |
| `job_info` | The job-related descriptions of templates, and the professions |
| `mesh_info` | A mesh's parts, draw ranges and bounds |
| `menu_scene_dump` | The meshes placed in a main-menu scene |
| `profile_info` | A start-condition profile |
| `string_table` | Entries of a string table |
| `template_info` | The components of an entity template |
| `ui_controllers` | A template's interface description: host, page and controllers |
| `save_refs` | A save's table of external references |
| `save_state`, `save_dump_state` | Write a save's game-state stream to a file |
| `savegame_check` | Decode the game state of saves and report the first failure |
| `savegame_diff` | Compare two saves: clocks, random state, objects by type |
| `savegame_fields_diff` | Where two saves differ, object by object and field by field |
| `savegame_same` | Whether two saves hold the same game state (exit code 0 or 1) |
| `savegame_works` | The jobs in a save's job manager |

Two more programs are binaries: `climate_info` and `environment_info`
(`cargo run -p banished-formats --bin climate_info -- …`).

## Tests

```bash
cargo test --release -p banished-formats
```

The corpus tests in `tests/` parse every resource of a type in your game data, and check known
values. They skip when `game-data/original/WinData` is missing.

## The API

The full API documentation is at
<https://tanepiper.github.io/unbanished/api/banished_formats/index.html>.

## Related documents

- [Game data files](https://tanepiper.github.io/unbanished/topics/game-data)
- [Save files](https://tanepiper.github.io/unbanished/topics/saves)
- [Decode a format](https://tanepiper.github.io/unbanished/develop/guides/format)
- [FINDINGS.md](https://github.com/tanepiper/unbanished/blob/main/docs/FINDINGS.md), sections 2 and 3: the container formats in detail
