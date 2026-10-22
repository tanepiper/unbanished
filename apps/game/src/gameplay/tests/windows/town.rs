//! The town hall, crop and destroy windows.

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
