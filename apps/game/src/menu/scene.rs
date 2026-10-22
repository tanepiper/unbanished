//! Loading the main menu's background scene.

use super::*;

/// `FUN_14009fe00`: load `Game/MenuScene/Scene{index}.rsc` and set up its camera and lighting.
pub(super) fn load_menu_scene(ctx: &mut Ctx, index: u32) -> unbanished_formats::Result<SceneView> {
    let reference = format!("Game/MenuScene/Scene{index}.rsc:resource");
    let scene = unbanished_formats::menu_scene::menu_scene(&ctx.archive.read_ref(&reference)?)?;
    let models = crate::gfx::models::SceneModels::from_menu_scene(ctx.archive, &scene);
    let extras = crate::view::scene_assets::SceneExtras::from_map(
        ctx.archive,
        &scene.map,
        scene.sky.as_deref(),
    );
    let map = scene.map;
    let material = map
        .terrain_material
        .clone()
        .unwrap_or_else(|| "Terrain\\Terrain.rsc:resource".into());
    let terrain = Terrain::new(
        ctx.archive,
        &material,
        map.dims[0] as usize,
        map.heights,
        map.surface,
    )?;
    let c = scene.camera;
    let camera = MenuCamera {
        target: c.target.into(),
        base_yaw: c.floats[0],
        pitch: c.floats[1],
        distance: c.floats[2],
        angle: 0.0,
    };
    // Same layout as an EnvironmentMonth: ambient, sun, fog colours; fog start, end, maximum;
    // sun yaw and pitch.
    let mut lighting = Lighting::from_month(&unbanished_formats::game::EnvironmentMonth {
        ambient_color: scene.colors[0],
        sun_color: scene.colors[1],
        fog_color: scene.colors[2],
        fog_start: scene.floats[0],
        fog_end: scene.floats[1],
        fog_max: scene.floats[2],
        sun_yaw: scene.floats[3],
        sun_pitch: scene.floats[4],
    });
    // `pc_userPosition` from the scene (`FUN_1400a0910`): snow, autumn, leaf loss, rain.
    lighting.user = glam::Vec4::from(scene.vector);
    log::info!(
        "menu scene {index}: {} placed meshes in {} groups",
        scene.placements.len(),
        models.groups.len()
    );
    let mut view = SceneView::new(terrain, SceneCamera::Menu(camera), lighting);
    view.models = Some(models);
    view.extras = Some(extras);
    Ok(view)
}
