//! The tutorial (`TutorialInstance`, the step machine `FUN_1400e7280`) and its window
//! (`TutorialUI`), setups, goals and images. See docs/TUTORIAL.md.
//!
//! A scenario's FinishTutorialGoal starts it (`FUN_1400dcf60`): the window opens at the
//! screen's right edge less 464, and each frame the current step's setups update and its goals
//! are tested; when they are all met the setups exit and the next step is entered.

use unbanished_formats::pkg::Archive;
use unbanished_formats::scenario::{self, ImageExtra, TutorialPart};
use unbanished_ui::{Kind, NodeId, Ui};

use super::GameplayLoop;
use crate::loops::Ctx;

/// A step's setup (`TutorialSetup`).
#[derive(Debug, Clone)]
pub enum Setup {
    Dialog {
        enable: Vec<String>,
        close: Vec<String>,
        point: String,
        highlight: Option<String>,
        angle: i32,
        component: String,
        index: i32,
    },
    Camera([i32; 4]),
    Speed {
        speed: i32,
        unpause: bool,
    },
    Restrict {
        rect: [i32; 4],
        rotation: i32,
        place: String,
        select: String,
    },
}

/// A step's goal (`TutorialGoal`).
#[derive(Debug, Clone)]
pub enum Goal {
    Button(String),
    Count {
        kind: i32,
        amount: i32,
        reference: String,
        at_least: bool,
        path: String,
    },
    Dialog {
        path: String,
        component: String,
    },
    Speed(i32),
}

#[derive(Debug, Clone)]
pub struct Image {
    pub column: i32,
    pub row: i32,
    pub element: Option<String>,
    pub extra: ImageExtra,
}

#[derive(Debug, Clone, Default)]
pub struct Step {
    /// Read when a saved game resumes (`FUN_1400e69f0`).
    #[allow(dead_code)]
    pub checkpoint: bool,
    pub strings: Option<String>,
    pub pre: String,
    pub post: String,
    pub images: Vec<Image>,
    pub setups: Vec<Setup>,
    pub goals: Vec<Goal>,
}

#[derive(Debug, Clone, Default)]
pub struct TutorialDef {
    pub steps: Vec<Step>,
    pub window: String,
    pub next: i32,
}

fn part(archive: &Archive, r: &str) -> Option<TutorialPart> {
    scenario::tutorial_part(&archive.read_ref(&r.replace('\\', "/")).ok()?).ok()
}

/// The tutorial and its steps' parts.
pub fn load(archive: &Archive, reference: &str) -> Option<TutorialDef> {
    let TutorialPart::Tutorial(t) = part(archive, reference)? else {
        return None;
    };
    let mut steps = Vec::new();
    for s in t.steps.iter().flatten() {
        let Some(TutorialPart::Step(s)) = part(archive, s) else {
            continue;
        };
        let mut step = Step {
            checkpoint: s.checkpoint,
            strings: s.strings.clone(),
            pre: s.pre.clone(),
            post: s.post.clone(),
            ..Default::default()
        };
        for r in s.images.iter().chain(&s.setups).chain(&s.goals).flatten() {
            match part(archive, r) {
                Some(TutorialPart::Image {
                    i28,
                    i2c,
                    r20,
                    extra,
                }) => step.images.push(Image {
                    column: i28,
                    row: i2c,
                    element: r20,
                    extra,
                }),
                Some(TutorialPart::SetupDialog {
                    a,
                    b,
                    s40,
                    r50,
                    i48,
                    component,
                    i70,
                }) => step.setups.push(Setup::Dialog {
                    enable: a,
                    close: b,
                    point: s40,
                    highlight: r50,
                    angle: i48,
                    component,
                    index: i70,
                }),
                Some(TutorialPart::SetupCamera(c)) => step.setups.push(Setup::Camera(c)),
                Some(TutorialPart::SetupGameSpeed { speed, b24 }) => {
                    step.setups.push(Setup::Speed {
                        speed,
                        unpause: b24,
                    })
                }
                Some(TutorialPart::SetupRestrictPlayer { values, s38, s50 }) => {
                    step.setups.push(Setup::Restrict {
                        rect: [values[0], values[1], values[2], values[3]],
                        rotation: values[4],
                        place: s38,
                        select: s50,
                    })
                }
                Some(TutorialPart::GoalButton { s20, .. }) => step.goals.push(Goal::Button(s20)),
                Some(TutorialPart::GoalCount {
                    e20,
                    i24,
                    s30,
                    b28,
                    s48,
                }) => step.goals.push(Goal::Count {
                    kind: e20,
                    amount: i24,
                    reference: s30,
                    at_least: b28,
                    path: s48,
                }),
                Some(TutorialPart::GoalDialog { s20, component }) => {
                    step.goals.push(Goal::Dialog {
                        path: s20,
                        component,
                    })
                }
                Some(TutorialPart::GoalGameSpeed(s)) => step.goals.push(Goal::Speed(s)),
                _ => {}
            }
        }
        steps.push(step);
    }
    Some(TutorialDef {
        steps,
        window: t.window.unwrap_or_default(),
        next: t.next,
    })
}

/// Game UI `+0x80..+0xa0`: what the player may do while a RestrictPlayer setup holds.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Restrict {
    /// The cells the placement must cover, `(x0, y0)` to `(x1, y1)`, and its rotation.
    pub rect: [i32; 4],
    pub rotation: i32,
    /// `+0x88`: the only template to place; `+0x90` the only one to select.
    pub place: String,
    pub select: String,
}

/// TutorialUI's elements (`FUN_1400e8c70`).
#[derive(Debug, Clone, Default)]
pub struct TutorialUi {
    pub root: Option<NodeId>,
    pub pre: Option<NodeId>,
    pub post: Option<NodeId>,
    pub table: Option<NodeId>,
    pub next: Option<NodeId>,
    pub quit: Option<NodeId>,
}

/// What the highlight points at: an element, or an entity in the world (`+0xe8`, projected).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Element(NodeId),
    Entity(usize),
}

/// `TutorialInstance`: `+0x10` the step, `+0x18` the window, `+0x20` its TutorialUI.
#[derive(Debug, Clone, Default)]
pub struct Tutorial {
    pub def: TutorialDef,
    pub step: usize,
    pub ui: Option<TutorialUi>,
    /// The highlight dialog (TutorialHighlightDialog): its root, arrow, target and angle, and
    /// its time (`+0xb8`).
    pub highlight: Option<(NodeId, Option<NodeId>, Target, i32, f32)>,
    /// The elements pressed since the last update (element flag `0x400000`).
    pub pressed: Vec<NodeId>,
    /// The step was entered (the window opened a frame before, `FUN_1400e7280`).
    pub entered: bool,
}

impl Tutorial {
    pub fn done(&self) -> bool {
        self.def.steps.len() <= self.step
    }
}

/// `CRC32` of a controller class's UTF-16 name.
fn class_id(name: &str) -> u32 {
    unbanished_formats::crc::type_id(name)
}

fn file(r: &str) -> String {
    r.split(':').next().unwrap_or("").to_ascii_lowercase()
}

/// `FUN_1400e7720`: lock or unlock an element and everything in it.
fn set_locked(ui: &mut Ui, n: NodeId, locked: bool) {
    use unbanished_formats::ui::flags::LOCKED;
    let node = ui.node_mut(n);
    if locked {
        node.flags |= LOCKED;
    } else {
        node.flags &= !LOCKED;
    }
    for c in ui.children(n) {
        set_locked(ui, c, locked);
    }
}

impl GameplayLoop {
    /// Every dialog's root (the user interface's dialogs).
    fn dialog_roots(&self) -> Vec<NodeId> {
        self.screen
            .as_ref()
            .map_or(Vec::new(), |s| s.dialogs.iter().map(|d| d.root).collect())
    }

    /// `FUN_1400e6d40`: an element by its path, `ToolbarDialog:<item>` or
    /// `DialogInstance:<controller>:<element>[:<template>]`.
    pub(super) fn tutorial_element(&self, ui: &Ui, path: &str) -> Option<NodeId> {
        let parts: Vec<&str> = path.split(':').collect();
        match parts.as_slice() {
            ["ToolbarDialog", item] => self.toolbar.as_ref()?.button(ui, item),
            ["DialogInstance", class, rest @ ..] => {
                let id = class_id(class);
                let name = rest.first().copied();
                let template = rest.get(1).copied();
                for (root, element) in self.controller_elements(ui, id, template) {
                    let Some(name) = name else {
                        return Some(element);
                    };
                    if let Some(n) = ui.find(element, name).or_else(|| ui.find(root, name)) {
                        return Some(n);
                    }
                }
                None
            }
            _ => None,
        }
    }

    /// The open windows with a controller of the class: (window root, controller element),
    /// an entity's only when its template's name is `template` (when given).
    fn controller_elements(
        &self,
        ui: &Ui,
        id: u32,
        template: Option<&str>,
    ) -> Vec<(NodeId, NodeId)> {
        let mut out = Vec::new();
        if id == class_id("TutorialUI") {
            if let Some(root) = self.tutorial.as_ref().and_then(|t| t.ui.as_ref()?.root) {
                out.push((root, root));
            }
        }
        for (root, ids) in self.utility.controllers() {
            if ids.contains(&id) {
                out.push((root, root));
            }
        }
        let town = self.sim.town.as_ref();
        for w in &self.entities.open {
            if let (Some(t), Some(town)) = (template, town) {
                let name = w.entity.template(town).to_ascii_lowercase();
                if !name.contains(&format!("\\{}.rsc", t.to_ascii_lowercase())) {
                    continue;
                }
            }
            for (c, element) in &w.controllers {
                if c.type_id == id {
                    out.push((w.root, *element));
                }
            }
        }
        let _ = ui;
        out
    }

    /// Open the tutorial: its window at the screen's right edge less 464 (`FUN_1401056c0`),
    /// the first step entered on the next update.
    pub(super) fn start_tutorial(&mut self, ctx: &mut Ctx, def: TutorialDef) {
        self.tutorial = Some(Tutorial {
            def,
            ..Default::default()
        });
        self.open_tutorial_window(ctx);
    }

    fn open_tutorial_window(&mut self, ctx: &mut Ctx) {
        let Some(window) = self.tutorial.as_ref().map(|t| t.def.window.clone()) else {
            return;
        };
        let Some(screen) = self.screen.as_mut() else {
            return;
        };
        let x = ctx.ui.screen[0] - 0x1d0;
        let root = self.utility.open_at(
            ctx.ui,
            screen,
            ctx.archive,
            &window,
            (x.max(0), 0),
            self.sim.town.as_ref().map_or(0, |t| t.cells.size as u32),
        );
        let Some(root) = root else { return };
        let f = |n: &str| ctx.ui.find(root, n);
        let ui = TutorialUi {
            root: Some(root),
            pre: f("preText"),
            post: f("postText"),
            table: f("imageTable"),
            next: f("nextButton"),
            quit: f("quitButton"),
        };
        // `FUN_1400e8c70`: the host's close button goes.
        if let Some(c) = f("buttonClose") {
            ctx.ui.set_visible(c, false);
        }
        if let Some(t) = &mut self.tutorial {
            t.ui = Some(ui);
        }
    }

    /// `FUN_1400e6730`: the step's setups enter; its texts and images fill the window.
    fn enter_step(&mut self, ctx: &mut Ctx) {
        let Some(t) = &self.tutorial else { return };
        let Some(step) = t.def.steps.get(t.step).cloned() else {
            return;
        };
        for s in &step.setups {
            self.setup_enter(ctx, s);
        }
        let Some(ui) = self.tutorial.as_ref().and_then(|t| t.ui.clone()) else {
            return;
        };
        for (n, key) in [(ui.pre, &step.pre), (ui.post, &step.post)] {
            if let Some(n) = n {
                // `UI::String::IsValid`: a key the table lacks shows nothing.
                let text = if key.is_empty() {
                    String::new()
                } else {
                    ctx.ui.res.string(step.strings.as_deref(), key)
                };
                let text = if &text == key { String::new() } else { text };
                ctx.ui.set_visible(n, !text.is_empty());
                ctx.ui.set_text(n, &text);
            }
        }
        if let Some(table) = ui.table {
            if let Kind::Control(unbanished_ui::Control::Table { cells, rows, .. }) =
                &mut ctx.ui.node_mut(table).kind
            {
                cells.clear();
                rows.clear();
            }
            for image in &step.images {
                let Some(cell) = self.image_element(ctx, image) else {
                    continue;
                };
                if let Kind::Control(unbanished_ui::Control::Table {
                    cells,
                    rows,
                    columns,
                    ..
                }) = &mut ctx.ui.node_mut(table).kind
                {
                    let (x, y) = (image.column.max(0) as usize, image.row.max(0) as usize);
                    if columns.len() <= x {
                        columns.resize(x + 1, 0);
                    }
                    if rows.len() <= y {
                        rows.resize(y + 1, 0);
                    }
                    cells.retain(|c| (c.0, c.1) != (x, y));
                    cells.push((x, y, cell));
                }
            }
        }
    }

    /// `TutorialImage` slot `+0x48`: the element, with a toolbar item's icon or key, or a
    /// key-map action's key.
    fn image_element(&self, ctx: &mut Ctx, image: &Image) -> Option<NodeId> {
        let element = ctx.ui.instantiate(image.element.as_deref()?)?;
        match &image.extra {
            ImageExtra::None => {}
            ImageExtra::ToolbarIcon(r) => {
                let sprite = self
                    .toolbar
                    .as_ref()
                    .and_then(|t| t.entry_sprite(r))
                    .and_then(|(sheet, name)| ctx.ui.res.sprite(sheet.as_deref(), &name));
                if let (Some(s), Some(img)) = (sprite, find_kind(ctx.ui, element, is_image)) {
                    if let Kind::Image { sprite, .. } = &mut ctx.ui.node_mut(img).kind {
                        *sprite = Some(s);
                    }
                }
            }
            ImageExtra::HotKey(r) => {
                let action = self.toolbar.as_ref().and_then(|t| t.entry_action(r));
                let name = action.and_then(|a| {
                    crate::ui::toolbar::key_name(ctx.ui, &ctx.settings.keymap, a as i32)
                });
                if let (Some(name), Some(text)) = (name, find_kind(ctx.ui, element, is_text)) {
                    ctx.ui.set_text(text, &name);
                }
            }
            ImageExtra::InputKey(a) => {
                let name = crate::ui::toolbar::key_name(ctx.ui, &ctx.settings.keymap, *a);
                if let (Some(name), Some(text)) = (name, find_kind(ctx.ui, element, is_text)) {
                    ctx.ui.set_text(text, &name);
                }
            }
        }
        Some(element)
    }

    fn setup_enter(&mut self, ctx: &mut Ctx, s: &Setup) {
        match s {
            // `FUN_1400e7ac0`: everything locked but the listed elements; the listed dialogs
            // close; the highlight points at its element.
            Setup::Dialog { close, .. } => {
                self.apply_locks(ctx.ui, s);
                for path in close {
                    self.close_dialogs(ctx, path);
                }
                self.update_highlight(ctx, s, false);
            }
            // `FUN_1400e75a0`: the camera goes to the cells.
            Setup::Camera(c) => {
                self.focus_cells(c[0], c[1], c[2], c[3]);
            }
            // `FUN_1400e8230`.
            Setup::Speed { speed, unpause } => {
                if let Some(i) = super::SPEED_LEVELS.iter().position(|&l| l as i32 == *speed) {
                    self.speed = i as u8;
                }
                if *unpause {
                    self.paused = false;
                }
                self.update_hud(ctx.ui);
            }
            // `FUN_1400e83f0`: the player may place and select only the templates, there;
            // with a template to place the camera goes there.
            Setup::Restrict {
                rect,
                rotation,
                place,
                select,
            } => {
                self.restrict = Some(Restrict {
                    rect: *rect,
                    rotation: *rotation,
                    place: place.clone(),
                    select: select.clone(),
                });
                if !place.is_empty() {
                    self.focus_cells(rect[0], rect[1], rect[2], rect[3]);
                }
            }
        }
    }

    fn setup_update(&mut self, ctx: &mut Ctx, s: &Setup) {
        // `FUN_1400e7990`: the locks again (new dialogs appear) and the highlight follows.
        if let Setup::Dialog { .. } = s {
            self.apply_locks(ctx.ui, s);
            self.update_highlight(ctx, s, true);
        }
    }

    fn setup_exit(&mut self, ctx: &mut Ctx, s: &Setup) {
        match s {
            // `FUN_1400e7930`: everything unlocked; the highlight closes.
            Setup::Dialog { .. } => {
                for root in self.dialog_roots() {
                    set_locked(ctx.ui, root, false);
                }
                self.close_highlight(ctx);
            }
            // `FUN_1400e8360`: the restriction lifts; with a template to place, its tool goes.
            Setup::Restrict { place, .. } => {
                self.restrict = None;
                if !place.is_empty() {
                    self.end_placement(ctx);
                }
            }
            _ => {}
        }
    }

    /// `FUN_1400e7ec0`: every dialog locked (`FUN_1400e7800`, each root and its children),
    /// then the listed elements unlocked.
    fn apply_locks(&mut self, ui: &mut Ui, s: &Setup) {
        let Setup::Dialog { enable, .. } = s else {
            return;
        };
        for root in self.dialog_roots() {
            set_locked(ui, root, true);
        }
        for path in enable {
            if let Some(n) = self.tutorial_element(ui, path) {
                set_locked(ui, n, false);
            }
        }
    }

    /// `FUN_1400e7ac0`'s close list: an entity's window (and its selection) or a utility
    /// window with the controller.
    fn close_dialogs(&mut self, ctx: &mut Ctx, path: &str) {
        let parts: Vec<&str> = path.split(':').collect();
        let ["DialogInstance", class, ..] = parts.as_slice() else {
            return;
        };
        let id = class_id(class);
        let Some(screen) = self.screen.as_mut() else {
            return;
        };
        let roots: Vec<NodeId> = self
            .entities
            .open
            .iter()
            .filter(|w| w.controllers.iter().any(|(c, _)| c.type_id == id))
            .map(|w| w.root)
            .collect();
        for root in roots {
            self.entities.open.retain(|w| w.root != root);
            screen.remove_root(ctx.ui, root);
            self.selection.selected = None;
        }
        self.utility.close_with(ctx.ui, screen, id);
    }

    /// `FUN_1400e7bd0`: the highlight dialog points at the element (`+0x40`), or at the
    /// window of the component (`+0x58`, the `+0x70`th of its kind).
    fn update_highlight(&mut self, ctx: &mut Ctx, s: &Setup, update: bool) {
        let Setup::Dialog {
            point,
            highlight,
            angle,
            component,
            index,
            ..
        } = s
        else {
            return;
        };
        let Some(dialog) = highlight else {
            return;
        };
        let target = if !point.is_empty() {
            self.tutorial_element(ctx.ui, point).map(Target::Element)
        } else if !component.is_empty() {
            // The `index`th entity of the template (stock manager `+0x160`).
            self.sim.town.as_ref().and_then(|town| {
                (0..town.placed.len())
                    .filter(|&e| {
                        town.placed[e].alive
                            && file(&town.templates[town.placed[e].template].reference)
                                == file(component)
                    })
                    .nth((*index).max(0) as usize)
                    .map(Target::Entity)
            })
        } else {
            None
        };
        let current = self.tutorial.as_ref().and_then(|t| t.highlight);
        match (target, current) {
            (Some(target), Some((root, arrow, old, _, time))) if update => {
                if old != target {
                    if let Some(t) = &mut self.tutorial {
                        t.highlight = Some((root, arrow, target, *angle, time));
                    }
                }
            }
            (Some(target), _) => {
                self.close_highlight(ctx);
                let Some(screen) = self.screen.as_mut() else {
                    return;
                };
                let reference = dialog.replace('\\', "/");
                let Some(i) = screen.add_dialog(ctx.ui, &reference) else {
                    return;
                };
                let root = screen.dialogs[i].root;
                // `FUN_1400e54f0`: the glow hidden, the arrow kept.
                if let Some(g) = ctx.ui.find(root, "glowGroup") {
                    ctx.ui.set_visible(g, false);
                }
                let arrow = ctx.ui.find(root, "arrow");
                if let Some(t) = &mut self.tutorial {
                    t.highlight = Some((root, arrow, target, *angle, 0.0));
                }
            }
            (None, _) if !update => self.close_highlight(ctx),
            _ => {}
        }
    }

    fn close_highlight(&mut self, ctx: &mut Ctx) {
        let Some((root, ..)) = self.tutorial.as_mut().and_then(|t| t.highlight.take()) else {
            return;
        };
        if let Some(screen) = self.screen.as_mut() {
            screen.remove_root(ctx.ui, root);
        }
    }

    /// `FUN_1400e54b0`: an element target (`FUN_1400e4f90`) gets the arrow at the corner its
    /// angle names, an entity (`FUN_1400e5270`) the arrow over its projected point; it bobs
    /// `sin(t·π)·8` pixels and is turned by `-angle` about its centre.
    fn place_highlight(&mut self, ui: &mut Ui, dt: f32) {
        let Some((root, arrow, target, angle, time)) = self
            .tutorial
            .as_mut()
            .and_then(|t| t.highlight.as_mut())
            .map(|h| {
                h.4 += dt;
                *h
            })
        else {
            return;
        };
        let Some(arrow) = arrow else { return };
        let size = match &ui.node(arrow).kind {
            Kind::Image {
                sprite: Some(s), ..
            } => [s.width, s.height],
            _ => [0, 0],
        };
        let bob = (time * std::f32::consts::PI).sin() * 8.0;
        let (x, y) = match target {
            // `FUN_1400e4f90`: the arrow's box (the sprite's width square) against a corner of
            // the element's rectangle, the sprite's point (down and right at 0°) on it: 0°
            // above and left of the top left, 90° below and left of the bottom left, 180°
            // below and right of the bottom right, 270° above and right of the top right. It
            // bobs along the same diagonal.
            Target::Element(n) => {
                let [l, t, r, b] = ui.node(n).rect;
                let w = size[0];
                let (x, y, sx, sy) = match angle {
                    90 => (l - w, b, 1.0, -1.0),
                    180 => (r, b, -1.0, -1.0),
                    270 => (r, t - w, -1.0, 1.0),
                    _ => (l - w, t - w, 1.0, 1.0),
                };
                (sx * bob + x as f32, sy * bob + y as f32)
            }
            // `FUN_1400e5270`: centred over the point, its foot there, bobbing up and down.
            Target::Entity(e) => {
                let Some((cx, cy)) = self.screen_point(ui.screen, e) else {
                    ui.set_visible(arrow, false);
                    return;
                };
                (
                    (cx as i32 - (size[0] >> 1)) as f32,
                    (cy as i32 - size[1]) as f32 + bob,
                )
            }
        };
        ui.set_visible(arrow, true);
        let node = ui.node_mut(root);
        node.alignment = 0;
        node.margins = [(x as i32).max(0), 0, (y as i32).max(0), 0];
        ui.node_mut(arrow).rotation = -(angle as f32);
    }

    /// An entity's position on the screen (`FUN_140105850`).
    fn screen_point(&self, screen: [i32; 2], e: usize) -> Option<(f32, f32)> {
        let (town, view) = (self.sim.town.as_ref()?, self.view.as_ref()?);
        if screen[0] <= 0 || screen[1] <= 0 {
            return None;
        }
        let pos = crate::tools::selection::Entity::Placed(e).position(town);
        let vp = view.view_proj(screen[0] as f32 / screen[1] as f32);
        let p = vp * pos.extend(1.0);
        if p.w <= 0.0 {
            return None;
        }
        Some((
            (p.x / p.w * 0.5 + 0.5) * screen[0] as f32,
            (0.5 - p.y / p.w * 0.5) * screen[1] as f32,
        ))
    }

    /// The camera to the middle of the cells.
    fn focus_cells(&mut self, x0: i32, y0: i32, x1: i32, y1: i32) {
        let Some(town) = &self.sim.town else { return };
        if x0 < 0 || y0 < 0 {
            return;
        }
        let half = (town.cells.size as i32 >> 1) as f32;
        let x = (x0 + x1) as f32 * 0.5 - half + 0.5;
        let y = (y0 + y1) as f32 * 0.5 - half + 0.5;
        if let Some(super::SceneView {
            camera: super::SceneCamera::Map(camera),
            ..
        }) = &mut self.view
        {
            camera.target = glam::Vec2::new(x, y);
        }
    }

    /// A goal (`TutorialGoal` slot `+0x40`).
    fn goal_met(&self, ui: &Ui, archive: &Archive, g: &Goal) -> bool {
        match g {
            // `FUN_1400e41b0`: the element was pressed.
            Goal::Button(path) => {
                let Some(n) = self.tutorial_element(ui, path) else {
                    return false;
                };
                self.tutorial
                    .as_ref()
                    .is_some_and(|t| t.pressed.iter().any(|&p| p == n || ui.descendant(n, p)))
            }
            Goal::Count {
                kind,
                amount,
                reference,
                at_least,
                path,
            } => {
                let n = self.goal_count(ui, archive, *kind, reference, path);
                if *at_least {
                    *amount <= n
                } else {
                    n == *amount
                }
            }
            // `FUN_1400e43b0`: a window with the controller is open (on the template).
            Goal::Dialog { path, component } => {
                let parts: Vec<&str> = path.split(':').collect();
                let ["DialogInstance", class, ..] = parts.as_slice() else {
                    return false;
                };
                let id = class_id(class);
                let Some(town) = &self.sim.town else {
                    return false;
                };
                self.entities.open.iter().any(|w| {
                    w.controllers.iter().any(|(c, _)| c.type_id == id)
                        && (component.is_empty()
                            || file(w.entity.template(town)) == file(component))
                })
            }
            // `FUN_1400e4620`.
            Goal::Speed(s) => !self.paused && super::SPEED_LEVELS[self.speed as usize] as i32 == *s,
        }
    }

    /// `FUN_1400e4760`'s counts.
    fn goal_count(&self, ui: &Ui, _archive: &Archive, kind: i32, r: &str, path: &str) -> i32 {
        let Some(town) = &self.sim.town else { return 0 };
        let of_template =
            |e: usize| file(&town.templates[town.placed[e].template].reference) == file(r);
        let profession = || {
            town.construction
                .professions
                .list
                .iter()
                .position(|p| p.reference.eq_ignore_ascii_case(r))
        };
        let live = || {
            town.citizens
                .iter()
                .zip(&town.agents.states)
                .filter(|(c, _)| c.alive)
        };
        match kind {
            // The template's entities (`FUN_1400974e0`).
            0 => (0..town.placed.len())
                .filter(|&e| town.placed[e].alive && of_template(e))
                .count() as i32,
            // Working adults of the profession.
            1 => {
                let p = profession();
                live()
                    .filter(|(c, s)| {
                        c.is_adult(&town.rules) && c.school.is_none() && Some(s.profession) == p
                    })
                    .count() as i32
            }
            2 => live().count() as i32,
            // The profession's workplaces (`FUN_140096570`).
            3 => profession()
                .and_then(|p| town.construction.workforce.registry.get(&p))
                .map_or(0, |l| l.len() as i32),
            // The stock manager's storages (`+8`).
            4 => town.storage_sites.len() as i32,
            // Roads planned (map `+0xf0`).
            5 => (!town.construction.pending_roads.is_empty()) as i32,
            // Workers at the profession's workplaces taking them (`FUN_140135080`).
            6 => profession()
                .and_then(|p| town.construction.workforce.registry.get(&p))
                .map_or(0, |l| {
                    l.iter()
                        .filter_map(|e| town.construction.workforce.workplaces.get(e))
                        .filter(|w| w.enabled)
                        .map(|w| w.workers.len() as i32)
                        .sum()
                }),
            // What the trading posts keep of the material.
            7 => {
                let m = town
                    .materials
                    .iter()
                    .position(|m| file(&m.reference) == file(r));
                town.construction
                    .trading_posts
                    .iter()
                    .filter_map(|p| town.construction.sites.get(p)?.trade.as_ref())
                    .map(|t| m.and_then(|m| t.keep.get(&m).copied()).unwrap_or(0))
                    .sum()
            }
            // What the trading posts hold.
            8 => town
                .construction
                .trading_posts
                .iter()
                .filter_map(|p| town.storages.get(town.construction.sites.get(p)?.storage))
                .flat_map(|s| s.items.iter())
                .map(|i| i.count)
                .sum(),
            // The spinner's value, unless it is being typed in.
            9 => self
                .tutorial_element(ui, path)
                .and_then(|n| crate::ui::building_ui::spinner_value(ui, n))
                .unwrap_or(0),
            // The template's activated entities (state machine past 0).
            10 => town
                .construction
                .sites
                .iter()
                .filter(|(&e, s)| s.machine.state != 0 && of_template(e))
                .count() as i32,
            _ => 0,
        }
    }

    /// `FUN_1400e7280`: the step machine, after the frame's iterations.
    pub(super) fn update_tutorial(&mut self, ctx: &mut Ctx, dt: f32) {
        if self.tutorial.as_ref().is_none_or(|t| t.done()) {
            return;
        }
        if self.tutorial.as_ref().is_some_and(|t| t.ui.is_none()) {
            self.open_tutorial_window(ctx);
            return;
        }
        if self.tutorial.as_ref().is_some_and(|t| !t.entered) {
            if let Some(t) = &mut self.tutorial {
                t.entered = true;
            }
            self.enter_step(ctx);
        }
        let Some(step) = self
            .tutorial
            .as_ref()
            .and_then(|t| t.def.steps.get(t.step).cloned())
        else {
            return;
        };
        for s in &step.setups {
            self.setup_update(ctx, s);
        }
        let met = step
            .goals
            .iter()
            .filter(|g| self.goal_met(ctx.ui, ctx.archive, g))
            .count();
        if let Some(t) = &mut self.tutorial {
            t.pressed.clear();
        }
        self.place_highlight(ctx.ui, dt);
        if met < step.goals.len() {
            return;
        }
        for s in &step.setups {
            self.setup_exit(ctx, s);
        }
        let Some(t) = &mut self.tutorial else { return };
        t.step += 1;
        if !t.done() {
            self.enter_step(ctx);
            return;
        }
        // Done: the next scenario starts (`FUN_1400e6240`, none below 0).
        let next = self.tutorial.as_ref().map_or(-1, |t| t.def.next);
        if 0 <= next {
            self.next_scenario = Some(next as usize);
        }
        // Done: the window closes (`FUN_140106f50`).
        if let (Some(root), Some(screen)) = (
            self.tutorial.as_ref().and_then(|t| t.ui.as_ref()?.root),
            self.screen.as_mut(),
        ) {
            self.utility.close_root(ctx.ui, screen, root);
        }
    }

    /// The tutorial's quit button (`0x579`, `FUN_1400e8bb0`).
    pub(super) fn tutorial_quit(&self, node: NodeId) -> bool {
        self.tutorial
            .as_ref()
            .and_then(|t| t.ui.as_ref()?.quit)
            .is_some_and(|q| q == node)
    }

    /// TutorialUI's refresh (`FUN_1400e8bd0`): its buttons shown while usable.
    pub(super) fn refresh_tutorial_ui(&self, ui: &mut Ui) {
        let Some(t) = self.tutorial.as_ref().and_then(|t| t.ui.as_ref()) else {
            return;
        };
        for b in [t.next, t.quit].into_iter().flatten() {
            let usable = ui.node(b).usable();
            ui.set_visible(b, usable);
        }
    }
}

fn is_image(k: &Kind) -> bool {
    matches!(k, Kind::Image { .. })
}

fn is_text(k: &Kind) -> bool {
    matches!(k, Kind::Text(_) | Kind::Label { .. })
}

/// `Element::FindElementType`: the first element of a kind in `n`.
fn find_kind(ui: &Ui, n: NodeId, f: fn(&Kind) -> bool) -> Option<NodeId> {
    if f(&ui.node(n).kind) {
        return Some(n);
    }
    ui.children(n).into_iter().find_map(|c| find_kind(ui, c, f))
}

/// The game's goal (`Game +0x150`, `+0x158` once it is met or failed): the scenario's
/// achievement.
#[derive(Debug, Clone)]
pub struct GameGoal {
    pub achievement: crate::achievements::Achievement,
    pub instance: crate::achievements::AchievementInstance,
    pub done: bool,
}

/// `Game\Scenario\ScenarioList.rsc`.
pub fn scenarios(archive: &Archive) -> Vec<(String, scenario::Scenario)> {
    let read = |r: &str| archive.read_ref(&r.replace('\\', "/")).ok();
    let Some(list) = read("Game/Scenario/ScenarioList.rsc:resource")
        .and_then(|d| unbanished_formats::game::external_list(&d).ok())
    else {
        return Vec::new();
    };
    list.iter()
        .filter_map(|r| Some((r.clone(), scenario::scenario(&read(r)?).ok()?)))
        .collect()
}

impl GameplayLoop {
    /// The scenario's goal and, with a FinishTutorialGoal, its tutorial (`FUN_1400dcf60`).
    ///
    /// `FUN_14009bfd0`: a scenario's start (the loop given its index) makes a new goal
    /// instance; a saved game loaded otherwise keeps its saved one (`Game +0x150`) and whether
    /// it was met (`+0x158`).
    pub(super) fn start_scenario(&mut self, ctx: &mut Ctx) {
        let Some(i) = self.params.scenario else {
            return;
        };
        let Some((reference, s)) = scenarios(ctx.archive).into_iter().nth(i) else {
            return;
        };
        let read = |r: &str| ctx.archive.read_ref(&r.replace('\\', "/")).ok();
        let (goal_references, goals): (Vec<String>, Vec<scenario::Goal>) = s
            .achievement
            .goals
            .iter()
            .flatten()
            .filter_map(|g| Some((g.clone(), scenario::goal(&read(g)?).ok()?)))
            .unzip();
        let tutorial = goals.iter().find_map(|g| match &g.kind {
            scenario::GoalKind::FinishTutorial(Some(t)) => Some(t.clone()),
            _ => None,
        });
        let achievement = crate::achievements::Achievement {
            reference,
            goal_references,
            desc: s.achievement.clone(),
            goals,
        };
        let restored = match (&self.saved, self.scenario_start) {
            (Some(saved), false) => saved.state.goal.map(|id| {
                let instance = crate::achievements::saved_instance(saved, id, &achievement);
                (instance, saved.state.goal_done)
            }),
            _ => None,
        };
        let (instance, done) = match restored {
            Some((Some(instance), done)) => (instance, done),
            _ => (
                crate::achievements::AchievementInstance::new(&achievement),
                false,
            ),
        };
        self.goal = Some(GameGoal {
            instance,
            achievement,
            done,
        });
        if let Some(def) = tutorial.and_then(|t| load(ctx.archive, &t)) {
            // `Game +0xd0 |= 0x5f0`: no disasters, no saving.
            if let Some(town) = &mut self.sim.town {
                town.construction.game_flags |= 0x5f0;
            }
            if let Some(env) = &mut self.sim.environment {
                env.game_flags |= 0x5f0;
            }
            self.start_tutorial(ctx, def);
        }
    }

    /// `FUN_14009b850`'s goal: the tutorial's step machine (its goal instance's update), the
    /// goals, and once met or failed event `0x1a` / `0x1b` with the title (`FUN_14009bd00`).
    pub(super) fn update_goal(&mut self, ctx: &mut Ctx, dt: f32, sim_dt: i64) {
        self.update_tutorial(ctx, dt);
        let done = self.tutorial.as_ref().is_some_and(|t| t.done());
        let Some(town) = &self.sim.town else { return };
        let Some(goal) = self.goal.as_mut().filter(|g| !g.done) else {
            return;
        };
        let terrain = format!(r"Game\Terrain\{}.rsc", self.params.terrain);
        let x = crate::achievements::GoalContext {
            town,
            statistics: &self.sim.statistics,
            year: self.sim.calendar.year,
            year_seconds: self.sim.calendar.seconds(),
            terrain: &terrain,
            climate: &self.params.climate_ref,
            size: self.params.size as i32,
        };
        goal.instance
            .update(&goal.achievement, sim_dt, &x, Some(ctx.archive), |_| done);
        let met = goal.instance.is_met();
        if !met && !goal.instance.has_failed() {
            return;
        }
        goal.done = true;
        let d = &goal.achievement.desc;
        let title = ctx.ui.res.string(d.strings.as_deref(), &d.title);
        let kind = if met { 0x1a } else { 0x1b };
        let (month, year) = (self.month(), self.sim.calendar.year as i32);
        self.sim.events.add(
            kind,
            4,
            -1,
            month,
            year,
            [title, String::new(), String::new()],
            false,
        );
    }
}

/// `FUN_14007d180`: the game for scenario `index`. A scenario naming a save (`+0x88`, the
/// tutorials') loads `Save\{name}.sav` from the data (`SaveLoad` operation 3) and starts it
/// with the scenario (`FUN_1400b9ec0`); the others start a new game ([`scenario_params`]).
///
/// The loop's scenario (`+0x15c`) is the index given: `FUN_14009bfd0` keeps it over the save's
/// when it is not −1.
pub fn scenario_game(
    ui: &mut Ui,
    archive: &unbanished_formats::pkg::Archive,
    global: Option<&crate::global::GlobalData>,
    s: &scenario::Scenario,
    index: usize,
) -> GameplayLoop {
    if !s.save.is_empty() {
        let name = format!("Save\\{}.sav", s.save);
        match archive
            .read(&name)
            .and_then(|b| crate::saves::saved::SavedGame::parse(&b))
        {
            Ok(mut saved) => {
                saved.state.scenario = index as i32;
                let mut g = GameplayLoop::from_save(saved);
                g.scenario_start = true;
                return g;
            }
            Err(e) => log::error!("{name}: {e}"),
        }
    }
    GameplayLoop::new(scenario_params(ui, global, s, index))
}

/// `FUN_1400b9a10`'s new game for a scenario: its seed (`+0x60`), size (`+0x64`), town name
/// (`+0x58`), disasters (`+0x68`), start condition (`+0x70`), terrain (`+0x78`) and climate
/// (`+0x80`).
pub fn scenario_params(
    ui: &mut Ui,
    global: Option<&crate::global::GlobalData>,
    s: &scenario::Scenario,
    index: usize,
) -> crate::view::world::WorldParams {
    let stem = |r: &Option<String>| {
        let r = r.as_deref().unwrap_or("");
        let path = r.split(':').next().unwrap_or(r);
        path.rsplit(['\\', '/'])
            .next()
            .unwrap_or(path)
            .trim_end_matches(".rsc")
            .to_string()
    };
    let same = |a: &str, b: &Option<String>| {
        b.as_deref().is_some_and(|b| {
            a.split(':')
                .next()
                .unwrap_or("")
                .eq_ignore_ascii_case(b.split(':').next().unwrap_or(""))
        })
    };
    let climate = global.and_then(|g| g.climates.iter().find(|c| same(&c.reference, &s.r80)));
    let condition = global.and_then(|g| {
        g.start_conditions
            .iter()
            .find(|c| same(&c.reference, &s.r70))
    });
    let defaults = crate::view::world::WorldParams::default();
    crate::view::world::WorldParams {
        town_name: s.town_name.clone(),
        terrain: stem(&s.r78),
        climate: climate.map_or_else(
            || stem(&s.r80),
            |c| ui.res.string(c.label.0.as_deref(), &c.label.1),
        ),
        climate_ref: climate.map_or(defaults.climate_ref, |c| c.reference.clone()),
        start_condition: condition.map_or_else(
            || stem(&s.r70),
            |c| ui.res.string(c.data.list.as_deref(), &c.data.name),
        ),
        start_condition_ref: condition.map_or_else(
            || s.r70.clone().unwrap_or_default(),
            |c| c.reference.clone(),
        ),
        size: s.i64.max(0) as usize,
        seed: s.i60 as u32,
        disasters: s.b68,
        scenario: Some(index),
    }
}
