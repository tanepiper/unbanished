use super::*;

fn game() -> Option<crate::Game> {
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../game-data/original/WinData");
    if !dir.join("data0.pkg").exists() {
        eprintln!("game data missing; skipping options integration");
        return None;
    }
    // Never touch the player's stored settings.
    let file = std::env::temp_dir().join(format!("banished-options-{}.txt", std::process::id()));
    std::env::set_var("UNBANISHED_SETTINGS", &file);
    let packages = ["data1.pkg", "patch1.pkg", "patch0.pkg", "data0.pkg"]
        .into_iter()
        .filter_map(|n| std::fs::read(dir.join(n)).ok().map(|d| (n.to_owned(), d)))
        .collect();
    crate::Game::with_settings(packages, crate::settings::Settings::default()).ok()
}

#[test]
fn edits_apply_cancel_and_rebind_like_the_original() {
    let Some(mut game) = game() else { return };
    game.with_ctx(|_, ctx| {
        let mut screen = Screen::new(ctx.ui);
        let mut o = OptionsDialog::open(ctx, &mut screen).unwrap();
        assert_eq!(o.el.keys.len(), keymap::id::COUNT - FIRST_KEY);
        let name = o.name_text(ctx.ui, 3).unwrap();
        assert!(matches!(&ctx.ui.node(name).kind, Kind::Text(t) if t.text == "Menu/Pause"));
        // Event 0x17: the music volume follows its slider at once; Cancel reverts it.
        let music = o.el.volumes[1].unwrap();
        ctx.ui.set_value(music, 20.0);
        o.event(ctx, &mut screen, Event::Changed(music));
        assert_eq!(ctx.settings.audio.music, 20);
        let cancel = o.el.cancel.unwrap();
        assert_eq!(o.event(ctx, &mut screen, Event::Clicked(cancel)), Outcome::Closed);
        assert_eq!(ctx.settings.audio.music, AudioConfig::default().music);

        // Apply stores the game tab.
        let mut o = OptionsDialog::open(ctx, &mut screen).unwrap();
        ctx.ui.set_value(o.el.ui_scale.unwrap(), 140.0);
        ctx.ui.set_checked(o.el.celsius.unwrap(), true);
        o.event(ctx, &mut screen, Event::Clicked(o.el.apply.unwrap()));
        assert_eq!(ctx.settings.game.ui_scale, 140);
        assert!(ctx.settings.game.show_celsius);
        assert!(ctx.settings.persistence.serialize().contains("GameUIScale=140"));

        // Rebind Forward (row 6) to S: S is Back's, so `inputBound` asks; Yes moves it.
        let forward = keymap::id::FORWARD - FIRST_KEY;
        let s = (keymap::KEYBOARD, 41);
        o.event(ctx, &mut screen, Event::Clicked(o.el.keys[forward][0]));
        // The release of the opening click does not bind.
        o.released(ctx, &mut screen, (keymap::MOUSE, 4));
        assert!(matches!(o.rebind, Some(Rebind::Waiting { .. })));
        o.update();
        o.released(ctx, &mut screen, s);
        let Some(Rebind::Confirm { root, .. }) = o.rebind else {
            panic!("expected the inputBound box");
        };
        let text = ctx.ui.find(root, "inputBoundText").unwrap();
        assert!(matches!(&ctx.ui.node(text).kind,
            Kind::Text(t) if t.text == "S is already bound to Move View Back. Bind it to Move View Forward instead?"));
        let yes = ctx.ui.find(root, "buttonYes").unwrap();
        o.event(ctx, &mut screen, Event::Clicked(yes));
        assert_eq!(ctx.settings.keymap.binding(keymap::id::FORWARD, 0), Some(s));
        assert_eq!(ctx.settings.keymap.binding(keymap::id::BACK, 0), None);
        // Unbind (result 2) clears the slot.
        o.event(ctx, &mut screen, Event::Clicked(o.el.keys[forward][0]));
        let Some(Rebind::Waiting { root, .. }) = o.rebind else {
            panic!("expected the inputBind box");
        };
        let unbind = ctx.ui.find(root, "buttonYes").unwrap();
        o.event(ctx, &mut screen, Event::Clicked(unbind));
        assert_eq!(ctx.settings.keymap.binding(keymap::id::FORWARD, 0), None);
        // Ok writes the key map.
        assert_eq!(o.event(ctx, &mut screen, Event::Clicked(o.el.ok.unwrap())), Outcome::Closed);
        assert!(ctx.settings.persistence.serialize().contains("InputForward=-1"));
    });
    if let Ok(file) = unbanished_formats::env::var("SETTINGS") {
        let _ = std::fs::remove_file(file);
    }
}
