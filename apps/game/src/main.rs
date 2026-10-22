//! Banished, reimplemented. Boots like the original: global data, the Shining Rock splash, the
//! loading screen, then the main menu with its music. New Game enters the generated world with an
//! interactive HUD, calendar, time controls and camera navigation.
//!
//! Native: `cargo run -p unbanished-app --release -- [path/to/WinData]`
//! Web:    `tools/dev-web.sh --app game --release`, then open `/?data=/game-data/`.
//! Captures (headless PNG):
//!   `unbanished --boot-capture OUT.png SECONDS` runs the boot sequence for SECONDS of game time;
//!   `unbanished --capture OUT.png [--seed N --terrain Valleys|Mountains --size 0|1|2 --month M
//!    --view overview|ground]` renders a generated map.
//!
//! On the map: middle drag to rotate, right click to cancel the tool, wheel to zoom,
//! WASD/arrows to move, Escape for the pause menu.

use unbanished_sim::achievements;
mod app;
mod audio;
mod game;
mod gameplay;
mod gfx;
mod global;
mod keymap;
mod loops;
mod menu;
use unbanished_sim::mods;
#[cfg(not(target_arch = "wasm32"))]
mod native;
mod platform;
#[cfg(feature = "probe")]
mod probe;
mod saves;
mod settings;
use unbanished_sim::town as start;
mod tools;
mod ui;
mod view;

use std::rc::Rc;
use std::sync::Arc;

use unbanished_formats::pkg::{Archive, Package};
use unbanished_game::random::Random;
use unbanished_ui::Ui;
use web_time::Instant;

/// The browser's resolution choices: parts of the page's size the scene is drawn at.
#[cfg(target_arch = "wasm32")]
const WEB_SCENE_SCALES: [f32; 3] = [1.0, 0.75, 0.5];
/// The browser's frame-rate limits, in frames per second.
#[cfg(target_arch = "wasm32")]
const WEB_FRAME_RATES: [i32; 4] = [30, 60, 90, 120];
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

use gfx::render::{Frame, Renderer};
use loops::{Ctx, DialogLoaderLoop, LoaderLoop, MainLoop, RawInput, View};

use app::App;
pub use game::Game;
use game::*;

pub enum UserEvent {
    Renderer(Box<Renderer>),
    Packages(Vec<(String, Vec<u8>)>),
    /// Mods (`(file name without .pkm, bytes)`): those stored in the browser when the page
    /// starts, then each one the player adds.
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    Mods(Vec<(String, Vec<u8>)>),
    /// A save to run for `?digest=` (`gameplay::digest`), with its update count and interval.
    #[cfg(target_arch = "wasm32")]
    Digest {
        save: Vec<u8>,
        updates: u32,
        every: u32,
    },
}

const CLEAR: wgpu::Color = wgpu::Color {
    r: 0.0,
    g: 0.0,
    b: 0.0,
    a: 1.0,
};

/// `Engine::MainLoop`'s update step and the most time it catches up (`DAT_180164818`,
/// `DAT_180164820`: 1/60 s and 1/15 s, as ticks).
const FRAME_STEP: i64 = 16_666;
const FRAME_MAX: i64 = 66_666;
// Deviation: the original also draws between steps (`MainLoop::Frame`'s `+0x38 / +0x48`
// handed to `Renderer::Draw`, models and poses blending their `InterpolatedState`); the port
// draws the last step, which differs only on displays faster than 60 Hz.

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let args: Vec<String> = std::env::args().skip(1).collect();
    if matches!(args.first().map(String::as_str), Some("--version" | "-V")) {
        println!("{}", platform::version::NAME_VERSION);
        return;
    }
    log::info!("{}", platform::version::NAME_VERSION);
    if args.iter().any(|a| a == "--vanilla") {
        platform::mode::set_vanilla();
    }
    // `--record FILE [--record-every N]`: record the next game loaded from a save.
    #[cfg(feature = "replay")]
    if let Some(path) = args.windows(2).find(|w| w[0] == "--record").map(|w| &w[1]) {
        let every = args
            .windows(2)
            .find(|w| w[0] == "--record-every")
            .and_then(|w| w[1].parse().ok())
            .unwrap_or(0);
        gameplay::recording::request(path.into(), every);
    }
    match args.first().map(String::as_str) {
        Some("--boot-capture") => {
            let out = args.get(1).expect("--boot-capture OUT.png SECONDS");
            let seconds = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(5.0);
            return native::boot_capture(&args, out, seconds);
        }
        Some("--sim-digest") => return native::sim_digest(&args),
        Some("--sim-continue") => return native::sim_continue(&args),
        #[cfg(feature = "replay")]
        Some("--replay") => return native::replay(&args),
        #[cfg(feature = "multiplayer")]
        Some("--lockstep-test") => return native::lockstep_test(&args),
        #[cfg(feature = "multiplayer")]
        Some("--net-test") => return native::net_test(&args),
        Some("--capture") => {
            let out = args.get(1).expect("--capture OUT.png [OPTIONS]");
            return native::map_capture(&args, out);
        }
        _ => {}
    }
    let packages = native::read_packages(&native::data_dir(&args));
    if packages.is_empty() {
        eprintln!("No .pkg files found. Run tools/extract-assets.sh or pass the WinData folder.");
        std::process::exit(1);
    }
    let event_loop = EventLoop::<UserEvent>::with_user_event()
        .build()
        .expect("event loop");
    let mut app = App::new(event_loop.create_proxy());
    app.pending_packages = Some(packages);
    let dir = native::data_dir(&args);
    app.pending_mods = Some((mods::find(&dir), Some(dir)));
    event_loop.run_app(&mut app).expect("run app");
}

#[cfg(target_arch = "wasm32")]
fn main() {
    console_error_panic_hook::set_once();
    let _ = console_log::init_with_level(log::Level::Info);
    log::info!("{}", platform::version::NAME_VERSION);
    platform::web::show_version(platform::version::NAME_VERSION);
    if platform::web::vanilla_requested() {
        platform::mode::set_vanilla();
    }
    let event_loop = EventLoop::<UserEvent>::with_user_event()
        .build()
        .expect("event loop");
    let proxy = event_loop.create_proxy();
    platform::web::install_file_picker(proxy.clone());
    platform::web::load_from_query(proxy.clone());
    platform::web::load_kept_packages(proxy.clone());
    platform::web::install_save_import();
    platform::web::load_digest_save(proxy.clone());
    // The saves in browser storage (their metadata, for the Save and Load dialogs).
    saves::save_store::init();
    platform::web::load_stored_mods(proxy.clone());
    platform::web::install_drop(proxy.clone());
    use winit::platform::web::EventLoopExtWebSys;
    event_loop.spawn_app(App::new(proxy));
}
