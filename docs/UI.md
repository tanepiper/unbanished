# UI runtime continuation

The runtime now instantiates every decoded element class. It uses original sprite sheets and fonts,
with bottom-up size calculation, top-down placement and intersected scissor rectangles for clipped
content. Stateful controls live in `crates/ui/src/controls.rs`, input in `input.rs`, dialog composition
in `dialog.rs`, and caret-markup documents in `document.rs`.

## Host integration

`Ui::instantiate_dialog(reference)` loads a dialog, applies named element substitutions by path
identifier (null means omit the slot), and resolves controller attachments. `DialogInstance::root`
is the root to lay out and draw. Use `layout_dialog` for dialog placement and `layout` for a regular
host tree. Dialog roots should be placed separately from their host's ordinary child layout.

| Data or action | Runtime API |
| --- | --- |
| Slider/progress/scrollbar/numeric values | `set_value`, `value` |
| Combo choices | `set_choices` |
| List rows and selection | `set_list_items`, `list_select` on the list node |
| Graph samples | `set_graph_series` (nonfinite samples break line segments) |
| Label, edit or document content | `set_text` |
| Pointer interaction | `pointer_move`, `pointer_button`, `wheel` |
| Keyboard focus and editing | `key`, `text_input`, `focused`, `clear_input` |
| Application shortcuts | `set_shortcut` (uppercase character code or 13/27 for Enter/Escape) |
| Change, submit, selection and link events | `drain_events`, plus `pointer_button`'s return value |
| Rich document link target | `document_link` |
| Application controller callbacks | `DialogInstance::dispatch` |

Collect binding callbacks before mutating the UI; dispatch holds a shared reference to the tree.
Bindings contain the controller type ID, configuration reference and resolved element. Application
controllers own simulation data and behavior. The viewer routes events through these attachments
and reports preview actions; it does not create entities or apply game settings.

## Ghidra evidence

These are static observations from the existing Ghidra 12.1.4 exports of Runtime-steam-x64.dll,
not observations of the original game's execution. The implementation uses reconstructed behavior
and original resource data; no decompiled code or assets are committed.

| Behavior | Engine entry point |
| --- | --- |
| Slider track/knob/adjacent number | `SliderElement::Draw` 0x1800d95b0; `ComputeLayout` 0x1800d9830 |
| Progress fill/background strips | `ProgressElement::Draw` 0x18011bb50 |
| Scrollbar size/orientation | `ScrollElement::ComputeSize` 0x18011d610 |
| Tabs and body containers | `TabElement::ComputeLayout` 0x1800dc390 |
| Table row/column dimensions and cell template precedence | `TableElement::ComputeSize` 0x1801207f0; `TableDescription::CreateInstance` 0x180120be0 |
| Relative layout anchors | `LayoutElement::GetChildLayout` 0x1800d5630 |
| Numeric initial value, limits and digit count | `NumberEditDescription::Serialize` 0x1800d7750; `FinalizeText` 0x1800d72d0 |
| Spinner step and stacked arrows | `SpinnerElement::Update` 0x1800da6e0; `ComputeLayout` 0x1800da930 |
| Rich text payload and caret commands | `DocumentTextDescription::Serialize` 0x1800d2110; `GetNextItem` 0x1800d07b0 |
| Captured document images | `SpriteCapture::Serialize` 0x1800dbda0; record serializer 0x18004cbd0 |
| Named substitutions and controller attachment | `Dialog::CreateElement` 0x180115760; `DialogInstance::AddController` 0x180115d00 |
| Text shadow, wrapping, ellipsis and baseline | `TextElement::Draw` 0x180121b10; `ComputeSize` 0x1801218f0; `Font::ComputeLineLayout` 0x180119500 |

Exact original-game screenshot parity remains unverified. Keyboard focus conventions and pointer
editing are host behavior where engine parity has not been established. Middle-dot synthesis is an
approximation; other absent Unicode glyphs use visible replacements. Animated images show their
first frame and custom rotated captures are currently axis aligned. Original game controllers and
material effects remain separate work.
