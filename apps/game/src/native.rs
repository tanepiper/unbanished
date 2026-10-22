//! The commands that run without a window (native only): headless captures and the probe
//! (`--boot-capture`), map pictures (`--capture`), `--sim-digest`, `--replay`, and the lockstep
//! and network tests.

mod boot;
mod continue_save;
#[cfg(feature = "multiplayer")]
mod network;
pub use boot::boot_capture;
pub use continue_save::sim_continue;
#[cfg(feature = "multiplayer")]
pub use network::{lockstep_test, net_test};

use super::*;

pub fn data_dir(args: &[String]) -> std::path::PathBuf {
    crate::platform::paths::data_dir(args)
}

pub fn read_packages(dir: &std::path::Path) -> Vec<(String, Vec<u8>)> {
    ["data1.pkg", "patch1.pkg", "patch0.pkg", "data0.pkg"]
        .iter()
        .filter_map(|name| match std::fs::read(dir.join(name)) {
            Ok(bytes) => Some((name.to_string(), bytes)),
            Err(e) => {
                log::warn!("{}: {e}", dir.join(name).display());
                None
            }
        })
        .collect()
}

pub fn write_png(path: &str, (w, h): (u32, u32), rgba: &[u8]) {
    let file = std::fs::File::create(path).expect("create png");
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()
        .and_then(|mut wr| wr.write_image_data(rgba))
        .expect("write png");
    log::info!("wrote {path}");
}

/// `--sim-digest SAVE [UPDATES] [EVERY]`: runs a save without a window at speed 10 and
/// prints a digest of the game state every EVERY updates (`gameplay::digest`), to compare
/// with the browser build's `?digest=`.
pub fn sim_digest(args: &[String]) {
    let save = args.get(1).expect("--sim-digest SAVE [UPDATES] [EVERY]");
    let updates = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(1200);
    let every = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(60);
    let bytes = std::fs::read(save).expect("read the save");
    let dir = data_dir(args);
    let found = crate::mods::find(&dir);
    let mut game = Game::with_mods(
        read_packages(&dir),
        found,
        Some(dir),
        crate::settings::Settings::default(),
    )
    .expect("game data");
    game.audio.set_master_volume(0.0);
    let start = std::time::Instant::now();
    match crate::gameplay::digest::run(&mut game, &bytes, updates, every) {
        Ok(digests) => {
            for d in digests {
                println!("{d}");
            }
            log::info!(
                "{updates} updates in {:.1} s",
                start.elapsed().as_secs_f32()
            );
        }
        Err(e) => {
            eprintln!("--sim-digest: {e}");
            std::process::exit(1);
        }
    }
}

/// `--replay SAVE RECORDING [UPDATES] [EVERY]`: replays a recording (`--record`) from its
/// save without a window and compares the game state with the recorded digests
/// (`gameplay::recording`). Exits with 1 when they differ.
#[cfg(feature = "replay")]
pub fn replay(args: &[String]) {
    let (Some(save), Some(recording)) = (args.get(1), args.get(2)) else {
        eprintln!("--replay SAVE RECORDING [UPDATES] [EVERY]");
        std::process::exit(2);
    };
    let updates = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(0);
    let every = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(0);
    let bytes = std::fs::read(save).expect("read the save");
    let text = std::fs::read_to_string(recording).expect("read the recording");
    let dir = data_dir(args);
    let found = crate::mods::find(&dir);
    let mut game = Game::with_mods(
        read_packages(&dir),
        found,
        Some(dir),
        crate::settings::Settings::default(),
    )
    .expect("game data");
    game.audio.set_master_volume(0.0);
    let start = std::time::Instant::now();
    match crate::gameplay::recording::replay(&mut game, &bytes, &text, updates, every) {
        Ok(r) => {
            for d in &r.digests {
                println!("{d}");
            }
            log::info!("replayed in {:.1} s", start.elapsed().as_secs_f32());
            match r.mismatch {
                Some((recorded, replayed)) => {
                    println!("DIFFERENT from the recording");
                    println!("  recorded {recorded}");
                    println!("  replayed {replayed}");
                    std::process::exit(1);
                }
                None => println!("identical: {} recorded digests matched", r.compared),
            }
        }
        Err(e) => {
            eprintln!("--replay: {e}");
            std::process::exit(2);
        }
    }
}

/// The generated-map capture from M5 (`--capture`).
pub fn map_capture(args: &[String], out: &str) {
    use crate::view::scene::{Camera, SceneCamera, SceneView};
    use crate::view::world::{World, WorldParams};
    let mut params = WorldParams::default();
    let (mut month, mut overview) = (3usize, false);
    let value = |flag: &str| {
        args.iter()
            .position(|a| a == flag)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    if let Some(v) = value("--seed") {
        params.seed = v.parse().expect("--seed N");
    }
    if let Some(v) = value("--terrain") {
        params.terrain = v;
    }
    if let Some(v) = value("--size") {
        params.size = v.parse().expect("--size 0|1|2");
    }
    if let Some(v) = value("--month") {
        month = v.parse::<usize>().expect("--month 0..11") % 12;
    }
    if let Some(v) = value("--view") {
        overview = v == "overview";
    }
    let mut archive = Archive::default();
    for (_, bytes) in read_packages(&data_dir(args)) {
        archive.add(Package::parse(bytes).expect("parse package"));
    }
    let world = World::generate(&archive, &params).expect("generate world");
    let mut lighting = gfx::render::Lighting::from_month(&world.environment.months[month]);
    let camera = if overview {
        // Fog distances are tuned for the in-game camera; push them out for whole-map views.
        lighting.fog_start = 1e5;
        lighting.fog_end = 2e5;
        Camera::overview(&world.terrain)
    } else {
        Camera::ground()
    };
    let size = (1280, 800);
    let mut renderer = pollster::block_on(Renderer::headless(size.0, size.1));
    let view = SceneView::new(world.terrain, SceneCamera::Map(camera), lighting);
    renderer.set_scene(&view);
    let rgba = renderer.capture(&Frame {
        terrain: Some(view.frame(renderer.aspect(), 0.0)),
        clear: wgpu::Color {
            r: 0.75,
            g: 0.75,
            b: 0.75,
            a: 1.0,
        },
        ui: None,
    });
    write_png(out, size, &rgba);
}
