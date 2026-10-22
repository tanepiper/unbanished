//! Element classes: instancing, layout, clipping, scrolling, tables, text and queries.

use super::*;

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
    // `ScrollElement::ComputeLayout`: a vertical bar fills the height beside the view, not
    // just its own minimum size.
    let [_, _, mt, mb] = ui.nodes[bar].margins;
    assert_eq!(
        ui.nodes[bar].rect[3] - ui.nodes[bar].rect[1],
        view[3] - view[1] - mt - mb,
        "the scrollbar spans the view's height"
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
fn queries_find_the_main_menu_buttons_by_name_text_and_kind() {
    use unbanished_ui::Query;
    let Some(mut ui) = game_ui() else { return };
    let root = ui.instantiate(r"Dialog\MainMenu.rsc:main").unwrap();
    ui.layout_dialog(root, [0, 0, 1280, 720]);
    let q = |ui: &Ui, text: &str| -> Vec<String> {
        ui.query(&[root], &Query::parse(text).unwrap())
            .into_iter()
            .map(|id| ui.nodes[id].name.clone())
            .collect()
    };
    // The buttons in tree order; the Exit button is called buttonQuit in the resource.
    let buttons = q(&ui, "kind:button");
    assert_eq!(buttons.first().map(String::as_str), Some("buttonNew"));
    assert_eq!(buttons.last().map(String::as_str), Some("buttonQuit"));
    // A button's text is its label's: both match, the button first, so a click takes the button.
    assert_eq!(
        q(&ui, "text:exit"),
        ["buttonQuit", "labelQuit"],
        "text ignores letter case"
    );
    assert_eq!(
        q(&ui, "kind:button text:Exit"),
        ["labelQuit"],
        "later steps look inside"
    );
    assert_eq!(q(&ui, "ref:MainMenu buttonLoad"), ["buttonLoad"]);
    assert!(
        q(&ui, "ref:Options buttonLoad").is_empty(),
        "ref: limits the dialog"
    );
    let info = ui.info(ui.find(root, "buttonNew").unwrap());
    assert_eq!(
        (info.kind, info.text.as_str(), info.enabled),
        ("button", "New", true)
    );
    // A hidden element and everything inside it are skipped.
    let load = ui.find(root, "buttonLoad").unwrap();
    ui.nodes[load].flags |= format::flags::HIDDEN;
    assert!(q(&ui, "buttonLoad").is_empty());
    assert!(q(&ui, "text:Load").is_empty());
}
