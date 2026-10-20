use super::*;

fn generated() -> Option<(crate::Game, GameplayLoop)> {
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../game-data/original/WinData");
    if !dir.join("data0.pkg").exists() {
        eprintln!("game data missing; skipping gameplay growth integration");
        return None;
    }
    let packages = ["data1.pkg", "patch1.pkg", "patch0.pkg", "data0.pkg"]
        .into_iter()
        .filter_map(|name| {
            std::fs::read(dir.join(name))
                .ok()
                .map(|data| (name.to_owned(), data))
        })
        .collect();
    let mut game =
        crate::Game::with_settings(packages, crate::settings::Settings::default()).unwrap();
    let mut gameplay = GameplayLoop::new(WorldParams {
        start_condition_ref: r"Game\StartCondition\Medium.rsc:resource".into(),
        ..WorldParams::default()
    });
    game.with_ctx(|_, ctx| {
        gameplay.load(ctx);
        gameplay.startup(ctx);
    });
    Some((game, gameplay))
}

#[test]
fn selection_host_and_highlight_leave_simulation_unchanged() {
    let Some((mut game, mut gameplay)) = generated() else {
        return;
    };
    let town = gameplay.town.as_ref().unwrap();
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
    let position = glam::Vec3::from(town.placed[id].position);
    let origin = position + glam::Vec3::new(0., -12., 10.);
    let direction = (position + glam::Vec3::Z - origin).normalize();
    let rng = gameplay.rng.clone();
    let time = gameplay.calendar.elapsed;
    let before = format!(
        "{:?}{:?}{:?}",
        town.citizens, town.agents.states, town.storages
    );
    game.with_ctx(|_, ctx| {
        let town = gameplay.town.as_ref().unwrap();
        assert_eq!(
            gameplay
                .selection
                .cast(ctx.archive, town, origin, direction, 400., None),
            Some(Entity::Placed(id))
        );
        for _ in 0..3 {
            gameplay.entities.select(
                ctx.ui,
                gameplay.screen.as_mut().unwrap(),
                ctx.archive,
                town,
                Entity::Placed(id),
            );
            assert_eq!(gameplay.entities.open.len(), 1);
            let window = &gameplay.entities.open[0];
            assert!(!window.controllers.is_empty());
            let root = window.root;
            assert!(ctx.ui.find(root, "labelTitle").is_some());
            let pages = ctx.ui.find(root, "pages").unwrap();
            assert!(matches!(
                ctx.ui.node(pages).kind,
                banished_ui::Kind::Control(banished_ui::Control::Page { selected: 1, .. })
            ));
            gameplay
                .screen
                .as_ref()
                .unwrap()
                .layout(ctx.ui, (1280, 800));
            ctx.ui
                .pointer_move(80, 80, gameplay.screen.as_ref().unwrap().root);
            let _ = ctx.ui.draw(gameplay.screen.as_ref().unwrap().root);
            let view = gameplay.view.as_ref().unwrap();
            let grid = banished_game::terrain::HeightGrid {
                width: view.terrain.size + 1,
                heights: &view.terrain.heights,
            };
            let geometry =
                gameplay
                    .highlight
                    .geometry(ctx.archive, town, &grid, Some(Entity::Placed(id)));
            assert!(!geometry.is_empty());
            assert_eq!(
                gameplay
                    .entities
                    .close_containing(ctx.ui, gameplay.screen.as_mut().unwrap(), root),
                Some(Entity::Placed(id))
            );
        }
    });
    let town = gameplay.town.as_ref().unwrap();
    assert_eq!(rng, gameplay.rng);
    assert_eq!(time, gameplay.calendar.elapsed);
    assert_eq!(
        before,
        format!(
            "{:?}{:?}{:?}",
            town.citizens, town.agents.states, town.storages
        )
    );
}

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
        let town = gameplay.town.as_ref().unwrap();
        let x = crate::ui::town_hall_ui::TownContext {
            stock_counts: &gameplay.stock_warnings.count,
            statistics: &gameplay.statistics,
        };
        th.refresh(ctx.ui, ctx.archive, town, &x);
        let text = |name: &str| {
            let n = ctx.ui.find(root, name).unwrap();
            match &ctx.ui.node(n).kind {
                banished_ui::Kind::Control(banished_ui::Control::Edit(e)) => e.text.text.clone(),
                banished_ui::Kind::Text(t) => t.text.clone(),
                _ => String::new(),
            }
        };
        let alive = town.citizens.iter().filter(|c| c.alive).count();
        assert_eq!(text("numberCitizens"), alive.to_string());
        assert!(!gameplay.statistics.series[0].is_empty(), "no sample yet");
        assert_eq!(gameplay.statistics.series[0][0], alive as i32);
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
    let terrain = banished_game::terrain::HeightGrid {
        width: view.terrain.size + 1,
        heights: &heights,
    };
    let id = game.with_ctx(|_, ctx| {
        let town = gameplay.town.as_mut().unwrap();
        let mut rng = banished_game::random::Random::new(7);
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
    let town = gameplay.town.as_mut().unwrap();
    // The next iteration activates the post (its TradeComponent).
    let mut rng = banished_game::random::Random::new(7);
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
        banished_game::commerce::trade::Offer {
            good: food,
            value: 2,
            amount: 10,
        },
        banished_game::commerce::trade::Offer {
            good: livestock,
            value: 1,
            amount: 2,
        },
    ];
    town.storages[storage].add(logs, 30);
    game.with_ctx(|_, ctx| {
        let town = gameplay.town.as_ref().unwrap();
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
        let value = banished_game::commerce::trade::sell_value(
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
            banished_game::work::professions::Command::Exchange {
                entity: id,
                sell: vec![(logs, sell)],
                buy: vec![(food, 4), (livestock, 2)],
            }
        );
        let town = gameplay.town.as_mut().unwrap();
        let unassigned = town.wildlife.unassigned.len();
        town.construction.commands.push(c);
        let mut rng = banished_game::random::Random::new(7);
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
            town.wildlife.animals[&a].flags & banished_game::nature::wildlife::WILD,
            0
        );
        // The window's amounts go back to none.
        let town = gameplay.town.as_ref().unwrap();
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
    let terrain = banished_game::terrain::HeightGrid {
        width: view.terrain.size + 1,
        heights: &heights,
    };
    let id = game.with_ctx(|_, ctx| {
        let town = gameplay.town.as_mut().unwrap();
        let n = town.cells.size as i32;
        let empty = |town: &crate::start::StartingTown, x: i32, y: i32| {
            (y..y + 6).all(|cy| {
                (x..x + 6).all(|cx| {
                    let i = (cy * n + cx) as usize;
                    cx < n
                        && cy < n
                        && town.cells.types[i] == banished_game::terrain::CellType::GROUND
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
    let town = gameplay.town.as_mut().unwrap();
    let mut rng = banished_game::random::Random::new(7);
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
        let town = gameplay.town.as_ref().unwrap();
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
            banished_ui::Kind::Control(banished_ui::Control::Combo { list, .. }) => *list,
            _ => panic!("selectResource is not a combo"),
        };
        let items = match &ctx.ui.node(list).kind {
            banished_ui::Kind::List(l) => l.items.clone(),
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
            WorkPlaceClick::Command(banished_game::work::professions::Command::SetCrop {
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
        let town = gameplay.town.as_ref().unwrap();
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
            banished_ui::Kind::Control(banished_ui::Control::Combo { list, .. }) => *list,
            _ => panic!("comboTopic is not a combo"),
        };
        let items = match &ctx.ui.node(list).kind {
            banished_ui::Kind::List(l) => l.items.clone(),
            _ => Vec::new(),
        };
        assert_eq!(items.len(), 66);
        assert_eq!(items[0], "Contents");
        let text = |ui: &banished_ui::Ui| match &ui.node(doc).kind {
            banished_ui::Kind::Control(banished_ui::Control::Document(_)) => {
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

/// The achievement list's goals read the started town: it has a storage barn (Builder's
/// goal is met), not 300 people, and is not in the mountains.
#[test]
fn achievements_read_the_town() {
    let Some((mut game, gameplay)) = generated() else {
        return;
    };
    game.with_ctx(|_, ctx| {
        let list = crate::achievements::load(ctx.archive);
        assert_eq!(list.len(), 36);
        let town = gameplay.town.as_ref().unwrap();
        let terrain = format!(r"Game\Terrain\{}.rsc", gameplay.params.terrain);
        let x = crate::achievements::GoalContext {
            town,
            statistics: &gameplay.statistics,
            year: gameplay.calendar.year,
            year_seconds: 0.0,
            terrain: &terrain,
            climate: &gameplay.params.climate_ref,
            size: 0,
        };
        let state = |title: &str, n: usize| {
            let a = list.iter().find(|a| a.desc.title == title).unwrap();
            crate::achievements::check(&a.goals[n], &Default::default(), &x, Some(ctx.archive))
        };
        use crate::achievements::GoalState;
        let builder = list.iter().find(|a| a.desc.title == "Builder").unwrap();
        let barn = builder
            .goals
            .iter()
            .position(|g| format!("{:?}", g.kind).contains("StorageBarn"))
            .unwrap();
        assert_eq!(state("Builder", barn), GoalState::Met);
        assert_eq!(state("Settlement", 0), GoalState::NotMet);
        let mountains = list.iter().find(|a| a.desc.title == "MountainMen").unwrap();
        let map = mountains
            .goals
            .iter()
            .position(|g| format!("{:?}", g.kind).contains("Mountains"))
            .unwrap();
        assert_eq!(state("MountainMen", map), GoalState::NotMet);
        // Isolationist fails once there is a trading post; there is none.
        let iso = list
            .iter()
            .find(|a| a.desc.title == "Isolationist")
            .unwrap();
        let post = iso
            .goals
            .iter()
            .position(|g| format!("{:?}", g.kind).contains("TradingPost"))
            .unwrap();
        assert_eq!(state("Isolationist", post), GoalState::Met);
    });
}

/// The first tutorial from its scenario: the window opens with step 0's text, only the
/// tutorial's next button (and the options) usable; pressing next enters step 1.
#[test]
fn tutorial_starts_and_steps_on() {
    let Some((mut game, _)) = generated() else {
        return;
    };
    let mut gameplay = None;
    game.with_ctx(|_, ctx| {
        let list = scenarios(ctx.archive);
        assert_eq!(list.len(), 4);
        let first = list
            .iter()
            .position(|(r, _)| r.contains("GettingStarted"))
            .unwrap();
        let params = scenario_params(ctx.ui, ctx.global.as_ref(), &list[first].1, first);
        assert_eq!(params.seed, 120323451);
        // It starts from `Save\TutorialGettingStarted.sav`, as the scenario's.
        let mut g = scenario_game(
            ctx.ui,
            ctx.archive,
            ctx.global.as_ref(),
            &list[first].1,
            first,
        );
        assert!(g.saved.is_some(), "the tutorial's save did not load");
        // Every tutorial names a save, and each one reads.
        for (i, (r, s)) in list.iter().enumerate() {
            assert!(!s.save.is_empty(), "{r}");
            let g = scenario_game(ctx.ui, ctx.archive, ctx.global.as_ref(), s, i);
            assert!(g.saved.is_some(), "{r}: {}", s.save);
        }
        assert_eq!(g.params.scenario, Some(first));
        g.load(ctx);
        g.startup(ctx);
        gameplay = Some(g);
    });
    let mut gameplay = gameplay.unwrap();
    game.with_ctx(|_, ctx| {
        gameplay.update(ctx, 1.0 / 30.0);
        gameplay.update(ctx, 1.0 / 30.0);
        let t = gameplay.tutorial.as_ref().expect("no tutorial");
        assert_eq!(t.step, 0);
        assert!(t.def.steps.len() > 50);
        let ui = t.ui.clone().expect("no tutorial window");
        let pre = ui.pre.unwrap();
        let text = format!("{:?}", ctx.ui.node(pre).kind);
        assert!(!text.contains("Step_Keys_0_Pre"), "untranslated: {text}");
        let next = ui.next.unwrap();
        assert!(ctx.ui.node(next).usable(), "next is locked");
        let housing = gameplay
            .toolbar
            .as_ref()
            .unwrap()
            .button(ctx.ui, "housing")
            .unwrap();
        assert!(!ctx.ui.node(housing).usable(), "the toolbar is not locked");
        gameplay.event(ctx, Event::Clicked(next));
        gameplay.update(ctx, 1.0 / 30.0);
        assert_eq!(gameplay.tutorial.as_ref().unwrap().step, 1);
        // Through the keys to the first house: the housing button alone opens up.
        for step in 2..=9 {
            gameplay.event(ctx, Event::Clicked(next));
            gameplay.update(ctx, 1.0 / 30.0);
            assert_eq!(gameplay.tutorial.as_ref().unwrap().step, step);
        }
        let toolbar = gameplay.toolbar.as_ref().unwrap();
        let housing = toolbar.button(ctx.ui, "housing").unwrap();
        let food = toolbar.button(ctx.ui, "food").unwrap();
        assert!(ctx.ui.node(housing).usable());
        assert!(!ctx.ui.node(food).usable());
    });
}

/// The placement overlay (`FUN_140106850`): over the barn the footprint has red cells; on
/// clear ground every cell is green and the porch's `.` cells show the entrance arrow
/// (frame 2) turned with the building.
#[test]
fn the_placement_overlay_shows_validity_and_the_entrance() {
    let Some((mut game, mut gameplay)) = generated() else {
        return;
    };
    game.with_ctx(|_, ctx| {
        let town = gameplay.town.as_mut().unwrap();
        let view = gameplay.view.as_ref().unwrap();
        let grid = banished_game::terrain::HeightGrid {
            width: view.terrain.size + 1,
            heights: &view.terrain.heights,
        };
        let mut tool = PlacementTool::start(
            ctx.archive,
            town,
            "Template\\WoodHouse.rsc:resource",
            &mut gameplay.rng,
        )
        .unwrap();
        let barn = town
            .placed
            .iter()
            .find(|p| town.templates[p.template].reference.contains("StorageBarn"))
            .unwrap()
            .cell;
        tool.hover(town, &grid, Some((barn.0 + 2, barn.1 + 2)), 0);
        let over = tool.overlay(town, &grid, tool.rect().unwrap(), 0, false);
        assert!(over.iter().any(|c| c.argb == 0xffff_0000));
        let at = tool.find_clear_spot(town, &grid, town.start).unwrap();
        let mut offsets = Vec::new();
        for rotation in 0..4 {
            tool.hover(town, &grid, Some(at), rotation);
            let Some(rect) = tool.rect().filter(|_| tool.valid()) else {
                continue;
            };
            let over = tool.overlay(town, &grid, rect, rotation * 90, false);
            assert!(over.iter().all(|c| c.argb == 0xff00_ff00));
            let arrows: Vec<_> = over.iter().filter(|c| c.frame == 2).collect();
            assert_eq!(arrows.len(), 4, "the porch");
            // Every arrow turns with the building by the same entrance offset.
            offsets.extend(arrows.iter().map(|c| c.rotation - rotation * 90));
        }
        assert!(offsets.len() >= 8, "{offsets:?}");
        assert!(
            offsets.iter().all(|&o| o == offsets[0] && o >= 0),
            "{offsets:?}"
        );
        let restricted = tool.overlay(town, &grid, tool.rect().unwrap(), 0, true);
        assert!(restricted.iter().all(|c| c.argb == 0xffff_ff00));
    });
}

/// A forester's work radius: its ghost shows the ring (`FUN_1401033b0` →
/// `FUN_140055d90`), and so does its window while open (RadiusUI, `FUN_14007cb20`).
#[test]
fn work_radii_show_while_placing_and_selected() {
    let Some((mut game, mut gameplay)) = generated() else {
        return;
    };
    game.with_ctx(|_, ctx| {
        let count = |g: &mut GameplayLoop, ctx: &mut Ctx| {
            g.refresh_decals(ctx.archive);
            g.view
                .as_ref()
                .unwrap()
                .extras
                .as_ref()
                .unwrap()
                .decals
                .len()
        };
        let base = count(&mut gameplay, ctx);
        assert!(gameplay.script(ctx, "tool Template/ForesterLodge.rsc"));
        let start = gameplay.town.as_ref().unwrap().start;
        {
            let town = gameplay.town.as_ref().unwrap();
            let view = gameplay.view.as_ref().unwrap();
            let grid = banished_game::terrain::HeightGrid {
                width: view.terrain.size + 1,
                heights: &view.terrain.heights,
            };
            let tool = gameplay.placement.as_mut().unwrap();
            tool.hover(town, &grid, Some(start), 0);
            assert!(tool.radius_decal().is_some());
        }
        assert_eq!(count(&mut gameplay, ctx), base + 1);
        gameplay.end_placement(ctx);
        assert_eq!(count(&mut gameplay, ctx), base);
        assert!(gameplay.script(ctx, "place Template/ForesterLodge.rsc"));
        let with_site = count(&mut gameplay, ctx);
        assert!(gameplay.script(ctx, "select"));
        assert_eq!(count(&mut gameplay, ctx), with_site + 1);
    });
}

/// A citizen window's Follow (`FUN_140065080`): the dialog opens with the citizen's name,
/// the camera stays on the citizen, and Escape closes it. Center, Home and Profession act
/// on the window's citizen.
#[test]
fn follow_keeps_the_camera_on_the_citizen() {
    let Some((mut game, mut gameplay)) = generated() else {
        return;
    };
    game.with_ctx(|_, ctx| {
        let entity = Entity::Citizen(0);
        {
            let (town, screen) = (
                gameplay.town.as_ref().unwrap(),
                gameplay.screen.as_mut().unwrap(),
            );
            gameplay
                .entities
                .select(ctx.ui, screen, ctx.archive, town, entity);
        }
        let root = gameplay.entities.open[0].root;
        let follow = ctx.ui.find(root, "buttonFollow").expect("buttonFollow");
        gameplay.event(ctx, Event::Clicked(follow));
        assert!(gameplay.follow.is_some());
        for _ in 0..30 {
            gameplay.update(ctx, 1.0 / 30.0);
        }
        let at = gameplay.town.as_ref().unwrap().citizens[0].position;
        let SceneCamera::Map(camera) = &gameplay.view.as_ref().unwrap().camera else {
            panic!("no map camera");
        };
        assert!((camera.target - glam::Vec2::new(at[0], at[1])).length() < 1e-3);
        let name = gameplay.town.as_ref().unwrap().citizens[0].name.clone();
        let screen = gameplay.screen.as_ref().unwrap();
        let shown = screen
            .dialogs
            .iter()
            .rev()
            .find_map(|d| ctx.ui.find(d.root, "textFollow"))
            .map(|n| crate::ui::utility::text_of(ctx.ui, n))
            .unwrap_or_default();
        assert!(shown.contains(&name), "{shown:?} {name:?}");
        gameplay.raw(ctx, RawInput::Escape);
        assert!(gameplay.follow.is_none());
        // Center: the camera on the citizen's model.
        let center = ctx.ui.find(root, "buttonCenter").expect("buttonCenter");
        gameplay.event(ctx, Event::Clicked(center));
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
        let town = gameplay.town.as_mut().unwrap();
        let view = gameplay.view.as_ref().unwrap();
        let grid = banished_game::terrain::HeightGrid {
            width: view.terrain.size + 1,
            heights: &view.terrain.heights,
        };
        let mut tool = PlacementTool::start(
            ctx.archive,
            town,
            "Template/ForesterLodge.rsc",
            &mut gameplay.rng,
        )
        .unwrap();
        let near = town.start;
        tool.find_clear_spot(town, &grid, near).unwrap();
        let id = tool.place(ctx.archive, town, 0).unwrap();
        gameplay.update(ctx, 1.0 / 30.0);
        gameplay.paused = true;
        let (town, screen) = (
            gameplay.town.as_ref().unwrap(),
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
        let time = gameplay.calendar.elapsed;
        gameplay.event(ctx, Event::Clicked(button));
        let town = gameplay.town.as_ref().unwrap();
        assert!(town.construction.commands.is_empty());
        gameplay.update(ctx, 1.0 / 30.0);
        assert_eq!(gameplay.calendar.elapsed, time, "the game ran while paused");
        let now = crate::ui::utility::text_of(ctx.ui, count);
        assert_ne!(now, shown, "the window still shows the old most workers");
    });
}

/// The Path tool (`FUN_1400e2c40`): a worked building's workers' commutes, home to
/// workplace, drawn as cells and steps; Escape ends it.
#[test]
fn the_path_tool_shows_commutes() {
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
        gameplay.start_tool(ctx, "Template\\UtilityPath.rsc:resource");
        assert!(gameplay.path_tool.is_some());
        let town = gameplay.town.as_mut().unwrap();
        let c = town
            .agents
            .states
            .iter()
            .position(|s| s.home.is_some() && s.workplace.is_some())
            .expect("a commuter in Save83");
        let workplace = town.agents.states[c].workplace.unwrap();
        let tool = gameplay.path_tool.as_mut().unwrap();
        tool.pick(town, &mut gameplay.rng, Entity::Citizen(c));
        let (lines, cells) = tool.overlay(town.cells.size as i32);
        assert!(cells.len() >= 2, "{}", cells.len());
        assert!(!lines.is_empty());
        assert!(cells.iter().all(|c| c.frame == 4 && c.argb == 0xfffc_cc4c));
        // The workplace: every worker's commute, at least as many cells.
        tool.pick(town, &mut gameplay.rng, Entity::Placed(workplace));
        let (_, all) = tool.overlay(town.cells.size as i32);
        assert!(all.len() >= cells.len());
        gameplay.raw(ctx, RawInput::Escape);
        assert!(gameplay.path_tool.is_none());
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
            let town = gameplay.town.as_ref().unwrap();
            town.placed
                .iter()
                .position(|p| p.alive && town.templates[p.template].reference.contains("CropField"))
                .expect("a crop field")
        };
        {
            let (town, screen) = (
                gameplay.town.as_ref().unwrap(),
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
        let banished_ui::Kind::List(data) = &ctx.ui.node(list).kind else {
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
            assert!(gameplay.stock_warnings.limits.contains(v), "{shown:?}");
        }
        crate::ui::building_ui::set_spinner(ctx.ui, spinners[0], 345);
        gameplay.event(ctx, Event::Changed(spinners[0]));
        assert!(gameplay.stock_warnings.limits.contains(&345));
    });
}

/// The start structures stamp the map's object (`+8`, `FUN_1400a30f0`) on their
/// footprint, so picking finds the storage yard through its cells.
#[test]
fn start_structures_stamp_their_cells() {
    let Some((_, gameplay)) = generated() else {
        return;
    };
    let town = gameplay.town.as_ref().unwrap();
    let n = town.cells.size;
    let mut checked = 0;
    for (e, p) in town.placed.iter().enumerate() {
        if !p.alive || town.templates[p.template].storage.is_none() {
            continue;
        }
        let cells: Vec<_> = (p.cell.1..p.cell.1 + p.footprint.1)
            .flat_map(|y| (p.cell.0..p.cell.0 + p.footprint.0).map(move |x| (x, y)))
            .collect();
        let stamped = cells
            .iter()
            .filter(|&&(x, y)| town.cells.objects[y as usize * n + x as usize] == Some(e))
            .count();
        eprintln!(
            "{} {stamped}/{}",
            town.templates[p.template].reference,
            cells.len()
        );
        assert!(stamped > 0, "{}", town.templates[p.template].reference);
        checked += 1;
    }
    assert!(checked > 0);
}

/// Planned road cells get the roads' tiled decals (`FUN_1400b4840`): one per tile and
/// kind, four vertices a cell, the alpha the built byte.
#[test]
fn roads_get_decals() {
    let Some((mut game, mut gameplay)) = generated() else {
        return;
    };
    game.with_ctx(|_, ctx| {
        assert!(
            gameplay.road_materials.iter().all(|m| m.is_some()),
            "{:?}",
            gameplay.road_materials
        );
        gameplay.update(ctx, 1.0 / 30.0);
        let count = |g: &GameplayLoop| -> usize {
            let e = g.view.as_ref().unwrap().extras.as_ref().unwrap();
            e.decals.iter().map(|d| d.vertices.len() / 20).sum()
        };
        let before = count(&gameplay);
        let town = gameplay.town.as_mut().unwrap();
        let n = town.cells.size as i32;
        let (x, y) = (town.start.0 as i32, town.start.1 as i32 + 12);
        for dx in 0..6 {
            town.construction
                .commands
                .push(banished_game::work::professions::Command::PlanRoad {
                    cell: y * n + x + dx,
                    kind: 1,
                    ty: 3,
                    material: None,
                });
        }
        gameplay.update(ctx, 1.0 / 30.0);
        let town = gameplay.town.as_ref().unwrap();
        let planned = (0..6)
            .filter(|dx| town.cells.road_kind[(y * n + x + dx) as usize] == 1)
            .count();
        assert!(0 < planned, "no road planned");
        assert_eq!(count(&gameplay), before + planned * 4);
    });
}

/// A burning barn's FireDescription slots start its fire particles and its fire sound;
/// put out, they fade and stop.
#[test]
fn burning_buildings_get_particles() {
    let Some((mut game, mut gameplay)) = generated() else {
        return;
    };
    game.with_ctx(|_, ctx| {
        gameplay.update(ctx, 1.0 / 30.0);
        let town = gameplay.town.as_mut().unwrap();
        let barn = town
            .storage_sites
            .iter()
            .map(|s| s.entity)
            .find(|&i| {
                town.templates[town.placed[i].template]
                    .reference
                    .contains("StorageBarn")
            })
            .unwrap();
        let fire = |g: &GameplayLoop| {
            g.view
                .as_ref()
                .unwrap()
                .particles
                .iter()
                .any(|b| b.material.contains("Fire"))
        };
        assert!(!fire(&gameplay));
        let town = gameplay.town.as_mut().unwrap();
        town.construction
            .sites
            .get_mut(&barn)
            .unwrap()
            .fire
            .as_mut()
            .unwrap()
            .burn = 1_000_000_000;
        for _ in 0..30 {
            gameplay.update(ctx, 1.0 / 30.0);
        }
        assert!(fire(&gameplay));
        // Its FireDescription's emitter slot plays at the building.
        let (_, playing) = ctx.audio.scene.active();
        assert!(0 < playing);
        let town = gameplay.town.as_mut().unwrap();
        town.construction
            .sites
            .get_mut(&barn)
            .unwrap()
            .fire
            .as_mut()
            .unwrap()
            .burn = 0;
        for _ in 0..120 {
            gameplay.update(ctx, 1.0 / 30.0);
        }
        assert!(!fire(&gameplay));
        assert!(ctx.audio.scene.active().1 < playing);
    });
}

/// The starting buildings' footprints are drawn opaque (their ClearArea reached state 2),
/// a cell for each of their footprint's cells.
#[test]
fn starting_buildings_get_decals() {
    let Some((mut game, mut gameplay)) = generated() else {
        return;
    };
    game.with_ctx(|_, ctx| {
        gameplay.update(ctx, 1.0 / 30.0);
        let town = gameplay.town.as_ref().unwrap();
        let with: Vec<usize> = (0..town.placed.len())
            .filter(|&i| {
                town.placed[i].alive
                    && gameplay.decal_descriptions[town.placed[i].template].is_some()
            })
            .collect();
        assert!(!with.is_empty());
        for &i in &with {
            let d = town
                .construction
                .decals
                .get(&i)
                .copied()
                .unwrap_or_default();
            assert!(
                d.opaque,
                "{}",
                town.templates[town.placed[i].template].reference
            );
        }
        let cells: usize = with
            .iter()
            .map(|&i| (town.placed[i].footprint.0 * town.placed[i].footprint.1) as usize)
            .sum();
        let e = gameplay.view.as_ref().unwrap().extras.as_ref().unwrap();
        let vertices: usize = e.decals.iter().map(|d| d.vertices.len() / 20).sum();
        assert!(cells * 4 <= vertices, "{cells} cells, {vertices} vertices");
    });
}

/// DestroyUI on a storage barn: its buttons ask for the demolition to be called off,
/// rebuilt or cleared.
#[test]
fn destroy_window_buttons_send_commands() {
    let Some((mut game, mut gameplay)) = generated() else {
        return;
    };
    let town = gameplay.town.as_ref().unwrap();
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
        let town = gameplay.town.as_ref().unwrap();
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
        use banished_game::work::professions::Command;
        let click = |gameplay: &mut GameplayLoop, ui: &banished_ui::Ui, n| match gameplay
            .entities
            .click(ui, gameplay.town.as_ref().unwrap(), n)
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
            .refresh(ctx.ui, ctx.archive, gameplay.town.as_ref().unwrap());
        assert!(ctx.ui.node(reclaim).visible());
        assert!(!ctx.ui.node(rebuild).visible());
    });
}

#[test]
fn status_bar_layout_open_close_and_draw_leave_simulation_unchanged() {
    let Some((mut game, mut gameplay)) = generated() else {
        return;
    };
    let town = gameplay.town.as_mut().unwrap();
    let rules = town.agents.rules();
    for s in &mut town.agents.states {
        s.health = rules.health_max * 0.5;
        s.happiness = rules.happiness * 0.5;
        s.status = 1 | 16;
    }
    let people = town.citizens.len() as u32;
    let state = format!("{:?}", town.agents.states);
    let rng = gameplay.rng.clone();
    let time = gameplay.calendar.elapsed;
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
    assert_eq!(rng, gameplay.rng);
    assert_eq!(time, gameplay.calendar.elapsed);
    assert_eq!(
        state,
        format!("{:?}", gameplay.town.as_ref().unwrap().agents.states)
    );
}

#[test]
fn audio_plays_music_ambience_and_keeps_the_simulation_stream() {
    let Some((mut game, mut gameplay)) = generated() else {
        return;
    };
    game.with_ctx(|_, ctx| {
        ctx.audio.set_master_volume(0.0);
        for _ in 0..3 {
            gameplay.update(ctx, 1.0 / 30.0);
        }
        let (effects, effects3d) = ctx.audio.scene.active();
        eprintln!("2D effects {effects}, 3D effects {effects3d}");
        // Environment classes (warm and cold) and the wind share 3D effects.
        assert!(effects3d >= 2, "{effects3d}");
        // The music track and the audible ambience.
        assert!(effects >= 1, "{effects}");
        let ambience = gameplay.ambience.as_ref().unwrap();
        assert!(ambience.region_count() > 0);
        // Every generated map has a river (`FUN_1400d2560`): emitters every 8 units.
        let length: f32 = gameplay
            .river_paths
            .iter()
            .flat_map(|p| p.windows(2))
            .map(|w| (glam::Vec2::from(w[1]) - glam::Vec2::from(w[0])).length())
            .sum();
        eprintln!("river length {length}, emitters {}", ambience.river_count());
        assert!(ambience.river_count() > 0);
        assert!(ambience.river_count() as f32 <= length / 8.0);
    });
}

#[test]
fn pause_menu_pauses_sounds_and_the_simulation() {
    use crate::audio::sound::sound_type;
    let Some((mut game, mut gameplay)) = generated() else {
        return;
    };
    game.with_ctx(|_, ctx| {
        ctx.audio.set_master_volume(0.0);
        gameplay.update(ctx, 1.0 / 30.0);
        gameplay.raw(ctx, RawInput::Escape);
        assert!(gameplay.pause_menu.is_some());
        for kind in [sound_type::MUSIC, sound_type::AMBIENT, sound_type::EFFECT] {
            assert!(ctx.audio.scene.is_paused(kind));
        }
        assert!(!ctx.audio.scene.is_paused(sound_type::UI));
        let (rng, seconds) = (gameplay.rng.clone(), gameplay.calendar.seconds());
        for _ in 0..3 {
            gameplay.update(ctx, 1.0 / 30.0);
        }
        assert_eq!(gameplay.calendar.seconds(), seconds);
        assert_eq!(format!("{:?}", gameplay.rng), format!("{rng:?}"));
        // Escape is buttonResume's action.
        gameplay.raw(ctx, RawInput::Escape);
        assert!(gameplay.pause_menu.is_none());
        assert!(!ctx.audio.scene.is_paused(sound_type::MUSIC));
        gameplay.update(ctx, 1.0 / 30.0);
        assert!(gameplay.calendar.seconds() > seconds);
    });
}

#[test]
fn rain_effect_drives_the_rain_emitter() {
    let Some((mut game, mut gameplay)) = generated() else {
        return;
    };
    assert_eq!(
        gameplay.environment.as_ref().unwrap().fade_times[0],
        Some(20.0)
    );
    game.with_ctx(|_, ctx| {
        ctx.audio.set_master_volume(0.0);
        gameplay.update(ctx, 1.0 / 30.0);
        let (_, before) = ctx.audio.scene.active();
        gameplay.environment.as_mut().unwrap().effects[0] =
            Some(banished_game::nature::environment::WeatherEffect {
                intensity: 0.5,
                direction: 1.0,
            });
        gameplay.update(ctx, 1.0 / 30.0);
        let ambience = gameplay.ambience.as_ref().unwrap();
        assert!(ambience.weather_active(0));
        // The rain emitter's effect plays (other 3D one-shots come and go with the game
        // stream, so the scene's total is not compared).
        let handles = ambience.weather_handles(0);
        assert!(handles
            .iter()
            .any(|&h| ctx.audio.scene.emitter_count(h) > 0));
        let _ = before;
        // A save made now has the rain emitter's entity, its first slot at the intensity.
        let w = ambience.saved().weather;
        assert_eq!(w.len(), 1);
        assert!(w[0].0.contains("RainEffect"), "{}", w[0].0);
        assert!((w[0].2[0] - 0.5).abs() < 0.01, "{:?}", w[0].2);
        let g = gameplay.save_state(ctx.archive, ctx.ui).unwrap();
        let rain = g.entities.iter().flatten().filter(|&&e| {
            let o = &g.objects[&e];
            matches!(o.fields.at("entity.template"),
                Some(banished_formats::savegame::Value::Ext(r))
                    if g.references[r].contains("RainEffect"))
        });
        assert_eq!(rain.count(), 1);
        // Rain stops and cannot start again in this step (whatever the stream draws).
        let env = gameplay.environment.as_mut().unwrap();
        env.effects[0] = None;
        env.fade_times[0] = None;
        gameplay.update(ctx, 1.0 / 30.0);
        assert!(!gameplay.ambience.as_ref().unwrap().weather_active(0));
        assert!(handles
            .iter()
            .all(|&h| ctx.audio.scene.emitter_count(h) == 0));
    });
}

#[test]
fn building_emitters_register_without_playing() {
    let Some((mut game, mut gameplay)) = generated() else {
        return;
    };
    game.with_ctx(|_, ctx| {
        ctx.audio.set_master_volume(0.0);
        gameplay.update(ctx, 1.0 / 30.0);
        let town = gameplay.town.as_ref().unwrap();
        let building = town
            .placed
            .iter()
            .position(|p| {
                p.alive
                    && ctx
                        .archive
                        .read_ref(&format!(
                            "{}:ambientemitter",
                            town.templates[p.template]
                                .reference
                                .replace('\\', "/")
                                .split(':')
                                .next()
                                .unwrap()
                        ))
                        .is_ok()
            })
            .expect("a building with an ambient emitter");
        let at = town.placed[building].position;
        let ambience = gameplay.ambience.as_mut().unwrap();
        assert!(ambience.entity_emitters() > 0);
        // Autostart is off: nothing plays until building state starts a slot (fire).
        let (_, before) = ctx.audio.scene.active();
        ambience.start_entity_slot(ctx.audio, building, 0, glam::Vec3::from(at), None);
        assert_eq!(ctx.audio.scene.active().1, before + 1);
        ambience.stop_entity_slot(ctx.audio, building, 0);
        assert_eq!(ctx.audio.scene.active().1, before);
    });
}

#[test]
fn event_ribbon_popup_and_goto() {
    let Some((mut game, mut gameplay)) = generated() else {
        return;
    };
    let rng = gameplay.rng.clone();
    game.with_ctx(|_, ctx| {
        let frame = |g: &mut GameplayLoop, ctx: &mut crate::loops::Ctx| {
            let screen = g.screen.as_mut().unwrap();
            screen.layout(ctx.ui, (1280, 800));
            let width = g.toolbar.as_ref().unwrap().width(ctx.ui);
            g.notify
                .as_mut()
                .unwrap()
                .update(ctx.ui, screen, &g.events, 0.1, width)
        };
        frame(&mut gameplay, ctx);
        // Low food (no cell), a fire at a cell, and a type-26 event (popup, icon 12).
        gameplay
            .events
            .add(22, 3, -1, 0, 1, Default::default(), false);
        let n = gameplay.town.as_ref().unwrap().cells.size as i32;
        gameplay
            .events
            .add(31, 3, 70 * n + 30, 0, 1, Default::default(), false);
        gameplay
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
        let visible = |ui: &banished_ui::Ui, n| ui.node(n).flags & 1 == 0;
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
    assert_eq!(rng, gameplay.rng);
}

#[test]
fn speed_loop_smooths_once_and_pause_still_updates_gpu_instances() {
    let Some((mut game, mut gameplay)) = generated() else {
        return;
    };
    let town = gameplay.town.as_mut().expect("generated starting town");
    // Use the first scheduled tree so the quota visits it during this update. Deliberately
    // lag its visible scale to distinguish one smoothing pass from ten speed iterations.
    let id = town
        .placed
        .iter()
        .position(|p| {
            p.alive
                && town.templates[p.template]
                    .growth
                    .as_ref()
                    .is_some_and(|d| d.f28 > 0.0)
        })
        .expect("growing tree");
    town.placed[id].growth = 0.2;
    let state = town.placed[id].growth_state.as_mut().unwrap();
    state.smoothed = 0.0;
    state.smoothing = true;
    gameplay.speed = 3; // 10 iterations
    game.with_ctx(|_, ctx| {
        gameplay.update(ctx, 0.5);
    });
    assert_eq!(gameplay.calendar.elapsed, 5_000_000);
    let town = gameplay.town.as_ref().unwrap();
    let value = town.placed[id].growth;
    let d = town.templates[town.placed[id].template]
        .growth
        .as_ref()
        .unwrap();
    let target = value.min(1.0).max(d.f60);
    let smoothed = town.placed[id].growth_state.as_ref().unwrap().smoothed;
    assert_eq!(smoothed.to_bits(), (target * 0.009_999_99f32).to_bits());
    let revision = gameplay
        .view
        .as_ref()
        .unwrap()
        .models
        .as_ref()
        .unwrap()
        .revision;
    assert!(revision > 0);

    let mut renderer = pollster::block_on(crate::gfx::render::Renderer::headless(320, 200));
    renderer.set_scene(gameplay.view.as_ref().unwrap());
    gameplay.paused = true;
    game.with_ctx(|_, ctx| {
        gameplay.update(ctx, 0.5);
    });
    assert_eq!(gameplay.calendar.elapsed, 5_000_000);
    let town = gameplay.town.as_ref().unwrap();
    assert_eq!(town.placed[id].growth.to_bits(), value.to_bits());
    let after_pause = town.placed[id].growth_state.as_ref().unwrap().smoothed;
    assert_eq!(
        after_pause.to_bits(),
        (smoothed * 0.99 + target * 0.009_999_99f32).to_bits()
    );
    let view = gameplay.view.as_ref().unwrap();
    assert!(view.models.as_ref().unwrap().revision > revision);
    renderer.set_scene(view); // exercises COPY_DST updates on an already uploaded scene
    let rgba = renderer.capture(&crate::gfx::render::Frame {
        terrain: Some(view.frame(renderer.aspect(), 5.0)),
        clear: crate::CLEAR,
        ui: None,
    });
    assert_eq!(rgba.len(), 320 * 200 * 4);
    assert!(rgba.chunks_exact(4).any(|pixel| pixel[..3] != [0, 0, 0]));
    eprintln!("generated town: speed/pause growth phases and live GPU instance upload verified");
}

#[test]
fn tree_expiry_plays_packaged_clip_and_removes_after_completion() {
    let Some((mut game, mut gameplay)) = generated() else {
        return;
    };
    let town = gameplay.town.as_mut().unwrap();
    let id = town
        .placed
        .iter()
        .position(|p| {
            p.alive
                && town.templates[p.template]
                    .natural
                    .as_ref()
                    .is_some_and(|n| n.flag)
        })
        .expect("tree resource");
    let p = &mut town.placed[id];
    p.growth_state.as_mut().unwrap().maximum = 0.01;
    p.growth = 1.0;
    p.scale = [1.0; 4];
    let position = p.position;
    let cell = p.cell;
    let model = town.templates[p.template].model.as_ref().unwrap();
    let mesh = model.variants[p.variant].meshes[p.mesh]
        .as_ref()
        .unwrap()
        .clone();
    let group = town.templates[p.template].animations[p.variant]
        .as_ref()
        .unwrap();
    assert_eq!(group.clips[2].name, "Remove");
    assert_eq!(group.clips[2].duration_ticks, 5_066_667);
    if let SceneCamera::Map(camera) = &mut gameplay.view.as_mut().unwrap().camera {
        camera.target = glam::Vec2::new(position[0], position[1]);
        camera.distance = 18.0;
    }
    // The chosen tree still uses the real quota scheduler. Wait for its terminal growth tick.
    for _ in 0..2000 {
        game.with_ctx(|_, ctx| {
            gameplay.update(ctx, 1.0 / 30.0);
        });
        if gameplay.town.as_ref().unwrap().placed[id]
            .animation
            .is_some()
        {
            break;
        }
    }
    let p = &gameplay.town.as_ref().unwrap().placed[id];
    let playback = p
        .animation
        .as_ref()
        .expect("growth transition started animation 2");
    assert_eq!(playback.clip, 2);
    assert_eq!(playback.elapsed, 0); // created after this iteration's animation phase
    assert!(p.alive);
    assert!(p.removal_requested);
    let standing = gameplay
        .view
        .as_ref()
        .unwrap()
        .models
        .as_ref()
        .unwrap()
        .groups
        .iter()
        .find(|g| g.name == mesh)
        .unwrap()
        .instances[0];

    let mut renderer = pollster::block_on(crate::gfx::render::Renderer::headless(640, 400));
    fn capture(
        gameplay: &GameplayLoop,
        renderer: &mut crate::gfx::render::Renderer,
        name: &str,
    ) -> Vec<u8> {
        let view = gameplay.view.as_ref().unwrap();
        renderer.set_scene(view);
        let rgba = renderer.capture(&crate::gfx::render::Frame {
            terrain: Some(view.frame(renderer.aspect(), gameplay.calendar.seconds())),
            clear: crate::CLEAR,
            ui: None,
        });
        if let Ok(directory) = std::env::var("BANISHED_CAPTURE_TREE_EXPIRY") {
            let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../..")
                .join(directory)
                .join(format!("tree-{name}.png"));
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            let file = std::fs::File::create(path).unwrap();
            let mut encoder = png::Encoder::new(file, 640, 400);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            encoder
                .write_header()
                .unwrap()
                .write_image_data(&rgba)
                .unwrap();
        }
        rgba
    }
    let before = capture(&gameplay, &mut renderer, "standing");
    for _ in 0..9 {
        game.with_ctx(|_, ctx| {
            gameplay.update(ctx, 0.25);
        });
    }
    let p = &gameplay.town.as_ref().unwrap().placed[id];
    assert_eq!(p.animation.as_ref().unwrap().elapsed, 2_250_000);
    assert_eq!(p.animation.as_ref().unwrap().sample_time, 2_000_000);
    assert!(p.alive);
    let falling = gameplay
        .view
        .as_ref()
        .unwrap()
        .models
        .as_ref()
        .unwrap()
        .groups
        .iter()
        .find(|g| g.name == mesh)
        .unwrap()
        .instances[0];
    assert_ne!(
        standing, falling,
        "each animated tree must receive its own node transform"
    );
    let during = capture(&gameplay, &mut renderer, "falling");
    assert_ne!(before, during);

    gameplay.paused = true;
    game.with_ctx(|_, ctx| {
        gameplay.update(ctx, 1.0);
    });
    assert_eq!(
        gameplay.town.as_ref().unwrap().placed[id]
            .animation
            .as_ref()
            .unwrap()
            .elapsed,
        2_250_000
    );
    gameplay.paused = false;
    // Advance to two ticks before completion: still registered and drawn. Requested f32
    // seconds undergo the original Time constructor's truncation to microsecond ticks.
    game.with_ctx(|_, ctx| {
        gameplay.update(ctx, 2.816666);
    });
    let p = &gameplay.town.as_ref().unwrap().placed[id];
    assert_eq!(p.animation.as_ref().unwrap().elapsed, 5_066_665);
    assert!(p.alive);
    game.with_ctx(|_, ctx| {
        gameplay.update(ctx, 0.000_001_1);
    });
    assert_eq!(
        gameplay.town.as_ref().unwrap().placed[id]
            .animation
            .as_ref()
            .unwrap()
            .elapsed,
        5_066_666
    );
    assert!(gameplay.town.as_ref().unwrap().placed[id].alive);
    game.with_ctx(|_, ctx| {
        gameplay.update(ctx, 0.000_001_1);
    });
    let town = gameplay.town.as_ref().unwrap();
    assert_eq!(
        town.placed[id].animation.as_ref().unwrap().elapsed,
        5_066_667
    );
    assert!(!town.placed[id].alive);
    let index = cell.1 as usize * town.cells.size + cell.0 as usize;
    assert_eq!(town.cells.objects[index], None);
    let terrain = &gameplay.view.as_ref().unwrap().terrain;
    let grid = banished_game::terrain::HeightGrid {
        width: terrain.size + 1,
        heights: &terrain.heights,
    };
    assert_eq!(
        town.cells.types[index],
        grid.natural_cell_type(town.cells.size, index)
    );
    let after = capture(&gameplay, &mut renderer, "removed");
    assert_ne!(during, after);
    eprintln!(
        "tree Remove clip, per-instance pose, pause and exact-duration cleanup verified on GPU"
    );
}

/// Issue #11: a pointer click on a toolbar menu button is a toggle (`Event::Changed`), not a
/// click; the tutorial must count it as pressing the button. The first tutorial then goes from
/// "select the Housing menu" (step 9) to the wood house (10) and to placing it (11).
#[test]
fn the_tutorial_counts_toggled_toolbar_buttons_as_pressed() {
    let Some((mut game, _)) = generated() else {
        return;
    };
    let mut gameplay = None;
    game.with_ctx(|_, ctx| {
        let list = scenarios(ctx.archive);
        let first = list
            .iter()
            .position(|(r, _)| r.contains("GettingStarted"))
            .unwrap();
        let mut g = scenario_game(
            ctx.ui,
            ctx.archive,
            ctx.global.as_ref(),
            &list[first].1,
            first,
        );
        g.load(ctx);
        g.startup(ctx);
        gameplay = Some(g);
    });
    let mut gameplay = gameplay.unwrap();
    game.with_ctx(|_, ctx| {
        gameplay.update(ctx, 1.0 / 30.0);
        for _ in 1..=9 {
            let next = gameplay
                .tutorial
                .as_ref()
                .unwrap()
                .ui
                .clone()
                .unwrap()
                .next
                .unwrap();
            gameplay.event(ctx, Event::Clicked(next));
            gameplay.update(ctx, 1.0 / 30.0);
        }
        assert_eq!(gameplay.tutorial.as_ref().unwrap().step, 9);
        let housing = gameplay
            .toolbar
            .as_ref()
            .unwrap()
            .button(ctx.ui, "housing")
            .unwrap();
        gameplay.event(ctx, Event::Changed(housing));
        for _ in 0..10 {
            gameplay.update(ctx, 1.0 / 30.0);
        }
        assert_eq!(
            gameplay.tutorial.as_ref().unwrap().step,
            10,
            "the Housing toggle was not seen"
        );
        let wood = gameplay
            .toolbar
            .as_ref()
            .unwrap()
            .button(ctx.ui, "woodhouse")
            .unwrap();
        assert!(ctx.ui.node(wood).usable(), "the wood house stays locked");
        gameplay.event(ctx, Event::Changed(wood));
        gameplay.update(ctx, 1.0 / 30.0);
        assert_eq!(gameplay.tutorial.as_ref().unwrap().step, 11);
    });
}
