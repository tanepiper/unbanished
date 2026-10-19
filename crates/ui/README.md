# banished-ui

A reimplementation of Banished's interface system (`UI::Element` and the classes around it, in
`Runtime-steam-x64.dll`). The game ships its windows as resources (`Dialog/*.rsc`): trees of
elements with sprite sheets, fonts and string tables. This crate builds those trees, sizes and
places the elements as the original does, handles the pointer and the keyboard, and produces a
list of textured quads that a renderer draws.

- Sizes are computed bottom-up and positions top-down, as `ComputeSize` and `ComputeLayout` do.
- Text layout follows `UI::Font::ComputeStringLayout`; documents with markup (the help and the
  tutorials) follow `DocumentTextElement`.
- The controls are the original's: buttons, check boxes, lists, combo boxes, sliders, edit
  boxes, tables, graphs, tooltips.
- The crate contains no rules of the game. A dialog's controllers (the code behind a window) are
  the application's; the crate tells the application which controller is attached to which
  element.
- It does not draw. `banished-gfx` draws its quads with wgpu.

## Use it

```toml
[dependencies]
banished-ui = { path = "crates/ui" }
banished-formats = { path = "crates/formats" }
```

### Show a dialog

```rust
use std::rc::Rc;
use banished_formats::pkg::Archive;
use banished_ui::{Quad, Ui};

fn main_menu(archive: Rc<Archive>) -> Option<Vec<Quad>> {
    let mut ui = Ui::new(archive);
    ui.screen = [1280, 720];
    // Build the element tree of a packaged dialog.
    let dialog = ui.instantiate_dialog("Dialog/MainMenu.rsc:resource")?;
    // Size the elements and place the dialog by its alignment on a 1280 × 720 screen.
    ui.layout_dialog(dialog.root, [0, 0, 1280, 720]);
    // Find an element by the name it has in the resource.
    let new_game = ui.find(dialog.root, "buttonNew")?;
    println!("New Game is at {:?}", ui.node(new_game).rect);
    // The quads to draw, in order.
    Some(ui.draw(dialog.root))
}
```

Each `Quad` has a texture (a sprite sheet, or an application texture), a rectangle in pixels,
texture coordinates, a colour, an optional clip rectangle and a rotation.

### Handle input

```rust
use banished_ui::{Event, NodeId, Ui};

fn click(ui: &mut Ui, root: NodeId, x: i32, y: i32) {
    ui.pointer_move(x, y, root);
    ui.pointer_button(true, root);
    // The click event is returned on release; other events are queued.
    if let Some(Event::Clicked(id)) = ui.pointer_button(false, root) {
        println!("clicked {}", ui.node(id).name);
    }
    for event in ui.drain_events() {
        println!("{event:?}");
    }
}
```

`Event` is `Clicked`, `Selected` (a list item), `Changed` (a check box, slider or edit box),
`Submitted` (Enter in an edit box) or `Link` (a link in a document). `ui.key(root, key)` passes a
key press; `banished_ui::Key` lists the keys.

### Change what a dialog shows

```rust
use banished_ui::{NodeId, Ui};

fn fill(ui: &mut Ui, root: NodeId) {
    if let Some(title) = ui.find(root, "textTitle") {
        ui.set_text(title, "Hello");
    }
    if let Some(list) = ui.find(root, "listSaves") {
        ui.set_list_items(list, vec!["Town one".into(), "Town two".into()]);
        ui.list_select(list, Some(0));
    }
}
```

Other helpers: `set_visible`, `set_icon`, `select_combo_item`, `set_list_icons`, and the
control accessors in `controls.rs`. Strings come from the string tables through
`ui.res.string(table, key)`; sprites through `ui.res.sprite(sheet, name)`.

### Controllers

A `DialogInstance` lists its `bindings`: which controller type is attached to which element, with
its configuration. The application implements the controllers. `DialogInstance::dispatch`
delivers an event to every controller on the element or on one of its parents.

## Source files

| File | What it contains |
|---|---|
| `src/lib.rs` | `Ui`, `Node`, `Kind`, `Quad`, `Event`; instantiation, layout, drawing |
| `src/dialog.rs` | `DialogInstance` and `Binding`: template substitution and controllers |
| `src/controls.rs` | The stateful controls and their helpers |
| `src/input.rs` | The pointer, the keyboard, focus and tooltips |
| `src/text.rs` | Text layout |
| `src/document.rs` | Documents with markup |
| `src/resources.rs` | A cache of sprite sheets, fonts, element descriptions and strings |

## Tests

```bash
cargo test --release -p banished-ui
```

`tests/controls.rs` checks the controls with your game data, and skips without it.

## The API

The full API documentation is at
<https://tanepiper.github.io/unbanished/api/banished_ui/index.html>.

## Related documents

- [The interface](https://tanepiper.github.io/unbanished/topics/interface)
- [UI.md](https://github.com/tanepiper/unbanished/blob/main/docs/UI.md), [FONTS.md](https://github.com/tanepiper/unbanished/blob/main/docs/FONTS.md), [TOOLTIPS.md](https://github.com/tanepiper/unbanished/blob/main/docs/TOOLTIPS.md)
