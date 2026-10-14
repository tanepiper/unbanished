//! Banished, reimplemented: generates a New Game map from the player's own game data with the
//! bit-exact world generator and draws it with the game's terrain shaders and lighting.
//!
//! Native:  `cargo run -p banished-app --release -- [OPTIONS] [path/to/WinData]`
//! Capture: `banished --capture OUT.png [OPTIONS]` renders one frame headlessly.
//! Options: `--seed N`, `--terrain Valleys|Mountains`, `--size 0|1|2` (Small/Medium/Large),
//!          `--month 0..11`, `--view overview|ground`.
//!
//! Controls: drag to rotate, wheel to zoom, WASD/arrows to move, `[` / `]` to change month,
//! N for a new random seed.

mod render;
mod world;

use std::sync::Arc;

use banished_formats::pkg::{Archive, Package};
use glam::{Mat4, Vec2, Vec3};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

use render::{Frame, Lighting, Renderer};
use world::{World, WorldParams};

/// Top-down strategy camera over the terrain (z up).
#[derive(Clone, Copy)]
struct Camera {
    target: Vec2,
    yaw: f32,
    pitch: f32,
    distance: f32,
}

impl Camera {
    fn overview(world: &World) -> Camera {
        Camera {
            target: Vec2::ZERO,
            yaw: 0.0,
            pitch: 60f32.to_radians(),
            distance: world.size as f32 * 1.1,
        }
    }

    fn ground() -> Camera {
        Camera {
            target: Vec2::ZERO,
            yaw: 0.5,
            pitch: 40f32.to_radians(),
            distance: 60.0,
        }
    }

    fn focus(&self, world: &World) -> Vec3 {
        self.target
            .extend(world.height_at(self.target.x, self.target.y).max(0.0))
    }

    fn eye(&self, world: &World) -> Vec3 {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        self.focus(world) + Vec3::new(-sy * cp, -cy * cp, sp) * self.distance
    }

    fn view_proj(&self, world: &World, aspect: f32) -> Mat4 {
        use glam::camera::rh::{proj::directx::perspective, view::look_at_mat4};
        let proj = perspective(45f32.to_radians(), aspect, 0.5, self.distance * 4.0 + 100.0);
        proj * look_at_mat4(self.eye(world), self.focus(world), Vec3::Z)
    }

    fn pan(&mut self, forward: f32, right: f32, world: &World) {
        let (sy, cy) = self.yaw.sin_cos();
        let step = self.distance * 0.02;
        self.target += Vec2::new(sy, cy) * forward * step + Vec2::new(cy, -sy) * right * step;
        let half = world.size as f32 / 2.0;
        self.target = self.target.clamp(Vec2::splat(-half), Vec2::splat(half));
    }
}

struct Options {
    params: WorldParams,
    month: usize,
    overview: bool,
    capture: Option<String>,
    data: std::path::PathBuf,
}

fn parse_args() -> Options {
    let mut o = Options {
        params: WorldParams::default(),
        month: 3,
        overview: false,
        capture: None,
        data: std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../game-data/original/WinData"),
    };
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        let value = || args.get(i + 1).cloned().unwrap_or_default();
        match args[i].as_str() {
            "--seed" => o.params.seed = value().parse().expect("--seed N"),
            "--terrain" => o.params.terrain = value(),
            "--size" => o.params.size = value().parse().expect("--size 0|1|2"),
            "--month" => o.month = value().parse::<usize>().expect("--month 0..11") % 12,
            "--view" => o.overview = value() == "overview",
            "--capture" => o.capture = Some(value()),
            path => {
                o.data = path.into();
                i += 1;
                continue;
            }
        }
        i += 2;
    }
    o
}

fn load_archive(dir: &std::path::Path) -> Archive {
    let mut archive = Archive::default();
    for name in ["data1.pkg", "patch1.pkg", "patch0.pkg", "data0.pkg"] {
        let path = dir.join(name);
        match std::fs::read(&path) {
            Ok(bytes) => archive.add(Package::parse(bytes).expect("parse package")),
            Err(e) => log::warn!("{}: {e}", path.display()),
        }
    }
    if archive.is_empty() {
        panic!(
            "No .pkg files in {}. Run tools/extract-assets.sh or pass the WinData folder.",
            dir.display()
        );
    }
    archive
}

const CLEAR: wgpu::Color = wgpu::Color {
    r: 0.75,
    g: 0.75,
    b: 0.75,
    a: 1.0,
};

fn lighting(world: &World, month: usize, overview: bool) -> Lighting {
    let mut l = Lighting::from_month(&world.environment.months[month]);
    if overview {
        // Fog distances are tuned for the in-game camera; push them out for whole-map views.
        l.fog_start = 1e5;
        l.fog_end = 2e5;
    }
    l
}

fn capture(o: Options, out: &str) {
    let archive = load_archive(&o.data);
    let world = World::generate(&archive, &o.params).expect("generate world");
    let (w, h) = (1280, 800);
    let mut renderer = pollster::block_on(Renderer::headless(w, h));
    renderer.set_world(&world);
    let camera = if o.overview {
        Camera::overview(&world)
    } else {
        Camera::ground()
    };
    let rgba = renderer.capture(&Frame {
        view_proj: camera.view_proj(&world, renderer.aspect()),
        lighting: lighting(&world, o.month, o.overview),
        clear: CLEAR,
    });
    let file = std::fs::File::create(out).expect("create png");
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()
        .and_then(|mut wr| wr.write_image_data(&rgba))
        .expect("write png");
    log::info!("wrote {out}");
}

struct App {
    options: Options,
    archive: Archive,
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    world: Option<World>,
    camera: Camera,
    drag: Option<(f64, f64)>,
    cursor: (f64, f64),
}

impl App {
    fn regenerate(&mut self) {
        match World::generate(&self.archive, &self.options.params) {
            Ok(world) => {
                if let Some(r) = &mut self.renderer {
                    r.set_world(&world);
                }
                self.camera = if self.options.overview {
                    Camera::overview(&world)
                } else {
                    Camera::ground()
                };
                if let Some(w) = &self.window {
                    w.set_title(&format!(
                        "Banished — {} {} seed {} (month {})",
                        world.profile.name,
                        ["Small", "Medium", "Large"][self.options.params.size.min(2)],
                        self.options.params.seed,
                        self.options.month + 1
                    ));
                }
                self.world = Some(world);
            }
            Err(e) => log::error!("world generation failed: {e}"),
        }
    }

    fn redraw(&self) {
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attrs = Window::default_attributes()
            .with_title("Banished")
            .with_inner_size(winit::dpi::LogicalSize::new(1400.0, 860.0));
        let window = Arc::new(event_loop.create_window(attrs).expect("create window"));
        let mut renderer = pollster::block_on(Renderer::new(window.clone()));
        let size = window.inner_size();
        renderer.resize(size.width, size.height);
        log::info!("GPU: {}", renderer.backend);
        self.window = Some(window);
        self.renderer = Some(renderer);
        self.regenerate();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(s) => {
                if let Some(r) = &mut self.renderer {
                    r.resize(s.width, s.height);
                }
            }
            WindowEvent::RedrawRequested => {
                if let (Some(r), Some(world)) = (&mut self.renderer, &self.world) {
                    let frame = Frame {
                        view_proj: self.camera.view_proj(world, r.aspect()),
                        lighting: lighting(world, self.options.month, false),
                        clear: CLEAR,
                    };
                    r.render(&frame);
                }
                return;
            }
            WindowEvent::MouseInput {
                button: MouseButton::Left | MouseButton::Right,
                state,
                ..
            } => {
                self.drag = (state == ElementState::Pressed).then_some(self.cursor);
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = (position.x, position.y);
                if let Some((x, y)) = self.drag {
                    self.camera.yaw += (position.x - x) as f32 * 0.005;
                    self.camera.pitch = (self.camera.pitch + (position.y - y) as f32 * 0.005)
                        .clamp(10f32.to_radians(), 89f32.to_radians());
                    self.drag = Some((position.x, position.y));
                } else {
                    return;
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let y = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32 / 40.0,
                };
                self.camera.distance = (self.camera.distance * 0.9f32.powf(y)).clamp(5.0, 2000.0);
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                let Some(world) = &self.world else { return };
                match event.logical_key.as_ref() {
                    Key::Named(NamedKey::ArrowUp) | Key::Character("w") => {
                        self.camera.pan(1.0, 0.0, world)
                    }
                    Key::Named(NamedKey::ArrowDown) | Key::Character("s") => {
                        self.camera.pan(-1.0, 0.0, world)
                    }
                    Key::Named(NamedKey::ArrowLeft) | Key::Character("a") => {
                        self.camera.pan(0.0, -1.0, world)
                    }
                    Key::Named(NamedKey::ArrowRight) | Key::Character("d") => {
                        self.camera.pan(0.0, 1.0, world)
                    }
                    Key::Character("]") => self.options.month = (self.options.month + 1) % 12,
                    Key::Character("[") => self.options.month = (self.options.month + 11) % 12,
                    Key::Character("n") => {
                        self.options.params.seed = self
                            .options
                            .params
                            .seed
                            .wrapping_mul(1_103_515_245)
                            .wrapping_add(12345);
                        self.regenerate();
                    }
                    _ => return,
                }
            }
            _ => return,
        }
        self.redraw();
    }
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let options = parse_args();
    if let Some(out) = options.capture.clone() {
        return capture(options, &out);
    }
    let archive = load_archive(&options.data);
    let event_loop = EventLoop::new().expect("event loop");
    let mut app = App {
        options,
        archive,
        window: None,
        renderer: None,
        world: None,
        camera: Camera::ground(),
        drag: None,
        cursor: (0.0, 0.0),
    };
    event_loop.run_app(&mut app).expect("run app");
}
