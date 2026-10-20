use super::*;
use crate::draw::argb_to_rgba;
use banished_formats::pkg::Package;
#[test]
fn parses_native_caret_commands_and_unicode_without_losing_words() {
    assert_eq!(
        tokenize("^f1Title^p^c2^abhelp.topic Link · text^ae ^i0Icon ^ml20 ^jl next"),
        vec![
            Token::Command('f', Some('1'), String::new()),
            Token::Word("Title".into()),
            Token::Command('p', None, String::new()),
            Token::Command('c', Some('2'), String::new()),
            Token::Command('a', Some('b'), "help.topic".into()),
            Token::Word("Link".into()),
            Token::Word("·".into()),
            Token::Word("text".into()),
            Token::Command('a', Some('e'), String::new()),
            Token::Command('i', Some('0'), "Icon".into()),
            Token::Command('m', Some('l'), "20".into()),
            Token::Command('j', Some('l'), String::new()),
            Token::Word("next".into())
        ]
    );
    assert!(tokenize("^f").len() == 1);
}

#[test]
fn renders_shipped_help_documents_and_decodes_captures_when_available() {
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../game-data/original/WinData");
    if !dir.join("data0.pkg").exists() {
        eprintln!("game data unavailable; skipping rich document integration");
        return;
    }
    let mut archive = Archive::default();
    for name in ["data1.pkg", "patch1.pkg", "patch0.pkg", "data0.pkg"] {
        archive.add(Package::parse(std::fs::read(dir.join(name)).unwrap()).unwrap());
    }
    let archive = Rc::new(archive);
    let mut specs = Vec::new();
    let mut capture_count = 0;
    let mut rotated_parts = 0;
    let mut rich_texts = Vec::new();
    for name in archive.names().into_iter().filter(|n| n.ends_with(".crs")) {
        let bytes = archive.read(name).unwrap();
        match crs::header(&bytes).unwrap().type_id {
            id if id == type_id("DocumentTextDescription") => {
                let element = ui::element(&bytes).unwrap();
                if let ui::ElementKind::DocumentText(spec) = element.kind {
                    specs.push((name.to_string(), spec));
                }
            }
            id if id == type_id("SpriteCapture") => {
                let capture = read_capture(&bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
                rotated_parts += capture.rotated_parts;
                capture_count += 1;
            }
            id if id == type_id("StringTable") => {
                rich_texts.extend(
                    ui::string_table(&bytes)
                        .unwrap()
                        .into_iter()
                        .filter(|(_, text)| text.contains('^')),
                );
            }
            _ => {}
        }
    }
    assert!(!specs.is_empty());
    let mut ui = Ui::new(archive.clone());
    let mut rendered = 0;
    let mut linked = 0;
    let mut embedded = 0;
    for (name, spec) in specs {
        let mut doc = ui.load_document(&spec);
        assert!(
            doc.fonts.iter().all(Option::is_some),
            "{name}: missing document font"
        );
        assert!(
            doc.sheets.iter().all(Option::is_some),
            "{name}: missing document sprite sheet"
        );
        assert!(
            doc.captures
                .values()
                .flat_map(|c| &c.parts)
                .all(|part| part.sheet < 0 || part.sprite.is_some()),
            "{name}: unresolved capture sprite"
        );
        eprintln!(
            "{name}: {} fonts, {} sheets, {} captures; width {}",
            doc.fonts.len(),
            doc.sheets.len(),
            doc.captures.len(),
            doc.width
        );
        if let Some(reference) = &spec.images {
            let refs = read_capture_list(&archive.read_ref(reference).unwrap()).unwrap();
            assert_eq!(
                doc.captures.len(),
                refs.iter().flatten().count(),
                "{name}: failed to load capture collection"
            );
        }
        let mut texts = spec
            .string_table
            .as_deref()
            .map(|table| ui::string_table(&archive.read_ref(table).unwrap()).unwrap())
            .unwrap_or_else(|| vec![(String::new(), spec.text.clone())]);
        if name.contains("Help_documentHelp") {
            texts.extend(rich_texts.clone());
        }
        for (key, text) in texts {
            if text.is_empty() {
                continue;
            }
            doc.text = text;
            ui.layout_document(&mut doc);
            let mut quads = Vec::new();
            ui.draw_document(&doc, [0, 0], 0xFFFF_FFFF, None, &mut quads);
            assert!(!quads.is_empty(), "{name} {key}: no document quads");
            assert!(doc.size[1] > 0, "{name} {key}: zero document height");
            assert!(
                doc.links.iter().all(|(_, r)| r[0] < r[2] && r[1] < r[3]),
                "{name} {key}: invalid link rectangle"
            );
            linked += doc.links.len();
            embedded += doc
                .items
                .iter()
                .filter(|i| matches!(i.content, Content::Sprite(_) | Content::Capture(_)))
                .count();
            rendered += 1;
        }
        if let Some(capture) = doc.captures.keys().next().cloned() {
            doc.text = format!("^d{capture}");
            ui.layout_document(&mut doc);
            let mut quads = Vec::new();
            ui.draw_document(&doc, [0, 0], 0xFFFF_FFFF, None, &mut quads);
            assert!(!quads.is_empty(), "{name}: capture generates no sprites");
            embedded += 1;
        }
        if let Some((slot, sprite_name)) = doc.sheets.iter().enumerate().find_map(|(slot, s)| {
            s.and_then(|s| {
                ui.res.sheets[s]
                    .sheet
                    .sprites
                    .first()
                    .map(|s| (slot, s.name.clone()))
            })
        }) {
            doc.text = format!("^i{slot}{sprite_name}");
            ui.layout_document(&mut doc);
            let mut quads = Vec::new();
            ui.draw_document(&doc, [0, 0], 0xFFFF_FFFF, None, &mut quads);
            assert_eq!(quads.len(), 1, "{name}: inline sprite generates no quad");
            embedded += 1;
        }
        if name.contains("Help_documentHelp") {
            doc.width = 40;
            doc.text = "^abHelpTopic One Two Three Four^ae".into();
            ui.layout_document(&mut doc);
            assert!(doc.links.len() >= 2);
            assert!(doc.links.iter().all(|(target, _)| target == "HelpTopic"));
            assert!(
                doc.links.iter().any(|(_, rect)| rect[1] > 0),
                "link did not wrap"
            );
            let mut normal = Vec::new();
            ui.draw_document(&doc, [0, 0], 0xFFFF_FFFF, None, &mut normal);
            doc.hovered_link = Some(0);
            let mut hovered = Vec::new();
            ui.draw_document(&doc, [0, 0], 0xFFFF_FFFF, None, &mut hovered);
            assert!(
                hovered.len() > normal.len(),
                "hover did not add the configured underline"
            );
            assert!(hovered
                .iter()
                .any(|q| q.color == argb_to_rgba(doc.link_colors[1], 255)));
            doc.pressed_link = Some(0);
            let mut pressed = Vec::new();
            ui.draw_document(&doc, [0, 0], 0xFFFF_FFFF, None, &mut pressed);
            assert!(pressed
                .iter()
                .any(|q| q.color == argb_to_rgba(doc.link_colors[2], 255)));
        }
    }
    eprintln!("rendered {rendered} shipped document strings, {linked} link rectangles, {embedded} embedded images; decoded {capture_count} captures");
    assert!(rendered > 0);
    assert!(linked > 0);
    assert_eq!(
        rotated_parts, 0,
        "shipped captures require rotation support"
    );
}
