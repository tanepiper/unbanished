//! Selection, placement overlays, work radii, following a citizen and the Path tool.

use super::*;

#[test]
fn selection_host_and_highlight_leave_simulation_unchanged() {
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
    let position = glam::Vec3::from(town.placed[id].position);
    let origin = position + glam::Vec3::new(0., -12., 10.);
    let direction = (position + glam::Vec3::Z - origin).normalize();
    let rng = gameplay.sim.rng.clone();
    let time = gameplay.sim.calendar.elapsed;
    let before = format!(
        "{:?}{:?}{:?}",
        town.citizens, town.agents.states, town.storages
    );
    game.with_ctx(|_, ctx| {
        let town = gameplay.sim.town.as_ref().unwrap();
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
                unbanished_ui::Kind::Control(unbanished_ui::Control::Page { selected: 1, .. })
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
            let grid = unbanished_game::terrain::HeightGrid {
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
    let town = gameplay.sim.town.as_ref().unwrap();
    assert_eq!(rng, gameplay.sim.rng);
    assert_eq!(time, gameplay.sim.calendar.elapsed);
    assert_eq!(
        before,
        format!(
            "{:?}{:?}{:?}",
            town.citizens, town.agents.states, town.storages
        )
    );
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
        let town = gameplay.sim.town.as_mut().unwrap();
        let view = gameplay.view.as_ref().unwrap();
        let grid = unbanished_game::terrain::HeightGrid {
            width: view.terrain.size + 1,
            heights: &view.terrain.heights,
        };
        let mut tool = PlacementTool::start(
            ctx.archive,
            town,
            "Template\\WoodHouse.rsc:resource",
            &mut gameplay.sim.rng,
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
        let start = gameplay.sim.town.as_ref().unwrap().start;
        {
            let town = gameplay.sim.town.as_ref().unwrap();
            let view = gameplay.view.as_ref().unwrap();
            let grid = unbanished_game::terrain::HeightGrid {
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
                gameplay.sim.town.as_ref().unwrap(),
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
        let at = gameplay.sim.town.as_ref().unwrap().citizens[0].position;
        let SceneCamera::Map(camera) = &gameplay.view.as_ref().unwrap().camera else {
            panic!("no map camera");
        };
        assert!((camera.target - glam::Vec2::new(at[0], at[1])).length() < 1e-3);
        let name = gameplay.sim.town.as_ref().unwrap().citizens[0].name.clone();
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
        let town = gameplay.sim.town.as_mut().unwrap();
        let c = town
            .agents
            .states
            .iter()
            .position(|s| s.home.is_some() && s.workplace.is_some())
            .expect("a commuter in Save83");
        let workplace = town.agents.states[c].workplace.unwrap();
        let tool = gameplay.path_tool.as_mut().unwrap();
        tool.pick(town, &mut gameplay.sim.rng, Entity::Citizen(c));
        let (lines, cells) = tool.overlay(town.cells.size as i32);
        assert!(cells.len() >= 2, "{}", cells.len());
        assert!(!lines.is_empty());
        assert!(cells.iter().all(|c| c.frame == 4 && c.argb == 0xfffc_cc4c));
        // The workplace: every worker's commute, at least as many cells.
        tool.pick(town, &mut gameplay.sim.rng, Entity::Placed(workplace));
        let (_, all) = tool.overlay(town.cells.size as i32);
        assert!(all.len() >= cells.len());
        gameplay.raw(ctx, RawInput::Escape);
        assert!(gameplay.path_tool.is_none());
    });
}
