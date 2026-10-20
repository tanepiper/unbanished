//! Integration coverage using the player's packages; skipped when no game install is present.
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::rc::Rc;

use banished_formats::pkg::{Archive, Package};
use banished_formats::{crs, ui as format};
use banished_ui::{Control, Event, Key, Kind, Node, NodeId, Ui};

fn game_ui() -> Option<Ui> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../game-data/original/WinData");
    if !dir.join("data0.pkg").exists() {
        eprintln!("game packages absent; skipping controls integration tests");
        return None;
    }
    let mut archive = Archive::default();
    for name in ["data1.pkg", "patch1.pkg", "patch0.pkg", "data0.pkg"] {
        archive.add(Package::parse(std::fs::read(dir.join(name)).unwrap()).unwrap());
    }
    Some(Ui::new(Rc::new(archive)))
}

fn click(ui: &mut Ui, root: NodeId, id: NodeId) {
    let [l, t, r, b] = ui.nodes[id].rect;
    assert!(
        r > l && b > t,
        "{} has no clickable area",
        ui.nodes[id].name
    );
    ui.pointer_move((l + r) / 2, (t + b) / 2, root);
    ui.pointer_button(true, root);
    ui.pointer_button(false, root);
}

#[test]
fn every_shipped_element_class_instantiates_lays_out_and_draws() {
    let Some(mut ui) = game_ui() else { return };
    let entries: Vec<_> = ui
        .res
        .archive
        .names()
        .into_iter()
        .map(str::to_owned)
        .collect();
    let mut classes = BTreeSet::new();
    let mut elements = 0;
    for entry in entries {
        if !entry.ends_with(".crs") {
            continue;
        }
        let bytes = ui.res.archive.read(&entry).unwrap();
        if !format::is_element(crs::header(&bytes).unwrap().type_id) {
            continue;
        }
        let element = format::element(&bytes).unwrap();
        classes.insert(element.class);
        let root = ui.instantiate(&entry).unwrap_or_else(|| panic!("{entry}"));
        assert!(
            ui.nodes
                .iter()
                .all(|n| !matches!(n.kind, Kind::Unsupported(_))),
            "{entry}"
        );
        ui.layout_dialog(root, [0, 0, 1280, 720]);
        let quads = ui.draw(root);
        assert!(
            quads.iter().all(|q| q.rect.iter().all(|v| v.is_finite())),
            "{entry}"
        );
        elements += 1;
        ui.nodes.clear();
    }
    assert_eq!(format::ELEMENT_CLASSES.len(), 28);
    let missing: Vec<_> = format::ELEMENT_CLASSES
        .iter()
        .copied()
        .filter(|class| !classes.contains(class))
        .collect();
    assert_eq!(missing, vec!["ClipRegionDescription"]);
    assert_eq!(classes.len(), 27);
    assert_eq!(elements, 1440);
}

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
fn progress_fill_is_clipped_to_its_fraction_and_clamped() {
    let Some(mut ui) = game_ui() else { return };
    let progress = ui.instantiate(r"Dialog\Storage.rsc:capacityFull").unwrap();
    ui.nodes[progress].flags |= format::flags::FILL_WIDTH;
    ui.layout(progress, [20, 20, 420, 60]);
    ui.set_value(progress, 0.0);
    let empty = ui.draw(progress);
    ui.set_value(progress, 0.25);
    let quarter = ui.draw(progress);
    assert!(
        quarter.len() > empty.len(),
        "positive progress adds fill geometry"
    );
    let [l, t, r, b] = ui.nodes[progress].rect;
    let fill_right = l + (r - l) / 4;
    let fill: Vec<_> = quarter
        .iter()
        .filter(|q| q.clip.is_some_and(|c| c == [l, t, fill_right, b]))
        .collect();
    assert!(
        !fill.is_empty(),
        "fill quads must carry fractional clipping"
    );
    assert!(fill.iter().all(|q| q.rect[2] <= fill_right as f32));
    ui.set_value(progress, 4.0);
    assert_eq!(ui.value(progress), Some(1.0));
    ui.set_value(progress, -1.0);
    assert_eq!(ui.value(progress), Some(0.0));
}

#[test]
fn overflowing_container_scrolls_content_and_preserves_its_clip() {
    let Some(mut ui) = game_ui() else { return };
    let root = ui.instantiate(r"Dialog\Options.rsc:gameOptions").unwrap();
    let body = ui.ribbon(true, 2);
    for i in 0..40 {
        let text = ui.text(r"Font\FontSmall.rsc:resource", &format!("Visible row {i}"));
        ui.add_child(body, text);
    }
    if let Kind::Control(Control::Container {
        content,
        size,
        offset,
        ..
    }) = &mut ui.nodes[root].kind
    {
        *content = Some(body);
        *size = [220, 100];
        *offset = [0, 150];
    } else {
        panic!("options container");
    }
    ui.layout_dialog(root, [0, 0, 800, 600]);
    let (view, bar, offset) = match ui.nodes[root].kind {
        Kind::Control(Control::Container {
            view, bars, offset, ..
        }) => (view, bars[1].unwrap(), offset),
        _ => unreachable!(),
    };
    assert_eq!(offset[1], 150);
    assert!(
        ui.nodes[body].rect[1] < view[1],
        "scroll moves content above its viewport"
    );
    assert!(
        ui.nodes[bar].rect[3] > ui.nodes[bar].rect[1],
        "overflow exposes a scrollbar"
    );
    let quads = ui.draw(root);
    let content_quads: Vec<_> = quads.iter().filter(|q| q.clip == Some(view)).collect();
    assert!(
        !content_quads.is_empty(),
        "content uses the viewport scissor"
    );
    assert!(
        content_quads.iter().any(|q| q.rect[1] < view[1] as f32),
        "scissor clips offscreen text"
    );
    let value = ui.value(bar).unwrap();
    assert!(
        value > 0.0 && value < 1.0,
        "scrollbar tracks content offset"
    );
    // `ContainerElement::Update`: a notch up queues 7 frames of a third of the view / 7.
    let offset_of = |ui: &Ui| match ui.nodes[root].kind {
        Kind::Control(Control::Container { offset, .. }) => offset[1],
        _ => unreachable!(),
    };
    ui.pointer_move((view[0] + view[2]) / 2, (view[1] + view[3]) / 2, root);
    assert!(ui.wheel(1));
    for _ in 0..10 {
        ui.update_containers();
    }
    let moved = 150 - offset_of(&ui);
    let third = ((view[3] - view[1]) as f32 * 0.33) as i32;
    assert!((moved - third).abs() <= 1, "{moved} vs {third}");
    // `ScrollElement::Update`: a press on the track below the thumb pages down.
    let before = ui.value(bar).unwrap();
    let r = ui.nodes[bar].rect;
    ui.pointer_move((r[0] + r[2]) / 2, r[3] - 1, root);
    ui.pointer_button(true, root);
    ui.pointer_button(false, root);
    ui.layout_dialog(root, [0, 0, 800, 600]);
    assert!(ui.value(bar).unwrap() > before);
}

#[test]
fn town_hall_table_positions_cells_and_graph_draws_finite_samples() {
    let Some(mut ui) = game_ui() else { return };
    let table = ui.instantiate(r"Dialog\TownHall.rsc:limits").unwrap();
    ui.layout(table, [0, 0, 1200, 700]);
    let cells = match &ui.nodes[table].kind {
        Kind::Control(Control::Table { cells, .. }) => cells.clone(),
        _ => panic!("limits table"),
    };
    assert_eq!(cells.len(), 66);
    for row in 0..11 {
        let mut row_cells: Vec<_> = cells.iter().filter(|c| c.1 == row).collect();
        row_cells.sort_by_key(|c| c.0);
        for pair in row_cells.windows(2) {
            assert!(
                ui.nodes[pair[0].2].rect[2] <= ui.nodes[pair[1].2].rect[0],
                "table columns overlap in row {row}"
            );
        }
    }
    let graph = ui.instantiate(r"Dialog\TownHall.rsc:graphData").unwrap();
    ui.layout(graph, [0, 0, 640, 480]);
    let empty_count = ui.draw(graph).len();
    ui.set_graph(
        graph,
        5.0,
        12,
        vec![
            (0xFFFF_FF00, vec![-2, 0, 3, 7, 1]),
            (0xFF00_FF00, vec![2, 1, 0, 1]),
        ],
    );
    let data = ui.draw(graph);
    assert!(
        data.len() > empty_count,
        "graph samples add visible line segments"
    );
    assert!(
        data.iter().all(|q| q.rect.iter().all(|v| v.is_finite())),
        "samples must never reach the renderer as non-finite rectangles"
    );
}

#[test]
fn clip_region_keeps_scrolled_text_inside_its_viewport() {
    let Some(mut ui) = game_ui() else { return };
    // This supported class has no standalone resource in the shipped packages.
    let content = ui.text(
        r"Font\FontSmall.rsc:resource",
        "A long line passing both edges of a narrow clip region",
    );
    let root = ui.add(Node::new(Kind::Control(Control::Clip {
        content: Some(content),
        size: [50, 20],
        offset: [30, 0],
    })));
    ui.layout(root, [0, 0, 100, 100]);
    let quads = ui.draw(root);
    assert!(!quads.is_empty());
    assert!(quads.iter().all(|q| q.clip == Some(ui.nodes[root].rect)));
    assert!(quads
        .iter()
        .any(|q| q.rect[0] < ui.nodes[root].rect[0] as f32));
    assert!(quads
        .iter()
        .any(|q| q.rect[2] > ui.nodes[root].rect[2] as f32));
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

#[test]
fn dialog_preview_layout_is_stable_on_first_frame() {
    let Some(mut ui) = game_ui() else {
        return;
    };
    for name in ["NewGame", "Options"] {
        let host = ui.group();
        let d = ui
            .instantiate_dialog(&format!(r"Dialog\{name}.rsc:resource"))
            .unwrap_or_else(|| panic!("dialog {name}"));
        ui.add_child(host, d.root);
        let mut rects = Vec::new();
        for _ in 0..3 {
            ui.layout(host, [400, 56, 1392, 822]);
            ui.layout_dialog(d.root, [412, 104, 1380, 810]);
            rects.push((ui.nodes[d.root].size, ui.nodes[d.root].rect));
        }
        assert_eq!(rects[0], rects[1], "{name}: {rects:?}");
        assert_eq!(rects[1], rects[2], "{name}: {rects:?}");
    }
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
fn text_shadow_and_wrapping_are_separate_and_ellipsis_is_not_a_shortcut() {
    let Some(mut ui) = game_ui() else { return };
    let root = ui.instantiate(r"Dialog\Mod.rsc:textModTitle").unwrap();
    assert_eq!(ui.nodes[root].shortcut, None);
    ui.set_text(
        root,
        "A long title that must fit within the original single line width limit",
    );
    ui.layout_dialog(root, [0, 0, 1000, 100]);
    let Kind::Text(t) = &ui.nodes[root].kind else {
        panic!("text")
    };
    assert_eq!(t.ellipsis_width, 154);
    assert!(t.layout.width <= 154);
    assert_eq!(t.layout.height, ui.res.fonts[t.font.unwrap()].font.size);
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
        vec![banished_ui::Event::Selected(root, 1)]
    );
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

/// The build panel's slots keep their height when blanked (unwrapped empty text is one line
/// high, `Font::ComputeStringLayout`), so the empty slot lines up with the others.
#[test]
fn blank_build_slots_line_up() {
    let Some(mut ui) = game_ui() else { return };
    ui.screen = [1280, 800];
    let root = ui.instantiate(r"Dialog\Build.rsc:build").unwrap();
    let table = ui.find(root, "storageTable").unwrap();
    let kids = ui.children(table);
    let last = *kids.last().unwrap();
    let l = ui.find(last, "labelStorage").unwrap();
    let t = ui.find(last, "textTotal").unwrap();
    ui.set_text(l, "");
    ui.set_text(t, "");
    if let Kind::Label { icon, .. } = &mut ui.nodes[l].kind {
        *icon = None;
    }
    ui.layout(root, [100, 100, 560, 260]);
    assert_eq!(ui.nodes[last].rect[1], ui.nodes[kids[1]].rect[1]);
    assert_eq!(ui.nodes[last].rect[3], ui.nodes[kids[1]].rect[3]);
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
