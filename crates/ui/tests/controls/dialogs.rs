//! Dialogs: named overrides and bindings, tooltips and the new-game dialog.

use super::*;

#[test]
fn dialog_named_overrides_suppress_slots_and_dispatch_binding_callbacks() {
    let Some(mut ui) = game_ui() else { return };
    let d = ui
        .instantiate_dialog(r"Dialog\Options.rsc:inputBind")
        .unwrap();
    assert!(ui.find(d.root, "inputBindText").is_some());
    assert!(ui.find(d.root, "buttonYes").is_some());
    assert!(ui.find(d.root, "buttonNo").is_none());
    assert!(ui.find(d.root, "buttonOk").is_none());
    ui.layout_dialog(d.root, [0, 0, 1280, 720]);
    assert!(!ui.draw(d.root).is_empty());
    let bound = ui
        .instantiate_dialog(r"Dialog\ToolTipTitleDesc.rsc:resource")
        .unwrap();
    assert_eq!(bound.bindings.len(), 1);
    let source = *ui.nodes[bound.root]
        .children
        .first()
        .expect("tooltip child");
    let event = Event::Changed(source);
    let mut callbacks = Vec::new();
    bound.dispatch(&ui, event, |binding, event| {
        callbacks.push((binding.controller, event))
    });
    assert_eq!(callbacks, vec![(870715395, event)]);
    let unrelated = ui.group();
    bound.dispatch(&ui, Event::Changed(unrelated), |_, _| {
        panic!("unrelated event reached controller")
    });
}

#[test]
fn hover_tooltip_adds_text_and_pressing_suppresses_it() {
    let Some(mut ui) = game_ui() else { return };
    let button = ui.instantiate(r"Dialog\Options.rsc:buttonSound").unwrap();
    ui.nodes[button].tooltip = "Adjust the volume of music and sound effects".into();
    ui.nodes[button].tooltip_dialog = Some(r"Dialog\ToolTip.rsc:resource".into());
    ui.layout(button, [0, 0, 320, 80]);
    ui.pointer_move(-1, -1, button);
    let idle = ui.draw(button);
    let [l, t, r, b] = ui.nodes[button].rect;
    ui.pointer_move((l + r) / 2, (t + b) / 2, button);
    assert_eq!(
        ui.hovered_tooltip(),
        Some("Adjust the volume of music and sound effects")
    );
    // The generic tooltip dialog: its border panel and the text.
    let hovered = ui.draw(button);
    assert!(
        hovered.len() > idle.len() + 9,
        "tooltip panel and text must reach renderer"
    );
    ui.pointer_button(true, button);
    assert!(ui.draw(button).len() < hovered.len());
}

/// `Element::IsToolTipValid`: tooltip text without a tooltip dialog shows nothing.
#[test]
fn tooltips_need_a_dialog() {
    let Some(mut ui) = game_ui() else { return };
    ui.screen = [1280, 800];
    let button = ui.instantiate(r"Dialog\Options.rsc:buttonSound").unwrap();
    ui.nodes[button].tooltip = "Text".into();
    ui.nodes[button].tooltip_dialog = None;
    ui.layout(button, [0, 0, 320, 80]);
    ui.pointer_move(-1, -1, button);
    let idle = ui.draw(button).len();
    let [l, t, r, b] = ui.nodes[button].rect;
    ui.pointer_move((l + r) / 2, (t + b) / 2, button);
    assert_eq!(ui.draw(button).len(), idle);
}

/// The New Game dialog's combos fill their 160-pixel column (the button is the combo) and the
/// description's padded box surrounds its text (the border takes the text's slot).
#[test]
fn new_game_combos_fill_and_description_sits_in_its_box() {
    let Some(mut ui) = game_ui() else { return };
    ui.screen = [1280, 800];
    let d = ui
        .instantiate_dialog(r"Dialog\NewGame.rsc:resource")
        .unwrap();
    let desc = ui.find(d.root, "textDescription").unwrap();
    ui.set_parameters(desc, &[(0, "A medium game begins with five families. Clothing, food, firewood, tools, and construction materials, are provided.")]);
    ui.layout_dialog(d.root, [0, 0, 1280, 800]);
    let combo = ui.find(d.root, "comboTerrain").unwrap();
    let Kind::Control(Control::Combo { button, .. }) = ui.nodes[combo].kind else {
        panic!()
    };
    let frame = ui.children(button)[0];
    assert_eq!(ui.nodes[frame].rect, ui.nodes[combo].rect);
    let text = ui.nodes[desc].rect;
    let border = ui.nodes[ui.nodes[desc].border.unwrap()].rect;
    assert!(
        border[0] <= text[0]
            && border[1] <= text[1]
            && border[2] >= text[2]
            && border[3] >= text[3],
        "box {border:?} must surround text {text:?}"
    );
}
