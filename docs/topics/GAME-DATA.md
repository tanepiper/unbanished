# Game data files

Everything that Banished shows or plays comes from four files in its `WinData` folder:
`data0.pkg`, `data1.pkg`, `patch0.pkg` and `patch1.pkg`, about 165 MB together. Un-Banished
reads these files from your copy of the game. This page explains what is inside them.

## The two programs

The original game has two main program files:

| File | What it contains |
|---|---|
| `Application-steam-x64.exe` | The game's logic: citizens, buildings, the game loop, saves. About 9,850 functions. |
| `Runtime-steam-x64.dll` | The engine: files, maths, random numbers, the interface, sound, rendering. About 15,260 functions. |

The engine exports about 7,400 functions with their C++ names, for example
`UI::Element::SetChild`. These names helped a lot. The game's own functions have no names; they
are known by their address, for example `FUN_1400d4860`.

## Packages

A package (`.pkg`) is an archive of entries. Its table of contents and its entries are stored in
*block streams*: a list of blocks, each up to 32 KiB, each either stored or compressed with LZF
(a simple, fast compression method). In total the four packages hold 5,832 entries.

| Package | Priority | Entries |
|---|---|---|
| `data0.pkg` | 200 | 5,428 |
| `data1.pkg` | 100 | 406 |
| `patch0.pkg`, `patch1.pkg` | 190, 120 | an index only |

When two packages have an entry with the same name, the package with the higher priority wins.
Mods (`.pkm` files, made with the BanishedKit) use the same format. The Mods menu decides their
order, and a mod can replace only the entries that the game allows it to replace.

In Un-Banished: `banished_formats::pkg::Archive`. See
[banished-formats](../../crates/formats/README.md) and [MODS.md](../MODS.md).

## Compiled resources

Most entries are compiled resources (`.crs`): engine objects, saved with the engine's own
serialiser. Every resource has the same shape:

- a block: a start marker (`0xaaaaaaaa`), a size, a version, the payload, and an end marker
  (`0xbbbbbbbb`);
- in the payload: a magic number, the **type id**, a file version, and a list of the resources it
  depends on;
- then the object itself: **one nested block for each level of its class**, base class first.

The type id is the CRC-32 of the class name, written in UTF-16. So `ImageBuffer` always has the
same id, and the id names the class. The game ships 170 different classes, and Un-Banished reads
all of them.

Resources refer to each other by a reference such as `Dialog/Toolbar.rsc:resource`: a resource
file and the name of an object in it. Inside the package, the same object is the entry
`Dialog\Toolbar_resource.crs`.

### What kinds of resources there are

| Folder | Entries | What it holds |
|---|---|---|
| `Models` | 1,117 | Meshes, skeletons, animations, materials, textures |
| `Game` | 1,159 | Gameplay data: terrain, climate, start conditions, names, the toolbar, menu scenes |
| `Template` | 1,540 | Entity templates: every building, citizen, animal, tree and resource, with its components |
| `Dialog` | 1,456 | The interface: dialogs, sprite sheets, fonts, string tables |
| `Terrain` | 43 | Terrain textures and materials |
| `Material` | 269 | Materials and the compiled Direct3D shaders |
| `Audio` | 137 (+16 streams) | Sounds and music |

## How the formats were decoded

Each class has a `Serialize` function in the original program. It reads or writes the fields in
order, through the engine's binary reader. So the field order and types come from that function.
A decoder is correct when it reads **every byte** of every block, for every resource of that
class. Leftover bytes mean a missed field. The Python tools in `tools/crs/` check this over the
whole game, and the Rust readers in `banished-formats` have tests that do the same.

[Decode a format](../develop/guides/FORMAT.md) explains how to add a reader for a class.

## Read more

- [FINDINGS.md](../FINDINGS.md), sections 2 and 3: the package and resource formats in detail.
- [GAME-RESOURCE-INVENTORY.tsv](../GAME-RESOURCE-INVENTORY.tsv): every resource by type.
- [The viewer](../../apps/viewer/README.md): browse the models, textures, sounds and dialogs.
