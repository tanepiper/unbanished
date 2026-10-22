//! The tutorial and the achievements.

use super::*;

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
        let town = gameplay.sim.town.as_ref().unwrap();
        let terrain = format!(r"Game\Terrain\{}.rsc", gameplay.params.terrain);
        let x = crate::achievements::GoalContext {
            town,
            statistics: &gameplay.sim.statistics,
            year: gameplay.sim.calendar.year,
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
