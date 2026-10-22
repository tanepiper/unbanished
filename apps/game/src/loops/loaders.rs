//! The loading loops: the plain loader and the one that shows a loading dialog.

use super::*;

/// `Engine::LoaderLoop` without a dialog: loads the next loop, then hands over at once.
pub struct LoaderLoop {
    name: &'static str,
    work: Option<LoadWork>,
    next: Option<Box<dyn GameLoop>>,
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

/// Lines of `StringTable:loadingDetails`, in table order.
pub(super) fn loading_details(ui: &mut Ui) -> Vec<String> {
    let Ok(data) = ui
        .res
        .archive
        .read_ref("Dialog/StringTable.rsc:loadingDetails")
    else {
        return Vec::new();
    };
    unbanished_formats::ui::string_table(&data)
        .map(|t| t.into_iter().map(|(_, v)| v).collect())
        .unwrap_or_default()
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
    /// The hourglass while loading (`FUN_1400b9a10` and the other loaders set cursor 2).
    fn cursor(&self, _ui: &Ui, _at: (i32, i32)) -> usize {
        2
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

    pub(super) fn rotate_description(&mut self, ctx: &mut Ctx) {
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

impl GameLoop for DialogLoaderLoop {
    fn name(&self) -> &'static str {
        "DialogLoaderLoop"
    }
    fn cursor(&self, _ui: &Ui, _at: (i32, i32)) -> usize {
        2
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
