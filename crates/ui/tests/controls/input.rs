//! Pointer and keyboard input: tabs, sliders, edits, combos, spinners and blocked ancestors.

use super::*;

#[test]
fn options_tabs_reveal_sound_controls_and_slider_accepts_input() {
    let Some(mut ui) = game_ui() else { return };
    let root = ui.instantiate(r"Dialog\Options.rsc:main").unwrap();
    ui.layout_dialog(root, [0, 0, 1280, 720]);
    let tab = ui.find(root, "tabOptions").unwrap();
    let sound = ui.find(root, "buttonSound").unwrap();
    click(&mut ui, root, sound);
    ui.layout_dialog(root, [0, 0, 1280, 720]);
    assert!(matches!(
        ui.nodes[tab].kind,
        Kind::Control(Control::Tabs { selected: 2, .. })
    ));
    let music = ui.find(root, "sliderMusic").unwrap();
    assert!(ui.nodes[music].rect[2] > ui.nodes[music].rect[0]);
    click(&mut ui, root, music);
    let before = ui.value(music).unwrap();
    ui.key(root, Key::Right);
    assert!(
        ui.value(music).unwrap() > before,
        "focused music slider must advance"
    );
    let range = match &ui.nodes[music].kind {
        Kind::Control(Control::Slider(s)) => s.range,
        _ => panic!("music slider"),
    };
    ui.set_value(music, f32::MAX);
    assert_eq!(ui.value(music), Some(range[0].max(range[1])));
    ui.set_value(music, f32::NAN);
    assert!(ui.value(music).unwrap().is_finite());
    assert!(!ui.draw(root).is_empty());
}

/// `SliderElement::Update`: the track pages by a step, the knob drags, and a drag more than
/// 32 px away puts the value back.
#[test]
fn slider_track_steps_and_knob_drags() {
    let Some(mut ui) = game_ui() else { return };
    let root = ui.instantiate(r"Dialog\Options.rsc:main").unwrap();
    ui.layout_dialog(root, [0, 0, 1280, 720]);
    let sound = ui.find(root, "buttonSound").unwrap();
    click(&mut ui, root, sound);
    ui.layout_dialog(root, [0, 0, 1280, 720]);
    let music = ui.find(root, "sliderMusic").unwrap();
    let slider = |ui: &Ui| match &ui.nodes[music].kind {
        Kind::Control(Control::Slider(s)) => s.clone(),
        _ => panic!("music slider"),
    };
    ui.set_value(music, slider(&ui).range[0]);
    let rect = ui.nodes[music].rect;
    let knob = slider(&ui).knob(rect);
    // A press right of the knob: one step.
    let (x, y) = (rect[2] - 2, (knob[1] + knob[3]) / 2);
    ui.pointer_move(x, y, root);
    ui.pointer_button(true, root);
    ui.pointer_button(false, root);
    let s = slider(&ui);
    assert!(
        (s.value - (s.range[0] + s.step())).abs() < 1e-4,
        "{}",
        s.value
    );
    // Drag the knob right by 20 px, then far away (back to the start), then release near.
    let knob = s.knob(rect);
    let (kx, ky) = ((knob[0] + knob[2]) / 2, (knob[1] + knob[3]) / 2);
    ui.pointer_move(kx, ky, root);
    ui.pointer_button(true, root);
    ui.pointer_move(kx + 20, ky, root);
    let dragged = slider(&ui).value;
    assert!(dragged > s.value);
    ui.pointer_move(kx + 20, ky + 200, root);
    assert_eq!(slider(&ui).value, s.value);
    ui.pointer_move(kx + 20, ky, root);
    ui.pointer_button(false, root);
    assert_eq!(slider(&ui).value, dragged);
    assert!(slider(&ui).drag.is_none());
}

#[test]
fn new_game_name_edit_enforces_length_and_edits_at_cursor() {
    let Some(mut ui) = game_ui() else { return };
    let edit = ui.instantiate(r"Dialog\NewGame.rsc:editName").unwrap();
    ui.layout_dialog(edit, [0, 0, 640, 100]);
    click(&mut ui, edit, edit);
    ui.text_input("ABCDEFGHIJKLMNOPQRSTUV");
    let text = |ui: &Ui| match &ui.nodes[edit].kind {
        Kind::Control(Control::Edit(e)) => e.text.text.clone(),
        _ => panic!("name edit"),
    };
    assert_eq!(text(&ui), "ABCDEFGHIJKLMNOPQR");
    ui.key(edit, Key::Home);
    ui.key(edit, Key::Delete);
    ui.key(edit, Key::End);
    ui.key(edit, Key::Backspace);
    assert_eq!(text(&ui), "BCDEFGHIJKLMNOPQ");
    assert!(!ui.draw(edit).is_empty());
}

#[test]
fn combo_selects_pointer_choice_and_escape_closes_dropdown() {
    let Some(mut ui) = game_ui() else { return };
    let combo = ui.instantiate(r"Dialog\NewGame.rsc:comboTerrain").unwrap();
    ui.set_choices(
        combo,
        vec!["Valleys".into(), "Mountains".into(), "Plains".into()],
    );
    ui.layout(combo, [0, 0, 320, 48]);
    click(&mut ui, combo, combo);
    ui.layout(combo, [0, 0, 320, 48]);
    let list = match ui.nodes[combo].kind {
        Kind::Control(Control::Combo {
            open: true, list, ..
        }) => list,
        _ => panic!("dropdown did not open"),
    };
    let row_height = match &ui.nodes[list].kind {
        Kind::List(l) => l.row_height,
        _ => unreachable!(),
    };
    let rc = ui.nodes[list].rect;
    ui.pointer_move(
        (rc[0] + rc[2]) / 2,
        rc[1] + row_height + row_height / 2,
        combo,
    );
    let event = ui.pointer_button(true, combo);
    ui.pointer_button(false, combo);
    assert_eq!(event, Some(Event::Selected(combo, 1)));
    assert!(matches!(
        ui.nodes[combo].kind,
        Kind::Control(Control::Combo { open: false, .. })
    ));
    assert!(matches!(&ui.nodes[list].kind,Kind::List(l) if l.selected==Some(1)));
    click(&mut ui, combo, combo);
    assert!(matches!(
        ui.nodes[combo].kind,
        Kind::Control(Control::Combo { open: true, .. })
    ));
    assert!(ui.key(combo, Key::Escape));
    assert!(matches!(
        ui.nodes[combo].kind,
        Kind::Control(Control::Combo { open: false, .. })
    ));
}

#[test]
fn tab_focus_and_ascii_cursor_editing_emit_changes() {
    let Some(mut ui) = game_ui() else { return };
    let root = ui.group();
    let edit = ui.instantiate(r"Dialog\NewGame.rsc:editName").unwrap();
    ui.add_child(root, edit);
    ui.layout(root, [0, 0, 640, 100]);
    assert!(ui.key(root, Key::Tab));
    assert_eq!(ui.focused(), Some(edit));
    // `EditElement::SelectCharacter` types printable ASCII but `:` and `;`.
    assert!(ui.text_input("Aé🦀:;xB"));
    let text = |ui: &Ui| match &ui.nodes[edit].kind {
        Kind::Control(Control::Edit(e)) => e.text.text.clone(),
        _ => unreachable!(),
    };
    assert_eq!(text(&ui), "AxB");
    ui.key(root, Key::Left);
    ui.key(root, Key::Backspace);
    assert_eq!(text(&ui), "AB");
    ui.key(root, Key::Home);
    ui.key(root, Key::Delete);
    assert_eq!(text(&ui), "B");
    assert!(
        ui.drain_events()
            .iter()
            .filter(|e| **e == Event::Changed(edit))
            .count()
            >= 3
    );
    // Escape gives up the focus with event 0x15.
    ui.key(root, Key::Escape);
    assert_eq!(ui.focused(), None);
    assert!(ui.drain_events().contains(&Event::Submitted(edit)));
}

#[test]
fn hidden_and_disabled_ancestors_block_pointer_and_shortcuts() {
    let Some(mut ui) = game_ui() else { return };
    let root = ui.group();
    let button = ui.instantiate(r"Dialog\Options.rsc:buttonSound").unwrap();
    ui.add_child(root, button);
    ui.set_shortcut(button, 'S' as u32);
    ui.layout(root, [0, 0, 320, 80]);
    let [l, t, r, b] = ui.nodes[button].rect;
    ui.nodes[root].enabled = false;
    ui.pointer_move((l + r) / 2, (t + b) / 2, root);
    assert_eq!(ui.pointer_button(true, root), None);
    assert_eq!(ui.pointer_button(false, root), None);
    assert!(!ui.key(root, Key::Character('s')));
    ui.nodes[root].enabled = true;
    ui.set_visible(root, false);
    ui.pointer_move((l + r) / 2, (t + b) / 2, root);
    assert_eq!(ui.pointer_button(true, root), None);
    assert_eq!(ui.pointer_button(false, root), None);
    assert!(!ui.key(root, Key::Character('s')));
    ui.set_visible(root, true);
    assert!(ui.key(root, Key::Character('s')));
    assert!(ui.drain_events().contains(&Event::Clicked(button)));
    // `ButtonElement::Update`: leaving a pressed button lets it go.
    ui.pointer_move((l + r) / 2, (t + b) / 2, root);
    ui.pointer_button(true, root);
    ui.pointer_move(r + 50, b + 50, root);
    ui.pointer_move((l + r) / 2, (t + b) / 2, root);
    assert_eq!(ui.pointer_button(false, root), None);
}

#[test]
fn spinner_respects_typed_value_range_and_stacks_its_arrow_buttons() {
    let Some(mut ui) = game_ui() else { return };
    let root = ui
        .instantiate(r"Dialog\Trade.rsc:spinnerPurchaseDesired")
        .unwrap();
    ui.layout_dialog(root, [0, 0, 400, 200]);
    let parts = match ui.nodes[root].kind {
        Kind::Control(Control::Spinner { parts, .. }) => parts,
        _ => panic!("spinner"),
    };
    let [edit, up, down] = parts.map(Option::unwrap);
    assert!(ui.nodes[up].rect[3] <= ui.nodes[down].rect[1]);
    ui.set_value(edit, 42.0);
    click(&mut ui, root, up);
    assert_eq!(ui.value(edit), Some(62.0));
    click(&mut ui, root, down);
    assert_eq!(ui.value(edit), Some(42.0));
    ui.set_value(edit, 9990.0);
    click(&mut ui, root, up);
    assert_eq!(ui.value(edit), Some(9999.0));
}

#[test]
fn combo_keyboard_browsing_keeps_popup_open_until_confirmation() {
    let Some(mut ui) = game_ui() else {
        return;
    };
    let root = ui.instantiate(r"Dialog\Options.rsc:comboAdapter").unwrap();
    ui.set_choices(root, vec!["First".into(), "Second".into(), "Third".into()]);
    ui.layout_dialog(root, [0, 0, 600, 300]);
    click(&mut ui, root, root);
    assert!(ui.key(root, Key::Down));
    assert!(ui.key(root, Key::Down));
    let list = match ui.nodes[root].kind {
        Kind::Control(Control::Combo {
            list, open: true, ..
        }) => list,
        _ => panic!("open combo"),
    };
    assert!(matches!(&ui.nodes[list].kind,Kind::List(l) if l.selected==Some(1)));
    ui.drain_events();
    assert!(ui.key(root, Key::Enter));
    assert!(matches!(
        ui.nodes[root].kind,
        Kind::Control(Control::Combo { open: false, .. })
    ));
    assert_eq!(
        ui.drain_events(),
        vec![unbanished_ui::Event::Selected(root, 1)]
    );
}

#[test]
fn an_edit_that_allows_a_colon_takes_it() {
    let Some(mut ui) = game_ui() else { return };
    let root = ui.group();
    let edit = ui.instantiate(r"Dialog\NewGame.rsc:editName").unwrap();
    ui.add_child(root, edit);
    ui.layout(root, [0, 0, 640, 100]);
    if let Kind::Control(Control::Edit(e)) = &mut ui.nodes[edit].kind {
        e.allowed = (' '..='~').collect();
        e.max_length = 40;
        e.text.text.clear();
        e.cursor = 0;
    }
    assert!(ui.key(root, Key::Tab));
    assert!(ui.text_input("ws://127.0.0.1:7780"));
    match &ui.nodes[edit].kind {
        Kind::Control(Control::Edit(e)) => assert_eq!(e.text.text, "ws://127.0.0.1:7780"),
        _ => unreachable!(),
    }
}
