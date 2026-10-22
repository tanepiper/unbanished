//! What the town draws: start structures, decals, particles, instances, a tree's last clip.

use super::*;

/// The start structures stamp the map's object (`+8`, `FUN_1400a30f0`) on their
/// footprint, so picking finds the storage yard through its cells.
#[test]
fn start_structures_stamp_their_cells() {
    let Some((_, gameplay)) = generated() else {
        return;
    };
    let town = gameplay.sim.town.as_ref().unwrap();
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
        let town = gameplay.sim.town.as_mut().unwrap();
        let n = town.cells.size as i32;
        let (x, y) = (town.start.0, town.start.1 + 12);
        for dx in 0..6 {
            town.construction.commands.push(
                unbanished_game::work::professions::Command::PlanRoad {
                    cell: y * n + x + dx,
                    kind: 1,
                    ty: 3,
                    material: None,
                },
            );
        }
        gameplay.update(ctx, 1.0 / 30.0);
        let town = gameplay.sim.town.as_ref().unwrap();
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
        let town = gameplay.sim.town.as_mut().unwrap();
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
        let town = gameplay.sim.town.as_mut().unwrap();
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
        let town = gameplay.sim.town.as_mut().unwrap();
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
        let town = gameplay.sim.town.as_ref().unwrap();
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

#[test]
fn speed_loop_smooths_once_and_pause_still_updates_gpu_instances() {
    let Some((mut game, mut gameplay)) = generated() else {
        return;
    };
    let town = gameplay.sim.town.as_mut().expect("generated starting town");
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
    assert_eq!(gameplay.sim.calendar.elapsed, 5_000_000);
    let town = gameplay.sim.town.as_ref().unwrap();
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
    assert_eq!(gameplay.sim.calendar.elapsed, 5_000_000);
    let town = gameplay.sim.town.as_ref().unwrap();
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
    let town = gameplay.sim.town.as_mut().unwrap();
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
        if gameplay.sim.town.as_ref().unwrap().placed[id]
            .animation
            .is_some()
        {
            break;
        }
    }
    let p = &gameplay.sim.town.as_ref().unwrap().placed[id];
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
            terrain: Some(view.frame(renderer.aspect(), gameplay.sim.calendar.seconds())),
            clear: crate::CLEAR,
            ui: None,
        });
        if let Ok(directory) = unbanished_formats::env::var("CAPTURE_TREE_EXPIRY") {
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
    let p = &gameplay.sim.town.as_ref().unwrap().placed[id];
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
        gameplay.sim.town.as_ref().unwrap().placed[id]
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
    let p = &gameplay.sim.town.as_ref().unwrap().placed[id];
    assert_eq!(p.animation.as_ref().unwrap().elapsed, 5_066_665);
    assert!(p.alive);
    game.with_ctx(|_, ctx| {
        gameplay.update(ctx, 0.000_001_1);
    });
    assert_eq!(
        gameplay.sim.town.as_ref().unwrap().placed[id]
            .animation
            .as_ref()
            .unwrap()
            .elapsed,
        5_066_666
    );
    assert!(gameplay.sim.town.as_ref().unwrap().placed[id].alive);
    game.with_ctx(|_, ctx| {
        gameplay.update(ctx, 0.000_001_1);
    });
    let town = gameplay.sim.town.as_ref().unwrap();
    assert_eq!(
        town.placed[id].animation.as_ref().unwrap().elapsed,
        5_066_667
    );
    assert!(!town.placed[id].alive);
    let index = cell.1 as usize * town.cells.size + cell.0 as usize;
    assert_eq!(town.cells.objects[index], None);
    let terrain = &gameplay.view.as_ref().unwrap().terrain;
    let grid = unbanished_game::terrain::HeightGrid {
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
