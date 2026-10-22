# 4. Port a behaviour to Rust

Porting means writing Rust code that does what the original functions do. Start only when the
behaviour is traced ([Trace](TRACE.md)) and when Un-Banished can read every resource it needs
([Decode a format](FORMAT.md)). If you notice that you are inventing behaviour, stop and trace
it.

## Where the code goes

| Kind of code | Where | Rules |
|---|---|---|
| Exact maths, random numbers, noise, game time | `crates/core` | No game rules, no data files |
| Map generation, terrain mesh, ground queries | `crates/terrain` | Exact; tested against the emulator |
| Path finding | `crates/path` | Exact; tested against the emulator |
| Game rules, the simulation, generators | `crates/game` | No graphics, windows or sound; deterministic; unit and parity tests |
| Reading resources and saves | `crates/formats` | No dependencies |
| Interface elements, controls, layout, input | `crates/ui` | General code only; no rules of the game |
| Engine loops, game controllers, input routing | `apps/game/src/` (`loops.rs`, `menu.rs`, `gameplay/`) | Follow the original's controllers |
| Dialog controllers (the game's windows) | `apps/game/src/ui/` | One module for each original controller (`toolbar.rs`, `town_hall_ui.rs`, …) |
| Rendering | `apps/game/src/gfx/` (`render.rs`, `scene_gfx.rs`, `models.rs`, `*.wgsl`) | Must also run on WebGL 2 ([Port a shader](SHADER.md)) |

## Keep the original's shape

- **Use the original's names.** Name modules and types after the original classes and resources
  (`ToolbarItem`, `GameplayLoop`).
- **Name the source.** Write the doc comment in plain English for readers of the API
  documentation, and end it with one line that names the original function. Offsets, vtables and
  `DAT_` constants go in an ordinary `//` comment, where the tracing tools still find them:
  ```rust
  // The speed level is Game +0x34.
  /// The next game speed: 1, 2, 5, then 10.
  ///
  /// Original: `FUN_1400be680`.
  fn next_speed(level: usize) -> usize { … }
  ```
  [The doc-comment style](../API.md#the-doc-comment-style) explains the rule, and
  `python3 tools/triage/doc_comments.py check` finds doc lines that break it.
- **Keep the data flow.** If the original builds a dialog from `Dialog/X.rsc` and fills it from
  resources, do the same. Do not build an interface in code that the game ships as a resource.
- **Keep the constants.** Write each constant with its source address. Do not round or "tidy"
  numbers.
- **Mark differences.** A deliberate difference (a platform limit, WebGL 2, one thread in the
  browser instead of several) gets a comment with `Deviation:` and the reason. After the change,
  run `python3 tools/triage/deviations.py` to update [DEVIATIONS.md](../../DEVIATIONS.md).

## Exact arithmetic

The details that drift are the ones that cause differences later:

- **Float order.** Float addition and multiplication are not associative. Write the terms in the
  original's order.
- **Runtime maths.** In simulation code, use `unbanished_core::crt` (`sinf`, `cosf`, `acosf`,
  `atan2f`) and `unbanished_core::math` (normalise, heading, `Transform`), not Rust's `f32`
  methods. Rust's `f32::sin` can give a different last bit.
- **Game time.** `System::Time` is integer microseconds. Convert with `scheduler::seconds` and
  `scheduler::ticks`.
- **Order of iteration.** The order in which objects are visited, the order of calls to the
  game's `Random`, and the instability of the C runtime's `qsort` can all be observed in the
  results. `behaviour::ai::crt_qsort_by` reproduces `qsort`.
- **No hidden non-determinism.** In `crates/game`, do not use the clock, do not let the order of
  a `HashMap` reach the output, and always use the ported `Random`.

## The engine loops

`apps/game/src/loops.rs` holds the engine's loop model. `MainLoop` runs one loop at a time:
the splash screen, the menu, the loader, the game. `LoaderLoop` and `DialogLoaderLoop` load the
next loop in small steps, because the browser cannot block. Fades run at 4 units per second.

Each loop owns a `Screen`, which holds its dialogs:

| Method | What it does |
|---|---|
| `add_dialog(ui, "Dialog/X.rsc:resource")` | Instantiate a packaged dialog |
| `add_modal(ui, reference)` | The same, as a modal dialog |
| `remove_dialog(ui, index)` | Remove a dialog |
| `find(ui, dialog, name)` | Find an element of a dialog by its name |
| `layout(ui, size)` | Lay out every dialog |

The game itself is `GameplayLoop` in `gameplay.rs`. In the original, `FUN_14009d530` builds it and
`FUN_14009d0d0` installs its interface: the toolbar, the event notifications and the autosave.

## Packaged dialogs

The game's windows are resources (`Dialog/*.rsc`). To port one:

1. Look at the resource: its element names, ids, templates and alignment.
   ```bash
   cargo run -p unbanished-formats --example dialog_info -- game-data/original/WinData \
     "Dialog/Toolbar.rsc:resource"
   ```
2. Instantiate it through the screen. Find its elements **by name**, as the original does
   (`screen.find(ui, dialog, "toolbarContent")`), never by their position in the tree.
3. Use the helpers of `unbanished_ui::Ui` (`set_text`, `set_table_child`, `set_images`, `checked`,
   `set_checked`, …). If a helper is missing, add a general one to `crates/ui/src/controls.rs`,
   with a test in `crates/ui/tests/controls/`.
4. Strings come from string tables (`UI::String(table, key)`); sprites come from sprite sheets
   (`ui.res.sprite(sheet, name)`).
5. Handle events: drain them with `drain_events` and dispatch on the original element ids and
   event codes that the trace recorded. Note that `unbanished_ui::pointer_button` **returns** the
   click event; it does not queue it.

## Work in small slices

1. Read the trace document again. List the functions to port and their addresses.
2. Make the smallest piece work from end to end first, for example "the toolbar's first level
   is visible". Then add the rest.
3. Build often: `cargo build --release -p unbanished-app`, or `-p unbanished-ui` for a crate.
4. Verify ([Verify](VERIFY.md)), then document and commit ([Document and land](LAND.md)), for
   each slice.

## Rules for the browser

The same code runs in the browser. So:

- no `std::thread` in code that the browser runs;
- no blocking file access;
- use `web_time::Instant` instead of `std::time::Instant`.
