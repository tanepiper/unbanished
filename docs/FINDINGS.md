# Banished → WASM: reverse-engineering findings

Target: Steam depot `app_242920/depot_242921` (Windows build), analysed with Ghidra 12.1.4 headless
(default auto-analysis). Unless a row says *inferred*, everything below was observed in decompiled code
or verified against the shipped data files.

## 1. Binary layout

| File | Role | Functions (named) | Notes |
|---|---|---|---|
| `Application-steam-x64.exe` | Game logic | 9,851 (5,245) | Imports engine classes from Runtime, Steam, Kernel32 only |
| `Runtime-steam-x64.dll` | Engine | 15,261 (12,623) | Exports ~7,400 mangled C++ symbols; no third-party middleware |
| `VideoDX11-steam-x64.dll` | Renderer backend | 423 | Implements abstract `Video::` interface over D3D11/DXGI |
| `VideoDX9-steam-x64.dll` | Renderer backend | 397 | Same interface over D3D9 |
| `steam_api64.dll` | Steamworks | — | Achievements etc.; drop or stub in a port |
| `WinData/*.pkg` | Game data | — | 5,832 entries, see §2 |

Runtime namespaces by exported-symbol count: `UI` 1761, `Graphics` 634, `Video` 605, `IO` 426,
`System` 277, `Math` 228, `Object` 209, `Audio` 144, `Achievement` 131, `Engine` 116,
`Collision2D` 80, `Input` 67, `Volume` 64, `Picking` 51, `Pathfinding` 48, `Save` 38.

**Port implication:** the engine is cleanly layered (game → engine → video backend). The video backend
is a small, swappable DLL behind a stable interface, so a WebGPU/WebGL2 backend is a contained task.

## 2. Package format (`.pkg`) — fully decoded

Source: `IO::PackageFile::ReadPackfile` (0x1800b6f10), `IO::CompressedStream::Open` (0x1800ab4b0),
`IO::CompressedStream::DecompressFromSource` (0x1800aa600), `System::Compression::Decompress` (0x1800c6ed0).

- **Compression is LZF** (liblzf wire format). Control byte `< 0x20` = literal run of `ctrl+1` bytes;
  otherwise length `ctrl>>5` (7 ⇒ +next byte), offset `((ctrl&0x1f)<<8) + next + 1`, copy `len+2`.
  `System::Compression::Index` is liblzf's hash function.
- **Block stream** (`IO::CompressedStream`): `u32 n`, then `n × {u32 offset, u16 compSize, u16 rawSize}`,
  then the blocks. `compSize == 0` means the block is stored uncompressed. Blocks decompress to ≤ 32 KiB. The engine
  validates `4 + 8n + Σ size == streamLength`.
- **Package:** `u32` header (bit 31 = extended flag, low 31 bits = TOC offset) → `u32 tocSize` → TOC
  as a block stream. TOC layout is documented in `tools/pkg_extract.py`.
- **Verified:** all four packages parse with zero leftover TOC bytes; 5,812 of 5,812 `.crs`
  resources pass the engine's size check and decompress.

| Package | Priority | Entries |
|---|---|---|
| data0.pkg | 200 | 5,428 |
| data1.pkg | 100 | 406 |
| patch0.pkg / patch1.pkg | 190 / 120 | index stub only |

Priority/override fields imply later packages shadow earlier entries by name (*inferred*, not yet
traced in the resource manager).

### Unresolved entries
- **16 × `Audio/Music/*.str`**: raw, not block streams. `Audio::SoundBuffer` exposes
  `GetWMAData`/`GetWMALength`/`IsStreamed`/`GetStreamPath`, so these are very likely **xWMA**
  (*inferred*). The header (format, decoded-packet table) is probably in the paired
  `*_music.crs`. Next: parse that header, wrap in a RIFF/XWMA container, decode with FFmpeg, and
  transcode to Opus for the browser.
- **4 × `Save/Tutorial*.sav`**: fail the block-stream size check, so they use a different save container
  (see the `Save::` / `SaveLoad` classes).

## 3. Resource format (`.crs`)

Implemented in `crates/formats/src/crs.rs` (Rust) and `tools/crs/` (Python, for exploration).

```text
block    = u32 0xaaaaaaaa, u32 size, u32 version, payload, u32 0xbbbbbbbb   (size covers payload + terminator)
resource = block { u32 0x5412a91f, u32 typeId, u32 fileVersion,
                   u32 depCount, depCount x { String path, u64 FILETIME }, object... }
String   = u32 n|0x80000000 then n bytes (8-bit, NUL included) | u32 n then n UTF-16 units
ExternalPtr = bool present, String "Path\File.rsc:object"   (stored as Path\File_object.crs)
```

- Sources: `IO::BinaryWriter` ctor (0x1800a9980) and terminator, `Object::External::Save`
  (0x1800be4a0), `System::String::Serialize` (0x1800cc1a0).
- **Type ids are `CRC32(UTF-16LE(className))`** (`Object::Factory::Startup`, 0x1800c2ea0). All 170
  observed ids are named in `tools/schema/types.json`.
- Every class level of `Serialize(Stream&)` opens its own nested block. Fields are written through
  virtual calls on `IO::BinaryReader`/`BinaryWriter`; the vtable slot → type map (dumped with
  `tools/ghidra/vtable.py`) is: `+0x18..+0x68` array overloads, `+0x70` char, `+0x78` bool,
  `+0x80` float, `+0x88` u64, `+0x90` i64, `+0x98` u32, `+0xa0` i32, `+0xa8` u16, `+0xb0` i16,
  `+0xb8` u8, `+0xc0` i8, `+0xc8` bytes. Field names are passed as null in this build.
- Every block is consumed exactly by the decoders, which is the main correctness check.

### Decoded classes

| Class | Count | Notes |
|---|---|---|
| `ImageBuffer` | 406/406 | w, h, type (0=2D, else cube), usage, format, generateMips, address×3, filter, source, levels. Format enum → DXGI via VideoDX11: 0 BC1, 1/2 BC3, 3 A8, 4 R8, 5 BGRA8, 6 R32F, 7 R24G8. All 3,179 levels match their sizes. |
| `GraphicsMesh` | 271/271 | parts (LOD distance, name, **AO texture** ref), material-instance lists, skeleton, bounds, then a DX9 and a DX11 geometry sub-block. DX11: per part draw ranges + 4 buffers (u8 flag, u32 len, bytes): static IB, static VB, skinned IB, skinned VB. |
| `MaterialInstance` | 209/209 | material ref, named texture slots (`diffuse`, `snow`, `damage`, …), named float4s. |
| `VertexLayout` | 19/19 | elements (semantic, stream, offset, usage, usageIndex, format) + DXBC input signature. Formats: 0 f32x4, 1 f32x3, 2 f32x2, 3 f32, 4 **sint16x4**, 5 sint16x2, 6 bgra8unorm, 7 uint8x4, 8 unorm8x4. |

### Geometry details
- `ModelLayout` (static, 24-byte stride): sint16x4 position **/512**, sint16x4 texcoord (xy /512
  diffuse, zw /8192 AO), unorm8x4 normal, 4 bytes padding. `ModelSkinLayout` (32 bytes) adds unorm8x4
  weights and uint8x4 bone indices.
- Skeleton convention (`crates/formats/src/skeleton.rs`): stored quaternions are conjugated;
  world = parent × local; skinning matrix = world × inverse bind. **Skinned vertices sit in a per-mesh
  rotated frame**, so they must be skinned even in bind pose. All eight skinned meshes, including the
  boat, reproduce stored bounds within 2 mm. The boat combines rigid hull/oars with a skinned merchant;
  both streams, local index base vertices and per-range materials must be retained. See [BOAT.md](BOAT.md).
- `OpaqueCharacterMaterial` diffuse textures are horizontal atlases of square outfit variants
  (citizens: 8 variants, 1024×128); the mesh UVs cover a single cell.

| Directory | Files | Size |
|---|---|---|
| Models | 1,117 | 98 MB |
| Game | 1,159 | 78 MB |
| Terrain | 43 | 31 MB |
| Dialog | 1,456 | 29 MB |
| Template | 1,540 | 6 MB |
| Material | 269 | 3 MB |
| Audio | 137 (+16 .str) | 2.3 MB |

Shaders (`*_vs`, `*_ps`) are compiled D3D bytecode (*inferred*). They need rewriting in WGSL or
translating, for example with DXBC → SPIR-V → WGSL tooling.

## 4. Recommended port architecture

A transliteration of decompiled code is not viable: Ghidra pseudocode is not compilable source, and it
reproduces Shining Rock's code. The workable approach is a **clean reimplementation** in C++ or Rust,
built with Emscripten or wasm-bindgen, that loads the user's own `.pkg` files at runtime. This is the
same model as OpenMW and OpenRCT2. The repository ships no original code or assets.

| Layer | Original | Port |
|---|---|---|
| Data | `IO::PackageFile`, LZF | ✅ done (`tools/pkg_extract.py`); port to the runtime language |
| Serialization | `IO::Serializer`, type hashes | Decode per component type; build a schema table |
| Rendering | `Video::` + DX11 DLL | WebGPU, with WebGL2 fallback; rewrite shaders |
| Audio | `Audio::` + XAudio2 (xWMA) | WebAudio; pre-transcode music to Opus |
| Platform | Win32 window and input | Browser canvas, Pointer Events, keyboard |
| Files/saves | Win32 file IO | OPFS / IndexedDB; user drops in their `WinData` folder |
| UI | `UI::` (data-driven dialogs in `Dialog/`) | Reimplement element types; render from `.crs` layouts |
| Simulation | Application exe components | Reimplement each `*Description`/component type |
| Steam | `steam_api64` | Stub (local achievements optional) |

Suggested milestones:
1. ✅ Package reader and extractor.
2. ✅ `.crs` deserializer: container, type ids, textures, meshes, material instances, vertex layouts
   (remaining classes are decoded as later milestones need them).
3. ✅ Model viewer, native and in the browser (Rust + wgpu; WebGPU with WebGL2 fallback). Terrain moves to M5.
4. ✅ UI system: all 1,644 UI resources decode; `crates/ui` renders the main menu and the asset
   browser from game data (see §6).
5. Map generation and terrain from `Terrain/` plus the Perlin-noise code.
6. Simulation components, one at a time (resources → storage → work → citizens → seasons).
7. Save/load, audio, polish. Audio decoding is done (§7).

## 6. UI system

Decoders: `crates/formats/src/ui.rs`. Runtime: `crates/ui`. All 1,644 UI resources decode exactly:
55 `Dialog`, 4 `Font`, 31 `SpriteSheet`, 115 `StringTable` and 1,439 elements across 26 shipped
`*Description` classes. All 27 decoder classes have runtime representations; ClipRegion has no standalone
resource in this install and is covered with a constructed fixture.

**Element base** (`ElementDescription::Serialize` 0x180119390, defaults from its constructor):
margins (left, right, top, bottom), minimum size, alignment 0–8 (3×3 grid, default 3), flags
(bit 0 hidden, bits 2/3 fill width/height, bits 4/5 screen-relative size, bit 18 half alpha),
ARGB colour (default white), id, then border, tooltip-dialog and string-table refs and a tooltip key.

**Layout** is two-pass, as in the engine: `ComputeSize` bottom-up, then `ComputeLayout` top-down.
- Groups fill the space offered.
- Ribbons stack their children with spacing; the last child can stretch.
- Borders fill.
- Other elements are placed at their size by alignment within their margins.
- Dialog roots are placed at their own size (`DialogInstance::ComputeLayout` 0x180116620).

**Drawing:**
- Borders are 9-slice: corners at natural size, edges and centre stretched.
- Buttons draw their current state element (0 enabled, 1 disabled, 2 highlight, 3 pressed) and then
  their content, tinted by a per-state colour.
- Labels pair an icon sprite with text; `orientation` places the icon left, right, above or below.

**Fonts** (`Font::Serialize` v2): an int code-point→glyph map and per-glyph `(a, c, b, d)` metrics.
The pen advances `a` plus the previous glyph's `c`; the glyph is `b` wide and raised by `d`. Glyph
bitmaps are sprites in the font's sprite sheet, and a sheet's image is a `MaterialInstance` `diffuse`
texture. The font baseline is `size - descent`, traced from `TextElement::Draw` and
`Font::ComputeLineLayout`, rather than estimated from glyphs. The original fonts cover code points up
to 169 (©). A missing middle dot uses a centered period bitmap; other missing characters use a visible
replacement and are reported by `TextLayout::missing_characters`. See [FONTS.md](FONTS.md).

**Verified:** the main menu renders from `Dialog\MainMenu.rsc:main` with its translucent border, seven
float buttons with icons, and labels from `StringTable:mainMenu` (New, Load, Tutorials,
Achievements, Options, Mods, Exit).

**Additional controls and input (2026-10-14):** sliders, scrollbars, clipped containers, tabs/pages,
radio/check buttons, tables with wildcard cell templates, relative layouts, edits/number edits,
spinners, dropdowns, progress bars, graphs and rich documents now produce runtime layout/draw data.
The rich text decoder consumes its complete payload and supports native caret markup, font/color
changes, wrapping, bullets, inline sprites, captured images and link hit regions.

Pointer input, keyboard focus/Tab/Shift-Tab, edit cursor operations, slider keys, list navigation,
shortcuts, tooltip templates and link events are wired through the viewer. Named dialog substitutions
(including null suppression) and controller bindings resolve to runtime elements; `DialogInstance::dispatch`
delivers events to host callbacks. The viewer supplies mode shortcuts 1–4 and dialog OK/Cancel use
Enter/Escape. `TextDescription` field 0x8c is an ellipsis width, **not a keyboard binding**; the preceding
booleans are shadow and wrapping. Text shadows and ellipsis now follow those decoded fields.

`crates/ui/tests/controls.rs` exercises all 1,439 element resources and actual Options/NewGame controls,
plus dropdown selection, clipping, Unicode edits, numeric limits, hidden/disabled input, tooltips,
named substitutions and binding dispatch. Document tests process 790 shipped strings and all 192
SpriteCapture resources. Native Metal captures were inspected, and Chrome exercised tabs, sliders,
keyboard input and text editing. See [UI.md](UI.md) and [BROWSER.md](BROWSER.md).

**Fidelity limits:** host controllers still supply game state, combo/list choices, graph series and
progress values. Application-specific entity/build/toolbar controllers belong to the simulation
milestones; dispatching a controller callback does not implement that simulation. Exact screenshot
parity with the original executable has not been established. Dialog previews group resources by
dialog name and show a second state/resource list for each selected group. Loading variants resolve
to one representative preview, with controls for playing,
pausing and stepping through all 60 frames at the decoded 30 fps. SaveLoad defaults to its explicit
load-confirmation dialog instead of the generic Save/Load screen, which combines unrelated button
states; its loading animation is therefore absent from that idle preview. The SaveLoad preview
suppresses text elements that overlap within the same parent. Custom rotated SpriteCapture parts
draw axis aligned with a diagnostic. Shipped captures have no rotation. Arbitrary unsupported
scripts display replacement glyphs rather than a new font.

**Model browser additions (2026-10-14):** model, texture and audio lists now filter through the
package directory categories. Model previews can hide material textures, render indexed vertices as
points, and play/pause, step through, or seek shipped mesh animation clips. AnimationGroup and
Animation samples were decoded from the format and Ghidra traces of `Animation::GetPoseAtTime` and
`AnimationState::UpdateNodes`; all 15 shipped groups and 64 clips load, and clips are offered only
when their mesh path and bone count match. This preview uses sampled poses and does not implement
the game's animation blending, root-motion controllers, or event callbacks.

## 7. Audio

All 72 `SoundBuffer`s are **xWMA** (WMA v2, tag 0x161, 48 kbps). Each holds a WAVEFORMATEX, a `dpds`
packet table and packets, or for the 16 music tracks a reference to a raw `.str` package entry.
`SoundBuffer::to_xwma()` writes a standard RIFF/XWMA file.

`crates/wma` is a pure-Rust WMA v2 decoder (built in a parallel session). It matches FFmpeg on all
72 sounds and builds for WASM. The asset browser decodes in-process and plays through rodio
natively and Web Audio in the browser. Chrome checks cover mono, streamed stereo and looping audio,
nonzero live graph output, Stop silence and context cleanup on package reload; see [BROWSER.md](BROWSER.md).

## 5. Analysis artefacts

Ghidra exports (decompiled C, functions, strings, imports and exports per binary) live in `analysis/`
(git-ignored). To regenerate them, run `tools/ghidra/export.sh`, which wraps
`analyzeHeadless <proj> banished -import <binaries> -scriptPath tools/ghidra -postScript ExportAll.java <outdir>` with
`GHIDRA_INSTALL_DIR=/opt/homebrew/opt/ghidra/libexec`.

Gameplay-specific string-reference, explicit-address and virtual-table exporters are also available
under `tools/ghidra/`. Their first static findings, binary hashes, repeatability notes and open tracing
paths are recorded separately in [GAMEPLAY.md](GAMEPLAY.md); generated pseudocode remains local and
git-ignored.
