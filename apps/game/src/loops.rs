//! The engine's loop model (`Engine::MainLoop`, `LoaderLoop`, `DialogLoaderLoopT`).
//!
//! The original runs one loop at a time. A loader loop shows its dialog while a background thread
//! loads the next loop, waits for a minimum display time, fades to black and hands over. The next
//! loop fades in from black. Here loading runs on the main thread between frames so the same code
//! works in the browser; timings and fades follow the original:
//! `UI::UserInterface::SetFullscreenFade` / `UpdateFade` move a black overlay at 4 units per second.

use std::rc::Rc;

use banished_formats::pkg::Archive;
use banished_formats::ui::flags;
use banished_game::random::Random;
use banished_ui::{DialogInstance, Event, Kind, Node, NodeId, Quad, Ui};

use crate::global::GlobalData;

/// Everything a loop may touch.
pub struct Ctx<'a> {
    pub archive: &'a Rc<Archive>,
    pub ui: &'a mut Ui,
    /// The shared 64-channel mixer (`Audio::Device`).
    pub audio: &'a mut crate::sound::Audio,
    pub global: &'a mut Option<GlobalData>,
    /// The engine's default `System::Random()`, seeded from the clock.
    pub rng: &'a mut Random,
    /// `Application::IsPaused`: the window does not have the focus.
    pub app_paused: bool,
}

/// Input not consumed by the UI (camera control).
pub enum RawInput {
    /// Cursor position in window pixels (always delivered, also over the UI).
    Cursor(i32, i32),
    /// Left button pressed over the world (not captured by the UI): `MouseLeft` (action 5).
    Click,
    /// Left button released (anywhere): `KeyMap::Released(MouseLeft)`.
    Release,
    /// A key-map action by its default key: `r` StructureLeft (0x13), `t` StructureRight (0x14),
    /// `f` StructureType (0x15).
    Key(char),
    Drag(f32, f32),
    PanDrag(f32, f32),
    Wheel(f32),
    Pan(f32, f32),
    Escape,
}

pub enum Next {
    Stay,
    /// Fade out, then make this loop current.
    Switch(Box<dyn GameLoop>),
    Quit,
}

/// 3D content a loop wants drawn behind its UI.
pub enum View<'a> {
    None,
    World(&'a crate::scene::SceneView),
}

pub trait GameLoop {
    fn name(&self) -> &'static str;
    /// Background loading, run while the previous loop is on screen (`Loader::Load`).
    fn load(&mut self, _ctx: &mut Ctx) {}
    /// Becoming the current loop (`Startup`): build the UI, start music.
    fn startup(&mut self, ctx: &mut Ctx);
    fn shutdown(&mut self, _ctx: &mut Ctx) {}
    fn update(&mut self, ctx: &mut Ctx, dt: f32) -> Next;
    fn event(&mut self, _ctx: &mut Ctx, _event: Event) -> Next {
        Next::Stay
    }
    fn raw(&mut self, _ctx: &mut Ctx, _input: RawInput) -> Next {
        Next::Stay
    }
    fn screen(&self) -> Option<&Screen>;
    /// A scripted capture command (`--event`); loops that do not take it return false.
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    fn script(&mut self, _ctx: &mut Ctx, _command: &str) -> bool {
        false
    }
    fn view(&self) -> View<'_> {
        View::None
    }
}

/// A loop's UI: a full-screen group holding an optional backdrop and dialog instances.
pub struct Screen {
    pub root: NodeId,
    backdrop: NodeId,
    /// The dialog whose background sprite is the backdrop.
    backdrop_owner: Option<NodeId>,
    dialog_host: NodeId,
    pub dialogs: Vec<DialogInstance>,
    /// A dialog that takes all input while open (e.g. New Game over the main menu).
    modal: Option<usize>,
    /// Draw-list position where this frame's added dialogs go. `UserInterface::AddDialog`
    /// queues dialogs and `UserInterface::Update` appends the queue in reverse, so dialogs
    /// added in the same frame draw in reverse order (the toolbar over the event ribbon).
    batch: std::cell::Cell<usize>,
}

impl Screen {
    pub fn new(ui: &mut Ui) -> Screen {
        let root = ui.group();
        let backdrop = ui.group();
        let dialog_host = ui.group();
        ui.add_child(root, backdrop);
        ui.add_child(root, dialog_host);
        Screen {
            root,
            backdrop,
            backdrop_owner: None,
            dialog_host,
            dialogs: Vec::new(),
            modal: None,
            batch: std::cell::Cell::new(0),
        }
    }

    /// Add a dialog (`UserInterface::AddDialog`); its background sprite becomes the backdrop.
    pub fn add_dialog(&mut self, ui: &mut Ui, reference: &str) -> Option<usize> {
        let Some(d) = ui.instantiate_dialog(reference) else {
            log::error!("cannot load dialog {reference}");
            return None;
        };
        if let Some(sprite) = d.background {
            let image = ui.add(Node::new(Kind::Image {
                sprite: Some(sprite),
                size: [-1, -1],
            }));
            ui.node_mut(image).flags = flags::FILL_WIDTH | flags::FILL_HEIGHT;
            ui.node_mut(self.backdrop).children = vec![image];
            self.backdrop_owner = Some(d.root);
        }
        let host = &mut ui.node_mut(self.dialog_host).children;
        let at = self.batch.get().min(host.len());
        host.insert(at, d.root);
        self.dialogs.push(d);
        Some(self.dialogs.len() - 1)
    }

    /// Add a dialog that receives all input until it is removed.
    pub fn add_modal(&mut self, ui: &mut Ui, reference: &str) -> Option<usize> {
        let index = self.add_dialog(ui, reference)?;
        self.modal = Some(index);
        // Drop hover and tooltips of the controls now covered.
        ui.clear_input();
        Some(index)
    }

    /// Start a frame: dialogs added from now on go after every dialog already shown.
    pub fn begin_frame(&self, ui: &Ui) {
        self.batch.set(ui.node(self.dialog_host).children.len());
    }

    pub fn remove_dialog(&mut self, ui: &mut Ui, index: usize) {
        let d = self.dialogs.remove(index);
        // The background is drawn with its dialog (the in-game pause menu's vignette).
        if self.backdrop_owner == Some(d.root) {
            ui.node_mut(self.backdrop).children.clear();
            self.backdrop_owner = None;
        }
        let host = &mut ui.node_mut(self.dialog_host).children;
        if let Some(p) = host.iter().position(|&c| c == d.root) {
            host.remove(p);
            if p < self.batch.get() {
                self.batch.set(self.batch.get() - 1);
            }
        }
        self.modal = match self.modal {
            Some(m) if m == index => None,
            Some(m) if m > index => Some(m - 1),
            m => m,
        };
    }

    /// Remove the dialog whose root element is `root` (indices of later dialogs shift down).
    pub fn remove_root(&mut self, ui: &mut Ui, root: NodeId) {
        if let Some(i) = self.dialogs.iter().position(|d| d.root == root) {
            self.remove_dialog(ui, i);
        }
    }

    pub fn find(&self, ui: &Ui, dialog: usize, name: &str) -> Option<NodeId> {
        ui.find(self.dialogs.get(dialog)?.root, name)
    }

    /// Full-screen layout; dialogs are placed by their own alignment (`DialogInstance::ComputeLayout`).
    pub fn layout(&self, ui: &mut Ui, size: (u32, u32)) {
        let rect = [0, 0, size.0 as i32, size.1 as i32];
        ui.screen = [rect[2], rect[3]];
        let dialogs = std::mem::take(&mut ui.node_mut(self.dialog_host).children);
        ui.layout(self.root, rect);
        for &d in &dialogs {
            ui.layout_dialog(d, rect);
        }
        ui.node_mut(self.dialog_host).children = dialogs;
    }

    /// Input goes to the modal dialog if one is open, otherwise to the whole screen.
    pub fn input_root(&self) -> NodeId {
        self.modal
            .and_then(|m| self.dialogs.get(m))
            .map_or(self.root, |d| d.root)
    }
}

/// Fullscreen fade (`UserInterface` `+0xf0` direction, `+0xf4` level, `+0xf8` colour).
pub struct Fade {
    pub level: f32,
    direction: f32,
}

const FADE_RATE: f32 = 4.0;

impl Fade {
    pub fn black() -> Fade {
        Fade {
            level: 1.0,
            direction: 0.0,
        }
    }
    pub fn fade_in(&mut self) {
        self.direction = -1.0;
    }
    pub fn fade_out(&mut self) {
        self.direction = 1.0;
    }
    pub fn busy(&self) -> bool {
        self.direction != 0.0
    }
    /// `UpdateFade`.
    pub fn update(&mut self, dt: f32) {
        if self.direction == 0.0 {
            return;
        }
        self.level += dt * self.direction * FADE_RATE;
        if self.level >= 1.0 {
            self.level = 1.0;
            self.direction = 0.0;
        } else if self.level <= 0.0 {
            self.level = 0.0;
            self.direction = 0.0;
        }
    }
}

/// Texture id of the 1×1 white texture the renderer registers for fades.
pub const WHITE: u32 = 0;
/// Texture ids of MapUI's images: the terrain render and the entity overlay.
pub const MINIMAP: u32 = 1;
pub const MINIMAP_OVERLAY: u32 = 2;

pub fn fade_quad(level: f32, size: (u32, u32)) -> Option<Quad> {
    (level > 0.0).then_some(Quad {
        texture: banished_ui::Texture::External(WHITE),
        rect: [0.0, 0.0, size.0 as f32, size.1 as f32],
        uv: [0.0, 0.0, 1.0, 1.0],
        color: [0, 0, 0, (level * 255.0) as u8],
        clip: None,
    })
}

/// Runs the current loop and the fades between loops (`Engine::MainLoop::Run`).
pub struct MainLoop {
    current: Box<dyn GameLoop>,
    pending: Option<Box<dyn GameLoop>>,
    started: bool,
    pub fade: Fade,
    pub quit: bool,
    /// Seconds since the current loop started, for UI animations.
    pub elapsed: f32,
}

impl MainLoop {
    pub fn new(first: Box<dyn GameLoop>) -> MainLoop {
        MainLoop {
            current: first,
            pending: None,
            started: false,
            fade: Fade::black(),
            quit: false,
            elapsed: 0.0,
        }
    }

    pub fn current(&self) -> &dyn GameLoop {
        self.current.as_ref()
    }

    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    pub fn current_mut(&mut self) -> &mut dyn GameLoop {
        self.current.as_mut()
    }

    fn apply(&mut self, next: Next) {
        match next {
            Next::Stay => {}
            Next::Quit => self.quit = true,
            Next::Switch(n) => {
                log::info!("{} -> {}", self.current.name(), n.name());
                self.pending = Some(n);
                self.fade.fade_out();
            }
        }
    }

    pub fn update(&mut self, ctx: &mut Ctx, dt: f32) {
        if !self.started {
            self.started = true;
            self.current.startup(ctx);
            self.fade = Fade::black();
            self.fade.fade_in();
        }
        self.fade.update(dt);
        self.elapsed += dt;
        ctx.ui.update_animations(self.elapsed);
        if self.pending.is_some() {
            if !self.fade.busy() {
                self.current.shutdown(ctx);
                self.current = self.pending.take().unwrap();
                self.elapsed = 0.0;
                self.current.startup(ctx);
                self.fade = Fade::black();
                self.fade.fade_in();
            }
            return;
        }
        // `UserInterface::Update`: queued dialogs join the draw list, then every dialog's
        // layout is recomputed before the loop runs.
        if let Some(screen) = self.current.screen() {
            screen.begin_frame(ctx.ui);
            if ctx.ui.screen[0] > 0 && ctx.ui.screen[1] > 0 {
                let size = (ctx.ui.screen[0] as u32, ctx.ui.screen[1] as u32);
                screen.layout(ctx.ui, size);
            }
        }
        let next = self.current.update(ctx, dt);
        self.apply(next);
        // `Audio::Device::Update`: free the channels that finished.
        ctx.audio.frame();
    }

    pub fn raw(&mut self, ctx: &mut Ctx, input: RawInput) {
        if self.pending.is_none() {
            let next = self.current.raw(ctx, input);
            self.apply(next);
        }
    }

    /// Deliver one UI event (pointer clicks are returned rather than queued by `banished_ui`).
    pub fn inject(&mut self, ctx: &mut Ctx, event: Event) {
        if self.pending.is_none() {
            let next = self.current.event(ctx, event);
            self.apply(next);
        }
    }

    /// Deliver queued UI events to the current loop (ignored while switching).
    pub fn events(&mut self, ctx: &mut Ctx) {
        for event in ctx.ui.drain_events() {
            if self.pending.is_some() {
                continue;
            }
            let next = self.current.event(ctx, event);
            self.apply(next);
        }
    }
}

type LoadWork = Box<dyn FnOnce(&mut Ctx)>;

/// `Engine::LoaderLoop` without a dialog: loads the next loop, then hands over at once.
pub struct LoaderLoop {
    name: &'static str,
    work: Option<LoadWork>,
    next: Option<Box<dyn GameLoop>>,
}

impl LoaderLoop {
    pub fn new(
        name: &'static str,
        work: impl FnOnce(&mut Ctx) + 'static,
        next: Box<dyn GameLoop>,
    ) -> LoaderLoop {
        LoaderLoop {
            name,
            work: Some(Box::new(work)),
            next: Some(next),
        }
    }
}

impl GameLoop for LoaderLoop {
    fn name(&self) -> &'static str {
        self.name
    }
    fn startup(&mut self, _ctx: &mut Ctx) {}
    fn update(&mut self, ctx: &mut Ctx, _dt: f32) -> Next {
        if let Some(work) = self.work.take() {
            work(ctx);
        }
        let mut next = self.next.take().expect("loader loop runs once");
        next.load(ctx);
        Next::Switch(next)
    }
    fn screen(&self) -> Option<&Screen> {
        None
    }
}

/// `Engine::DialogLoaderLoopT` with the game's splash/loading behaviour (`FUN_14009e470`,
/// `FUN_14009e270`): show a dialog for at least `min_seconds`, load the next loop meanwhile, and
/// for dialogs with a `descText` element show a random `loadingDetails` line every 2 seconds.
pub struct DialogLoaderLoop {
    dialog: String,
    min_seconds: f32,
    elapsed: f32,
    desc_timer: f32,
    frames: u32,
    screen: Option<Screen>,
    next: Option<Box<dyn GameLoop>>,
    loaded: bool,
}

impl DialogLoaderLoop {
    pub fn new(dialog: String, min_seconds: f32, next: Box<dyn GameLoop>) -> DialogLoaderLoop {
        DialogLoaderLoop {
            dialog,
            min_seconds,
            elapsed: 0.0,
            desc_timer: 0.0,
            frames: 0,
            screen: None,
            next: Some(next),
            loaded: false,
        }
    }

    fn rotate_description(&mut self, ctx: &mut Ctx) {
        let Some(screen) = &self.screen else { return };
        let Some(desc) = screen.find(ctx.ui, 0, "descText") else {
            return;
        };
        let lines = loading_details(ctx.ui);
        if lines.is_empty() {
            return;
        }
        // `Rand(count - 1) + 1`: entry 0 is never shown.
        let i = ctx.rng.rand_below(lines.len() as u32 - 1) as usize + 1;
        ctx.ui.set_text(desc, &lines[i]);
    }
}

/// Lines of `StringTable:loadingDetails`, in table order.
fn loading_details(ui: &mut Ui) -> Vec<String> {
    let Ok(data) = ui
        .res
        .archive
        .read_ref("Dialog/StringTable.rsc:loadingDetails")
    else {
        return Vec::new();
    };
    banished_formats::ui::string_table(&data)
        .map(|t| t.into_iter().map(|(_, v)| v).collect())
        .unwrap_or_default()
}

impl GameLoop for DialogLoaderLoop {
    fn name(&self) -> &'static str {
        "DialogLoaderLoop"
    }
    fn startup(&mut self, ctx: &mut Ctx) {
        log::info!("showing {}", self.dialog);
        let mut screen = Screen::new(ctx.ui);
        screen.add_dialog(ctx.ui, &self.dialog);
        self.screen = Some(screen);
    }
    fn update(&mut self, ctx: &mut Ctx, dt: f32) -> Next {
        self.elapsed += dt;
        self.frames += 1;
        // Let the dialog appear before loading (the original loads on a thread).
        if !self.loaded && self.frames > 2 {
            if let Some(next) = &mut self.next {
                next.load(ctx);
            }
            self.loaded = true;
        }
        self.desc_timer += dt;
        if self.desc_timer >= 2.0 {
            self.desc_timer = 0.0;
            self.rotate_description(ctx);
        }
        if self.loaded && self.elapsed >= self.min_seconds {
            return Next::Switch(self.next.take().expect("switches once"));
        }
        Next::Stay
    }
    fn screen(&self) -> Option<&Screen> {
        self.screen.as_ref()
    }
}
