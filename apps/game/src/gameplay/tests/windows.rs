//! The game's windows: the Town Hall, trade, crops, limits, help, the status bar, the event ribbon.

use super::*;

/// TownHallUI over the started town: the overview's counts, a row per stock slot and
/// a profession row each, and the graphs drawn from the statistics.
#[test]
fn town_hall_window_reads_the_town() {
    let Some((mut game, mut gameplay)) = generated() else {
        return;
    };
    game.with_ctx(|_, ctx| {
        for _ in 0..3 {
            gameplay.iterate(ctx.archive, 1.0 / 30.0);
        }
        let root = ctx.ui.instantiate("Dialog\\TownHall.rsc:tabs").unwrap();
        let mut th = crate::ui::town_hall_ui::TownHallUi::bind(ctx.ui, ctx.archive, root);
        let town = gameplay.sim.town.as_ref().unwrap();
        let x = crate::ui::town_hall_ui::TownContext {
            stock_counts: &gameplay.sim.stock_warnings.count,
            statistics: &gameplay.sim.statistics,
        };
        th.refresh(ctx.ui, ctx.archive, town, &x);
        let text = |name: &str| {
            let n = ctx.ui.find(root, name).unwrap();
            match &ctx.ui.node(n).kind {
                unbanished_ui::Kind::Control(unbanished_ui::Control::Edit(e)) => {
                    e.text.text.clone()
                }
                unbanished_ui::Kind::Text(t) => t.text.clone(),
                _ => String::new(),
            }
        };
        let alive = town.citizens.iter().filter(|c| c.alive).count();
        assert_eq!(text("numberCitizens"), alive.to_string());
        assert!(
            !gameplay.sim.statistics.series[0].is_empty(),
            "no sample yet"
        );
        assert_eq!(gameplay.sim.statistics.series[0][0], alive as i32);
    });
}

/// TradeUI on a trading post with a merchant docked: the post's kept logs sell at the
/// merchant's value, its goods buy; the trade button sends the exchange, which moves the
/// goods, counts the value traded away and brings the livestock to the post.
#[test]
fn trade_window_exchanges_with_the_merchant() {
    let Some((mut game, mut gameplay)) = generated() else {
        return;
    };
    let view = gameplay.view.as_ref().unwrap();
    let heights = view.terrain.heights.clone();
    let terrain = unbanished_game::terrain::HeightGrid {
        width: view.terrain.size + 1,
        heights: &heights,
    };
    let id = game.with_ctx(|_, ctx| {
        let town = gameplay.sim.town.as_mut().unwrap();
        let mut rng = unbanished_game::random::Random::new(7);
        let mut tool = crate::tools::placement_tool::PlacementTool::start(
            ctx.archive,
            town,
            r"Template\TradingPost.rsc:resource",
            &mut rng,
        )
        .unwrap();
        let n = town.cells.size as i32;
        let half = (n >> 1) as f32;
        let river = gameplay.river_paths.first().cloned().unwrap_or_default();
        let mut spot = None;
        'search: for p in &river {
            let (x, y) = ((p[0] + half) as i32, (p[1] + half) as i32);
            for dy in -6..=6 {
                for dx in -6..=6 {
                    for r in 0..4 {
                        tool.hover(town, &terrain, Some((x + dx, y + dy)), r);
                        if tool.valid() {
                            spot = Some(((x + dx, y + dy), r));
                            break 'search;
                        }
                    }
                }
            }
        }
        let (at, r) = spot.expect("no trading post spot");
        tool.hover(town, &terrain, Some(at), r);
        tool.place(ctx.archive, town, r).unwrap()
    });
    let town = gameplay.sim.town.as_mut().unwrap();
    // The next iteration activates the post (its TradeComponent).
    let mut rng = unbanished_game::random::Random::new(7);
    town.apply_commands(&terrain, &mut rng);
    town.iterate(&terrain, &mut rng, 1.0 / 30.0, 50.0);
    let material = |town: &crate::start::StartingTown, key: &str| {
        (0..town.materials.len())
            .find(|&m| town.materials[m].reference.contains(key))
            .unwrap()
    };
    let logs = material(town, "RawMaterialWood");
    let goods = &town.construction.trade_goods;
    let food = goods
        .iter()
        .position(|g| g.material.is_some() && g.material != Some(logs))
        .unwrap();
    let livestock = goods
        .iter()
        .position(|g| town.wildlife.livestock_kinds.contains_key(&g.template))
        .expect("no livestock good");
    // A merchant who takes logs.
    let st = town.construction.sites[&id].template;
    let flags = town.materials[logs].flags;
    let merchant = town.construction.templates[st]
        .trade
        .as_ref()
        .unwrap()
        .merchants
        .iter()
        .position(|m| (m.flags.0 | m.flags.1) & flags != 0)
        .expect("no merchant takes logs") as i32;
    let site = town.construction.sites.get_mut(&id).unwrap();
    site.machine.state = 0x13;
    let storage = site.storage;
    let tr = site.trade.as_mut().unwrap();
    tr.merchant = merchant;
    tr.timer = i64::MAX / 2;
    tr.keep.insert(logs, 30);
    tr.offers = vec![
        unbanished_game::commerce::trade::Offer {
            good: food,
            value: 2,
            amount: 10,
        },
        unbanished_game::commerce::trade::Offer {
            good: livestock,
            value: 1,
            amount: 2,
        },
    ];
    town.storages[storage].add(logs, 30);
    game.with_ctx(|_, ctx| {
        let town = gameplay.sim.town.as_ref().unwrap();
        gameplay.entities.select(
            ctx.ui,
            gameplay.screen.as_mut().unwrap(),
            ctx.archive,
            town,
            Entity::Placed(id),
        );
        let root = gameplay.entities.open[0].root;
        assert!(gameplay.entities.trade_windows().contains(&id));
        // The sell row of the kept logs and the merchant's two goods.
        let spinners: Vec<_> = (0..ctx.ui.nodes.len())
            .filter(|&n| ctx.ui.descendant(root, n) && ctx.ui.node(n).name == "spinnerItemCount")
            .collect();
        assert_eq!(spinners.len(), 3, "one sell row and two buy rows");
        let value = unbanished_game::commerce::trade::sell_value(
            &town.construction.trade_goods,
            &town.materials,
            town.construction.sites[&id].trade.as_ref().unwrap(),
            town.construction.templates[town.construction.sites[&id].template]
                .trade
                .as_ref(),
            logs,
        );
        // Sell enough logs to pay for 4 food and both animals.
        let sell = (4 * 2 + 2 + value - 1) / value;
        crate::ui::building_ui::set_spinner(ctx.ui, spinners[0], sell);
        crate::ui::building_ui::set_spinner(ctx.ui, spinners[1], 4);
        crate::ui::building_ui::set_spinner(ctx.ui, spinners[2], 2);
        gameplay.entities.refresh(ctx.ui, ctx.archive, town);
        let trade = ctx.ui.find(root, "buttonTrade").unwrap();
        assert!(value > 0);
        assert!(ctx.ui.node(trade).enabled, "the trade is not allowed");
        let Some(WorkPlaceClick::Command(c)) = gameplay.entities.click(ctx.ui, town, trade) else {
            panic!("the trade button sends nothing");
        };
        assert_eq!(
            c,
            unbanished_game::work::professions::Command::Exchange {
                entity: id,
                sell: vec![(logs, sell)],
                buy: vec![(food, 4), (livestock, 2)],
            }
        );
        let town = gameplay.sim.town.as_mut().unwrap();
        let unassigned = town.wildlife.unassigned.len();
        town.construction.commands.push(c);
        let mut rng = unbanished_game::random::Random::new(7);
        town.apply_commands(&terrain, &mut rng);
        let s = &town.storages[storage];
        let food_m = town.construction.trade_goods[food].material.unwrap();
        assert_eq!(s.available(logs), 30 - sell);
        assert_eq!(s.available(food_m), 4);
        assert!(town.construction.known_materials.contains(&food_m));
        let tr = town.construction.sites[&id].trade.as_ref().unwrap();
        assert_eq!(tr.offers.len(), 1, "the livestock offer is used up");
        assert_eq!(tr.offers[0].amount, 6);
        assert_eq!(town.construction.trade_value, sell * value);
        let t = town.construction.trade_goods[livestock].template;
        assert!(town.construction.known.contains(&t));
        assert_eq!(town.wildlife.unassigned.len(), unassigned + 2);
        let a = *town.wildlife.unassigned.last().unwrap();
        assert_eq!(
            town.wildlife.animals[&a].flags & unbanished_game::nature::wildlife::WILD,
            0
        );
        // The window's amounts go back to none.
        let town = gameplay.sim.town.as_ref().unwrap();
        gameplay.entities.refresh(ctx.ui, ctx.archive, town);
        let left: Vec<_> = (0..ctx.ui.nodes.len())
            .filter(|&n| ctx.ui.descendant(root, n) && ctx.ui.node(n).name == "spinnerItemCount")
            .map(|n| crate::ui::building_ui::spinner_value(ctx.ui, n))
            .collect();
        assert_eq!(left, vec![Some(0), Some(0)]);
    });
}

/// CropUI on a placed field: its `selectResource` combo lists the known crops; choosing
/// one sends the field's crop.
#[test]
fn crop_window_lists_the_known_crops() {
    let Some((mut game, mut gameplay)) = generated() else {
        return;
    };
    let view = gameplay.view.as_ref().unwrap();
    let heights = view.terrain.heights.clone();
    let terrain = unbanished_game::terrain::HeightGrid {
        width: view.terrain.size + 1,
        heights: &heights,
    };
    let id = game.with_ctx(|_, ctx| {
        let town = gameplay.sim.town.as_mut().unwrap();
        let n = town.cells.size as i32;
        let empty = |town: &crate::start::StartingTown, x: i32, y: i32| {
            (y..y + 6).all(|cy| {
                (x..x + 6).all(|cx| {
                    let i = (cy * n + cx) as usize;
                    cx < n
                        && cy < n
                        && town.cells.types[i] == unbanished_game::terrain::CellType::GROUND
                        && town.cells.objects[i].is_none()
                        && town.cells.occupants[i].is_none()
                        && town.cells.flags[i] == 0
                })
            })
        };
        let (sx, sy) = town.start;
        let rect = (8..60)
            .flat_map(|r| (-r..=r).flat_map(move |d| [(d, -r), (d, r), (-r, d), (r, d)]))
            .map(|(dx, dy)| (sx + dx, sy + dy))
            .find(|&(x, y)| x > 0 && y > 0 && empty(town, x, y))
            .expect("no empty ground");
        town.place_area(
            ctx.archive,
            r"Template\CropField.rsc:resource",
            (rect.0, rect.1, 6, 6),
            &terrain,
        )
        .expect("the field was not placed")
    });
    let town = gameplay.sim.town.as_mut().unwrap();
    let mut rng = unbanished_game::random::Random::new(7);
    town.apply_commands(&terrain, &mut rng);
    town.iterate(&terrain, &mut rng, 1.0 / 30.0, 50.0);
    let crops: Vec<usize> = town
        .construction
        .known
        .iter()
        .copied()
        .filter(|&t| {
            town.templates[t].growth.is_some() && !town.templates[t].reference.contains("Tree")
        })
        .collect();
    assert!(!crops.is_empty());
    game.with_ctx(|_, ctx| {
        let town = gameplay.sim.town.as_ref().unwrap();
        gameplay.entities.select(
            ctx.ui,
            gameplay.screen.as_mut().unwrap(),
            ctx.archive,
            town,
            Entity::Placed(id),
        );
        let root = gameplay.entities.open[0].root;
        let combo = ctx.ui.find(root, "selectResource").unwrap();
        let list = match &ctx.ui.node(combo).kind {
            unbanished_ui::Kind::Control(unbanished_ui::Control::Combo { list, .. }) => *list,
            _ => panic!("selectResource is not a combo"),
        };
        let items = match &ctx.ui.node(list).kind {
            unbanished_ui::Kind::List(l) => l.items.clone(),
            _ => Vec::new(),
        };
        assert_eq!(items.len(), crops.len(), "{items:?}");
        assert!(ctx.ui.node(combo).enabled);
        let chosen = gameplay
            .entities
            .select_item(ctx.ui, town, combo, 0)
            .expect("the choice sends nothing");
        assert!(matches!(
            chosen,
            WorkPlaceClick::Command(unbanished_game::work::professions::Command::SetCrop {
                entity,
                crop: Some(c),
            }) if entity == id && crops.contains(&c)
        ));
    });
}

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

/// A crop field's window lists the known crops with their icons (`FUN_14007dd00`), and the
/// combo's label shows the chosen one's.
#[test]
fn crop_choices_have_icons() {
    let Some((mut game, _)) = generated() else {
        return;
    };
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let Ok(bytes) = std::fs::read(root.join("save-game/Save83.sav")) else {
        return;
    };
    let mut gameplay =
        GameplayLoop::from_save(crate::saves::saved::SavedGame::parse(&bytes).unwrap());
    game.with_ctx(|_, ctx| {
        gameplay.load(ctx);
        gameplay.startup(ctx);
        let field = {
            let town = gameplay.sim.town.as_ref().unwrap();
            town.placed
                .iter()
                .position(|p| p.alive && town.templates[p.template].reference.contains("CropField"))
                .expect("a crop field")
        };
        {
            let (town, screen) = (
                gameplay.sim.town.as_ref().unwrap(),
                gameplay.screen.as_mut().unwrap(),
            );
            gameplay
                .entities
                .select(ctx.ui, screen, ctx.archive, town, Entity::Placed(field));
        }
        gameplay.update(ctx, 1.0 / 30.0);
        let w = gameplay.entities.open.last().unwrap().root;
        let combo = ctx.ui.find(w, "selectResource").expect("selectResource");
        let list = crate::menu::combo_list(ctx.ui, combo).unwrap();
        let unbanished_ui::Kind::List(data) = &ctx.ui.node(list).kind else {
            panic!("no list");
        };
        assert!(!data.items.is_empty());
        assert_eq!(data.icons.len(), data.items.len());
        assert!(data.icons.iter().all(Option::is_some), "{:?}", data.items);
    });
}

/// The Limits window (`UtilityLimits`): each ResourceLimitUI row shows its slot's limit,
/// and a changed spinner sets it (`FUN_14007ccb0`).
#[test]
fn the_limits_window_sets_the_stock_limits() {
    let Some((mut game, mut gameplay)) = generated() else {
        return;
    };
    game.with_ctx(|_, ctx| {
        assert!(gameplay.script(ctx, "open Template/UtilityLimits.rsc"));
        gameplay.update(ctx, 1.0 / 30.0);
        let root = gameplay.utility.controllers()[0].0;
        let mut spinners = Vec::new();
        let mut stack = vec![root];
        while let Some(id) = stack.pop() {
            if ctx.ui.node(id).name == "editLimit" {
                spinners.push(id);
            }
            stack.extend(ctx.ui.children(id));
        }
        assert_eq!(spinners.len(), 11);
        let shown: Vec<_> = spinners
            .iter()
            .map(|&s| crate::ui::building_ui::spinner_value(ctx.ui, s).unwrap())
            .collect();
        for v in &shown {
            assert!(gameplay.sim.stock_warnings.limits.contains(v), "{shown:?}");
        }
        crate::ui::building_ui::set_spinner(ctx.ui, spinners[0], 345);
        gameplay.event(ctx, Event::Changed(spinners[0]));
        assert!(gameplay.sim.stock_warnings.limits.contains(&345));
    });
}

/// DestroyUI on a storage barn: its buttons ask for the demolition to be called off,
/// rebuilt or cleared.
#[test]
fn destroy_window_buttons_send_commands() {
    let Some((mut game, mut gameplay)) = generated() else {
        return;
    };
    let town = gameplay.sim.town.as_ref().unwrap();
    let id = town
        .storage_sites
        .iter()
        .map(|s| s.entity)
        .find(|&i| {
            town.templates[town.placed[i].template]
                .reference
                .contains("StorageBarn")
        })
        .unwrap();
    game.with_ctx(|_, ctx| {
        let town = gameplay.sim.town.as_ref().unwrap();
        gameplay.entities.select(
            ctx.ui,
            gameplay.screen.as_mut().unwrap(),
            ctx.archive,
            town,
            Entity::Placed(id),
        );
        let window = &gameplay.entities.open[0];
        assert!(window
            .bound
            .iter()
            .any(|c| matches!(c, crate::ui::building_ui::Controller::Destroy(_))));
        let root = window.root;
        let (reclaim, rebuild, destroy) = (
            ctx.ui.find(root, "buttonReclaim").unwrap(),
            ctx.ui.find(root, "buttonRebuild").unwrap(),
            ctx.ui.find(root, "buttonDestroy").unwrap(),
        );
        use unbanished_game::work::professions::Command;
        let click = |gameplay: &mut GameplayLoop, ui: &unbanished_ui::Ui, n| match gameplay
            .entities
            .click(ui, gameplay.sim.town.as_ref().unwrap(), n)
        {
            Some(WorkPlaceClick::Command(c)) => Some(c),
            _ => None,
        };
        assert_eq!(
            click(&mut gameplay, ctx.ui, reclaim),
            Some(Command::CancelDemolition { entity: id })
        );
        assert_eq!(
            click(&mut gameplay, ctx.ui, rebuild),
            Some(Command::StateMessage {
                entity: id,
                message: 0
            })
        );
        assert_eq!(
            click(&mut gameplay, ctx.ui, destroy),
            Some(Command::StateMessage {
                entity: id,
                message: 1
            })
        );
        // A building standing: no damage text, the reclaim button shows.
        gameplay
            .entities
            .refresh(ctx.ui, ctx.archive, gameplay.sim.town.as_ref().unwrap());
        assert!(ctx.ui.node(reclaim).visible());
        assert!(!ctx.ui.node(rebuild).visible());
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
