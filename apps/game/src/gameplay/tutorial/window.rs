//! The tutorial window and the dialog elements a step's text and images point at.

use super::*;

pub(super) fn is_image(k: &Kind) -> bool {
    matches!(k, Kind::Image { .. })
}

pub(super) fn is_text(k: &Kind) -> bool {
    matches!(k, Kind::Text(_) | Kind::Label { .. })
}

/// `Element::FindElementType`: the first element of a kind in `n`.
pub(super) fn find_kind(ui: &Ui, n: NodeId, f: fn(&Kind) -> bool) -> Option<NodeId> {
    if f(&ui.node(n).kind) {
        return Some(n);
    }
    ui.children(n).into_iter().find_map(|c| find_kind(ui, c, f))
}

impl GameplayLoop {
    /// Every dialog's root (the user interface's dialogs).
    pub(super) fn dialog_roots(&self) -> Vec<NodeId> {
        self.screen
            .as_ref()
            .map_or(Vec::new(), |s| s.dialogs.iter().map(|d| d.root).collect())
    }

    /// `FUN_1400e6d40`: an element by its path, `ToolbarDialog:<item>` or
    /// `DialogInstance:<controller>:<element>[:<template>]`.
    pub(in crate::gameplay) fn tutorial_element(&self, ui: &Ui, path: &str) -> Option<NodeId> {
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
    pub(super) fn controller_elements(
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

    pub(super) fn open_tutorial_window(&mut self, ctx: &mut Ctx) {
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

    /// `TutorialImage` slot `+0x48`: the element, with a toolbar item's icon or key, or a
    /// key-map action's key.
    pub(super) fn image_element(&self, ctx: &mut Ctx, image: &Image) -> Option<NodeId> {
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

    /// TutorialUI's refresh (`FUN_1400e8bd0`): its buttons shown while usable.
    pub(in crate::gameplay) fn refresh_tutorial_ui(&self, ui: &mut Ui) {
        let Some(t) = self.tutorial.as_ref().and_then(|t| t.ui.as_ref()) else {
            return;
        };
        for b in [t.next, t.quit].into_iter().flatten() {
            let usable = ui.node(b).usable();
            ui.set_visible(b, usable);
        }
    }
}
