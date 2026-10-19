---
name: banished-port
description: Port traced Banished behaviour into the Rust reimplementation — game logic in crates/game, engine loops/controllers/dialogs in apps/game, packaged UI dialogs via crates/ui. Use when implementing a feature whose original behaviour has been traced (toolbar, status bar, utility windows, placement, calendar, start conditions, citizens, menus) so the code mirrors the original structure 1:1.
---

# Port to Rust, 1:1

Precondition: the behaviour is traced (`banished-trace`) and any resource it reads parses
(`banished-format`). If you find yourself inventing behaviour, stop and trace it.

## Where code goes

| Kind | Location | Constraints |
|---|---|---|
| Deterministic game logic, simulation, generators, queries | `crates/game` | No wgpu/winit/audio; bit-exact where emulated; unit/parity tests |
| Resource parsing | `crates/formats` | Dependency-free |
| UI element tree, controls, layout, input, events | `crates/ui` | Generic engine UI — no Banished-specific game rules |
| Engine loops, game controllers, dialog controllers, input routing | `apps/game/src/*.rs` | One module per original controller (`toolbar.rs`, `status_bar.rs`, …) |
| Rendering | `apps/game/src/gfx/render.rs`, `scene_gfx.rs`, `models.rs`, `*.wgsl` | Must run on WebGL2 too (see `banished-shader`) |

## Mirror the original's shape

- Name modules/structs after the original classes or resources (`ToolbarItem`, `GameplayLoop`), and
  put the original address on each ported function:
  `/// Port of FUN_1400be680 (speed step): speeds 1, 2, 5, 10.`
- Keep the original's data flow: if the original builds a dialog from `Dialog/X.rsc` and fills it
  from resources, do the same — **don't hand-build UI that the game ships as a resource**.
- Keep constants with their source address; don't "tidy" magic numbers into different values.
- Mark deliberate deviations (platform differences, WebGL limits, threads → incremental steps)
  with `// Deviation:` and a reason.

## Engine loop model (`apps/game/src/loops.rs`)

`MainLoop` runs one loop at a time; `LoaderLoop`/`DialogLoaderLoop` load the next loop
incrementally (no threads — the web can't block). Fades run at 4 units/s. A loop owns a `Screen`:
`add_dialog(ui, "Dialog/X.rsc:resource")`, `add_modal`, `remove_dialog`, `find(name)`, `layout`,
`input_root`. The game loop is `GameplayLoop` in `gameplay.rs` (built by `FUN_14009d530`, UI
installed by `FUN_14009d0d0`: Toolbar, EventNotify, AutoSave).

## Packaged dialogs (`crates/ui`)

1. Inspect the resource: `cargo run -p banished-formats --example dialog_info -- game-data/original/WinData "Dialog/Toolbar.rsc:resource"`
   to see element names, ids, templates, alignment.
2. Instantiate via the screen, then look elements up by **name** as the original does
   (`find("toolbarContent")`), never by tree position.
3. Use the `Ui` helpers (`set_text`, `set_table_child`, `set_images`, `checked`/`set_checked`, …);
   add a generic helper to `crates/ui/src/controls.rs` if one is missing, with a test in
   `crates/ui/tests/controls.rs`.
4. Strings: `UI::String(table, key)` → the string-table lookup; sprites: `ui.res.sprite(sheet, name)`.
5. Events: `drain_events` and dispatch on the original element ids / event codes recorded in the
   trace doc. Note `banished_ui::pointer_button` **returns** the click event (not queued).

## Workflow

1. Re-read the trace doc; list the functions to port and their addresses.
2. Implement the smallest end-to-end slice (e.g. toolbar root level visible) before the rest.
3. Build often: `cargo build --release -p banished-app` (and `-p banished-ui` etc. for crates).
4. Verify (`banished-verify`), then document and commit (`banished-land`) — per slice, not at the end.

## Pitfalls

- Other sessions may be editing the same app files; re-read a file before editing if a
  system note says it changed, and keep edits scoped.
- Keep `crates/game` deterministic: no wall-clock time, no `HashMap` iteration order in outputs,
  use the ported `Random`.
- WASM: no `std::thread`, no blocking file IO, `web_time::Instant` instead of `std::time::Instant`.
