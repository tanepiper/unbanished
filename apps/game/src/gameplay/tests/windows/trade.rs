//! The trade and limits windows.

use super::*;

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
