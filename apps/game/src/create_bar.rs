//! The build tool's floating bar (`Dialog/CreateBar.rsc`, controller `BuildRequirementUI`):
//! opened by `FUN_140104eb0`, filled by `FUN_14005f120` (OnOpened) and updated each hover by
//! `FUN_140105130`. See docs/BUILD-TOOLS.md.

use banished_formats::game::{raw_material_description, BuildDescription, ToolbarDescription};
use banished_formats::pkg::Archive;
use banished_ui::{Kind, NodeId, Ui};

use crate::loops::Screen;

/// `DAT_140178800` / `DAT_140178824`: smoothing of the bar's position.
const FOLLOW_NEW: f32 = 0.25;
const FOLLOW_OLD: f32 = 0.75;

fn text_of(ui: &Ui, id: NodeId) -> String {
    match &ui.node(id).kind {
        Kind::Text(t) | Kind::Label { text: t, .. } => t.text.clone(),
        _ => String::new(),
    }
}

fn with_param(ui: &mut Ui, id: NodeId, template: &str, value: &str) {
    ui.set_text(id, &template.replace("@0", value));
}

pub struct CreateBar {
    root: NodeId,
    size: Option<(NodeId, String)>,
    status: Option<NodeId>,
    icon: Option<NodeId>,
    string_table: Option<String>,
    messages: Vec<String>,
    name: String,
    /// Smoothed upper-left corner (`param_1 + 8`), negative until first placed.
    position: Option<(f32, f32)>,
}

impl CreateBar {
    /// `FUN_140104eb0` + `FUN_14005f120`.
    pub fn open(
        ui: &mut Ui,
        screen: &mut Screen,
        archive: &Archive,
        toolbar: &ToolbarDescription,
        build: Option<&BuildDescription>,
    ) -> Option<CreateBar> {
        let dialog = screen.add_dialog(ui, "Dialog/CreateBar.rsc:resource")?;
        let root = screen.dialogs[dialog].root;
        let find = |ui: &Ui, n: &str| ui.find(root, n);
        let table = toolbar.string_table.as_deref();
        if let Some(icon) = find(ui, "imageIcon") {
            if let Some(s) = ui.res.sprite(toolbar.sprite_sheet.as_deref(), &toolbar.sprite) {
                ui.set_images(icon, s);
            }
        }
        if let Some(title) = find(ui, "textTitle") {
            let t = ui.res.string(table, &toolbar.name);
            ui.set_text(title, &t);
        }
        if let Some(multi) = find(ui, "labelBuildMulti") {
            ui.set_visible(multi, false);
        }
        let mut labels = Vec::new();
        while let Some(l) = find(ui, &format!("labelBuild{}", labels.len())) {
            ui.set_visible(l, false);
            labels.push(l);
        }
        if let Some(build) = build {
            for ((count, material), &label) in build.materials.iter().zip(&labels) {
                let template = text_of(ui, label);
                if let Some(d) = material.as_deref().and_then(|m| {
                    let file = m.split(':').next()?.replace('\\', "/");
                    raw_material_description(&archive.read_ref(&format!("{file}:rawmaterial")).ok()?)
                        .ok()
                }) {
                    if let Some(s) = ui.res.sprite(d.sprite_sheet.as_deref(), &d.name_small) {
                        if let Kind::Label { icon, .. } = &mut ui.node_mut(label).kind {
                            *icon = Some(s);
                        }
                    }
                }
                with_param(ui, label, &template, &count.to_string());
                ui.set_visible(label, true);
            }
            if build.multi {
                if let Some(multi) = find(ui, "labelBuildMulti") {
                    ui.set_visible(multi, true);
                }
            }
        }
        let size = find(ui, "textSize").map(|n| (n, text_of(ui, n)));
        Some(CreateBar {
            root,
            size,
            status: find(ui, "textStatus"),
            icon: find(ui, "imageIcon"),
            string_table: toolbar.string_table.clone(),
            messages: toolbar.messages.clone(),
            name: toolbar.name_lower.clone(),
            position: None,
        })
    }

    pub fn close(self, ui: &mut Ui, screen: &mut Screen) {
        screen.remove_root(ui, self.root);
    }

    /// `FUN_140105130` for a placed building: size `w×h`, message `messages[!valid]` with the
    /// building's name, icon white or red, centred above the building's screen rectangle.
    pub fn update(
        &mut self,
        ui: &mut Ui,
        size: Option<(i32, i32)>,
        valid: bool,
        rect: Option<[f32; 4]>,
    ) {
        if let (Some((node, template)), Some((w, h))) = (&self.size, size) {
            with_param(ui, *node, template, &format!("{w}x{h}"));
        }
        if let Some(status) = self.status {
            let key = self
                .messages
                .get(usize::from(!valid))
                .cloned()
                .unwrap_or_default();
            let table = self.string_table.as_deref();
            let text = ui.res.string(table, &key);
            let name = ui.res.string(table, &self.name);
            ui.set_text(status, &text.replace("@0", &name));
            ui.set_visible(status, !text.is_empty());
        }
        if let Some(icon) = self.icon {
            ui.node_mut(icon).color = if valid { 0xffff_ffff } else { 0xffff_0000 };
        }
        let Some([x0, y0, x1, _]) = rect else {
            ui.set_visible(self.root, false);
            return;
        };
        ui.set_visible(self.root, true);
        let r = ui.node(self.root).rect;
        let (w, h) = ((r[2] - r[0]) as f32, (r[3] - r[1]) as f32);
        let target = (((x0 + x1) * 0.5) as i32 as f32 - (w as i32 >> 1) as f32, (y0 as i32 - (h as i32 >> 1)) as f32);
        let p = match self.position {
            Some((px, py)) => (
                target.0 * FOLLOW_NEW + px * FOLLOW_OLD,
                target.1 * FOLLOW_NEW + py * FOLLOW_OLD,
            ),
            None => target,
        };
        self.position = Some(p);
        // `DialogInstance::SetUpperLeft`.
        let node = ui.node_mut(self.root);
        node.alignment = 0;
        node.margins = [(p.0 as i32).max(0), 0, (p.1 as i32).max(0), 0];
    }
}
