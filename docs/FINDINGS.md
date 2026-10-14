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

## 3. Resource format (`.crs`) — partially observed

Decompressed resources are serialized engine objects (`IO::Serializer`). The sample
`Game/Toolbar_storage.crs` starts with a `0xaaaaaaaa` marker, a size and a 32-bit type hash
(`0x5412a91f`), followed by fields that include cross-references written as `Path\File.rsc:objectName`
strings.

Resource names carry a component suffix (`Name_component.crs`). The most common are `resource` (1,717),
`edge`, `model`, `map`, `work`, `value`, `ps`/`vs` (pixel/vertex shaders), `toolbar`,
`picking`, `statemachine`, `rawmaterial`, `storage`, `zone` and `interact`.

**Port implication:** gameplay is substantially **data-driven**. Buildings, citizens and resources are
component bundles in `Game/` and `Template/` (2,700 files), and the exe implements one
`*Description` class per component type (`MarketDescription`, `OrchardDescription`, `DiseaseDescription`
and so on). A port reimplements those component types and loads the original data unchanged.

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
2. `.crs` deserializer: type-hash table → field schemas, starting with `Material`, `Model`, `Terrain`.
3. Model/terrain viewer in the browser (WebGPU), which proves the asset pipeline end to end.
4. UI system rendering the main menu from `Dialog/MainMenu.rsc`.
5. Map generation and terrain from `Terrain/` plus the Perlin-noise code.
6. Simulation components, one at a time (resources → storage → work → citizens → seasons).
7. Save/load, audio, polish.

## 5. Analysis artefacts

Ghidra exports (decompiled C, functions, strings, imports and exports per binary) live in `analysis/`
(git-ignored). To regenerate them, run `tools/ghidra/export.sh`, which wraps
`analyzeHeadless <proj> banished -import <binaries> -scriptPath tools/ghidra -postScript ExportAll.java <outdir>` with
`GHIDRA_INSTALL_DIR=/opt/homebrew/opt/ghidra/libexec`.
