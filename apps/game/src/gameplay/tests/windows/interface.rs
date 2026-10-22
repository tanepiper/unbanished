//! Moving windows, help, paused commands, the status bar and the event ribbon.

use super::*;

/// HelpUI: the topics by title, the chosen page's text, back and forward through the
/// pages seen.
#[test]
fn windows_drag_by_their_title() {
    let Some((mut game, mut gameplay)) = generated() else {
        return;
    };
    game.with_ctx(|_, ctx| {
        let template = r"Template\UtilityHelp.rsc:resource";
        gameplay.utility.toggle(
            ctx.ui,
            gameplay.screen.as_mut().unwrap(),
            ctx.archive,
            template,
            None,
            0,
        );
        gameplay
            .screen
            .as_mut()
            .unwrap()
            .layout(ctx.ui, (1280, 800));
        let screen_root = gameplay.screen.as_ref().unwrap().root;
        let title = ctx.ui.find(screen_root, "borderTitle").expect("no title");
        let [tx, ty, ..] = ctx.ui.node(title).rect;
        let (_, root) = gameplay
            .utility
            .title_at(ctx.ui, tx + 4, ty + 4)
            .expect("no title to drag");
        let [x0, y0, ..] = ctx.ui.node(root).rect;
        let b = ctx.ui.find(root, "borderTitle").unwrap();
        let [bx, by, ..] = ctx.ui.node(b).rect;
        let press = (bx + 4, by + 4);
        gameplay.raw(ctx, RawInput::Cursor(press.0, press.1));
        gameplay.raw(ctx, RawInput::UiPress);
        gameplay.raw(ctx, RawInput::Cursor(press.0 + 100, press.1 + 50));
        gameplay.raw(ctx, RawInput::Release);
        gameplay.raw(ctx, RawInput::Cursor(press.0 + 300, press.1 + 300));
        let m = ctx.ui.node(root).margins;
        assert_eq!((m[0], m[2]), (x0 + 100, y0 + 50));
        // Reopened, it comes back where it was left.
        for _ in 0..2 {
            gameplay.utility.toggle(
                ctx.ui,
                gameplay.screen.as_mut().unwrap(),
                ctx.archive,
                template,
                None,
                0,
            );
        }
        gameplay
            .screen
            .as_mut()
            .unwrap()
            .layout(ctx.ui, (1280, 800));
        assert!(gameplay
            .utility
            .title_at(ctx.ui, press.0 + 100, press.1 + 50)
            .is_some());
        // An entity window pins itself once dragged more than 5 pixels.
        let town = gameplay.sim.town.as_ref().unwrap();
        let barn = town.storage_sites[0].entity;
        gameplay.entities.select(
            ctx.ui,
            gameplay.screen.as_mut().unwrap(),
            ctx.archive,
            town,
            Entity::Placed(barn),
        );
        gameplay
            .screen
            .as_mut()
            .unwrap()
            .layout(ctx.ui, (1280, 800));
        let root = gameplay.entities.open[0].root;
        let b = ctx.ui.find(root, "borderTitle").expect("no entity title");
        let [bx, by, ..] = ctx.ui.node(b).rect;
        gameplay.raw(ctx, RawInput::Cursor(bx + 4, by + 4));
        gameplay.raw(ctx, RawInput::UiPress);
        gameplay.raw(ctx, RawInput::Cursor(bx + 7, by + 6));
        assert!(!gameplay.entities.open[0].pinned);
        gameplay.raw(ctx, RawInput::Cursor(bx + 24, by + 4));
        gameplay.raw(ctx, RawInput::Release);
        assert!(gameplay.entities.open[0].pinned);
        assert!(gameplay.entities.open[0].at.is_some());
        let pin = ctx.ui.find(root, "checkPin").unwrap();
        assert!(ctx.ui.checked(pin));
    });
}

#[test]
fn help_window_browses_the_pages() {
    let Some((mut game, mut gameplay)) = generated() else {
        return;
    };
    game.with_ctx(|_, ctx| {
        gameplay.utility.toggle(
            ctx.ui,
            gameplay.screen.as_mut().unwrap(),
            ctx.archive,
            r"Template\UtilityHelp.rsc:resource",
            None,
            0,
        );
        let root = gameplay.screen.as_ref().unwrap().root;
        let combo = ctx.ui.find(root, "comboTopic").expect("no help window");
        let doc = ctx.ui.find(root, "documentHelp").unwrap();
        let list = match &ctx.ui.node(combo).kind {
            unbanished_ui::Kind::Control(unbanished_ui::Control::Combo { list, .. }) => *list,
            _ => panic!("comboTopic is not a combo"),
        };
        let items = match &ctx.ui.node(list).kind {
            unbanished_ui::Kind::List(l) => l.items.clone(),
            _ => Vec::new(),
        };
        assert_eq!(items.len(), 66);
        assert_eq!(items[0], "Contents");
        let text = |ui: &unbanished_ui::Ui| match &ui.node(doc).kind {
            unbanished_ui::Kind::Control(unbanished_ui::Control::Document(_)) => {
                format!("{:?}", ui.node(doc).kind)
            }
            _ => String::new(),
        };
        assert!(text(ctx.ui).contains("Welcome to the world of Banished"));
        let quarry = items.iter().position(|t| t == "Quarries").unwrap();
        gameplay.event(ctx, Event::Selected(combo, quarry));
        assert!(text(ctx.ui).contains("cut stone"));
        gameplay.update_hud(ctx.ui);
        let back = ctx.ui.find(root, "buttonBack").unwrap();
        let forward = ctx.ui.find(root, "buttonForward").unwrap();
        assert!(ctx.ui.node(back).enabled && !ctx.ui.node(forward).enabled);
        gameplay.event(ctx, Event::Clicked(back));
        assert!(text(ctx.ui).contains("Welcome to the world of Banished"));
        gameplay.update_hud(ctx.ui);
        assert!(!ctx.ui.node(back).enabled && ctx.ui.node(forward).enabled);
        gameplay.event(ctx, Event::Clicked(forward));
        assert!(text(ctx.ui).contains("cut stone"));
    });
}

/// Issue #3: WorkPlaceUI's buttons act inside the event (`FUN_14008ccb0`), so a change
/// made while paused shows in the window's next refresh without an iteration.
#[test]
fn window_commands_apply_while_paused() {
    let Some((mut game, mut gameplay)) = generated() else {
        return;
    };
    game.with_ctx(|_, ctx| {
        let town = gameplay.sim.town.as_mut().unwrap();
        let view = gameplay.view.as_ref().unwrap();
        let grid = unbanished_game::terrain::HeightGrid {
            width: view.terrain.size + 1,
            heights: &view.terrain.heights,
        };
        let mut tool = PlacementTool::start(
            ctx.archive,
            town,
            "Template/ForesterLodge.rsc",
            &mut gameplay.sim.rng,
        )
        .unwrap();
        let near = town.start;
        tool.find_clear_spot(town, &grid, near).unwrap();
        let id = tool.place(ctx.archive, town, 0).unwrap();
        gameplay.update(ctx, 1.0 / 30.0);
        gameplay.paused = true;
        let (town, screen) = (
            gameplay.sim.town.as_ref().unwrap(),
            gameplay.screen.as_mut().unwrap(),
        );
        gameplay
            .entities
            .select(ctx.ui, screen, ctx.archive, town, Entity::Placed(id));
        gameplay.update(ctx, 1.0 / 30.0);
        let root = gameplay.entities.open[0].root;
        let count = ctx.ui.find(root, "workPlaceCount").expect("workPlaceCount");
        let shown = crate::ui::utility::text_of(ctx.ui, count);
        let up = ctx.ui.find(root, "jobCountUp").unwrap();
        let down = ctx.ui.find(root, "jobCountDown").unwrap();
        let button = if ctx.ui.node(up).enabled { up } else { down };
        assert!(ctx.ui.node(button).enabled, "neither job button is enabled");
        let time = gameplay.sim.calendar.elapsed;
        gameplay.event(ctx, Event::Clicked(button));
        let town = gameplay.sim.town.as_ref().unwrap();
        assert!(town.construction.commands.is_empty());
        gameplay.update(ctx, 1.0 / 30.0);
        assert_eq!(
            gameplay.sim.calendar.elapsed, time,
            "the game ran while paused"
        );
        let now = crate::ui::utility::text_of(ctx.ui, count);
        assert_ne!(now, shown, "the window still shows the old most workers");
    });
}

#[test]
fn status_bar_layout_open_close_and_draw_leave_simulation_unchanged() {
    let Some((mut game, mut gameplay)) = generated() else {
        return;
    };
    let town = gameplay.sim.town.as_mut().unwrap();
    let rules = town.agents.rules();
    for s in &mut town.agents.states {
        s.health = rules.health_max * 0.5;
        s.happiness = rules.happiness * 0.5;
        s.status = 1 | 16;
    }
    let people = town.citizens.len() as u32;
    let state = format!("{:?}", town.agents.states);
    let rng = gameplay.sim.rng.clone();
    let time = gameplay.sim.calendar.elapsed;
    let values = gameplay.status_values();
    assert_eq!(values.status, [people, 0, 0, 0, people, 0]);
    assert_eq!(values.stock[4], 40);
    assert_eq!(values.stock[5], 1800);
    assert_eq!(values.health, rules.health_max * 0.5);
    game.with_ctx(|_, ctx| {
        for _ in 0..4 {
            gameplay.utility.toggle(
                ctx.ui,
                gameplay.screen.as_mut().unwrap(),
                ctx.archive,
                "Template/StatusBar.rsc",
                None,
                0,
            );
            gameplay.update_hud(ctx.ui);
            let screen = gameplay.screen.as_ref().unwrap();
            screen.layout(ctx.ui, (1280, 800));
            ctx.ui.pointer_move(80, 80, screen.root);
            let _ = ctx.ui.draw(screen.root);
            if let Some(group) = ctx.ui.find(screen.root, "groupHearts") {
                let icons = ctx.ui.node(group).children.clone();
                assert_eq!(icons.len(), 5);
                let half = ctx.ui.find(icons[2], "half").unwrap();
                let whole = ctx.ui.find(icons[2], "whole").unwrap();
                assert_eq!(ctx.ui.node(half).flags & 1, 0);
                assert_eq!(ctx.ui.node(whole).flags & 1, 1);
            }
        }
    });
    assert_eq!(rng, gameplay.sim.rng);
    assert_eq!(time, gameplay.sim.calendar.elapsed);
    assert_eq!(
        state,
        format!("{:?}", gameplay.sim.town.as_ref().unwrap().agents.states)
    );
}

#[test]
fn event_ribbon_popup_and_goto() {
    let Some((mut game, mut gameplay)) = generated() else {
        return;
    };
    let rng = gameplay.sim.rng.clone();
    game.with_ctx(|_, ctx| {
        let frame = |g: &mut GameplayLoop, ctx: &mut crate::loops::Ctx| {
            let screen = g.screen.as_mut().unwrap();
            screen.layout(ctx.ui, (1280, 800));
            let width = g.toolbar.as_ref().unwrap().width(ctx.ui);
            g.notify
                .as_mut()
                .unwrap()
                .update(ctx.ui, screen, &g.sim.events, 0.1, width)
        };
        frame(&mut gameplay, ctx);
        // Low food (no cell), a fire at a cell, and a type-26 event (popup, icon 12).
        gameplay
            .sim
            .events
            .add(22, 3, -1, 0, 1, Default::default(), false);
        let n = gameplay.sim.town.as_ref().unwrap().cells.size as i32;
        gameplay
            .sim
            .events
            .add(31, 3, 70 * n + 30, 0, 1, Default::default(), false);
        gameplay
            .sim
            .events
            .add(0x1a, 1, -1, 0, 1, Default::default(), false);
        assert!(frame(&mut gameplay, ctx));
        for _ in 0..40 {
            frame(&mut gameplay, ctx);
        }
        let screen = gameplay.screen.as_ref().unwrap();
        let root = screen
            .dialogs
            .iter()
            .find(|d| ctx.ui.find(d.root, "ribbonNotify").is_some())
            .unwrap()
            .root;
        let content = ctx.ui.find(root, "notifyContent").unwrap();
        let groups = ctx.ui.node(content).children.clone();
        assert_eq!(groups.len(), 6);
        let visible = |ui: &unbanished_ui::Ui, n| ui.node(n).flags & 1 == 0;
        let button: Vec<_> = groups
            .iter()
            .map(|&g| ctx.ui.find(g, "notifyButton").unwrap())
            .collect();
        let image: Vec<_> = groups
            .iter()
            .map(|&g| ctx.ui.find(g, "notifyImage").unwrap())
            .collect();
        assert!(visible(ctx.ui, image[0]) && !visible(ctx.ui, button[0]));
        assert!(visible(ctx.ui, button[1]) && !visible(ctx.ui, image[1]));
        assert_eq!(
            ctx.ui.node(groups[0]).tooltip,
            "The reserve of food is low!"
        );
        eprintln!(
            "ribbon root {:?} margins {:?}",
            ctx.ui.node(root).rect,
            ctx.ui.node(root).margins
        );
        // The popup and its OK button.
        let popup = screen
            .dialogs
            .iter()
            .find(|d| ctx.ui.find(d.root, "imageIcon").is_some())
            .map(|d| d.root)
            .unwrap();
        let ok = ctx.ui.find(popup, "buttonOk").unwrap();
        let dialogs = screen.dialogs.len();
        gameplay.event(ctx, Event::Clicked(ok));
        assert_eq!(gameplay.screen.as_ref().unwrap().dialogs.len(), dialogs - 1);
        // The fire's button moves the camera to its cell.
        gameplay.event(ctx, Event::Clicked(button[1]));
        let half = (n >> 1) as f32 - 0.5;
        let Some(SceneView {
            camera: SceneCamera::Map(c),
            ..
        }) = &gameplay.view
        else {
            panic!("no map camera");
        };
        assert_eq!(c.target, glam::Vec2::new(30.0 - half, 70.0 - half));
    });
    assert_eq!(rng, gameplay.sim.rng);
}
