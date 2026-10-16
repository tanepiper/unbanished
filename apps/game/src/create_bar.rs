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
    /// The requirement labels, their text and the counts (BuildRequirementUI `+0x40`, `+0x50`)
    /// and the multiplier shown (`+0x38`).
    labels: Vec<(NodeId, String, i32)>,
    count: i32,
}

/// What a DragTool's slot `+0x70` reports for the CreateBar (`FUN_140101b80`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DragInfo {
    /// 0 `"WxH"`, 1 `"%i"` of the width, 2 no size (and the counts left as they are).
    pub mode: u8,
    pub width: i32,
    pub height: i32,
    /// The multiplier of the build requirements (cells of road, bridge or fence).
    pub count: i32,
    /// The ToolbarDescription message shown; 0 is valid (the icon white, else red).
    pub message: usize,
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
        let labels = fill_requirements(ui, archive, root, toolbar, build);
        let find = |ui: &Ui, n: &str| ui.find(root, n);
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
            labels,
            count: 1,
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
        let text = size.map(|(w, h)| format!("{w}x{h}"));
        self.update_at(ui, text, None, usize::from(!valid), rect, false);
    }

    /// `FUN_140105130` for a drag (start and end cells given): the size by the mode, the
    /// requirements times the count (`FUN_14005f060`), the message, centred on the rectangle of
    /// the drag's corner cells (`FUN_140104140`) on both axes.
    pub fn update_drag(&mut self, ui: &mut Ui, info: DragInfo, rect: Option<[f32; 4]>) {
        let (text, count) = match info.mode {
            0 => (format!("{}x{}", info.width, info.height), Some(info.count)),
            1 => (info.width.to_string(), Some(info.count)),
            _ => (String::new(), None),
        };
        self.update_at(ui, Some(text), count, info.message, rect, true);
    }

    fn update_at(
        &mut self,
        ui: &mut Ui,
        size: Option<String>,
        count: Option<i32>,
        message: usize,
        rect: Option<[f32; 4]>,
        centre: bool,
    ) {
        let valid = message == 0;
        if let (Some((node, template)), Some(text)) = (&self.size, size) {
            if text.is_empty() {
                ui.set_text(*node, "");
            } else {
                with_param(ui, *node, template, &text);
            }
        }
        if let Some(count) = count.filter(|&c| c != self.count) {
            self.count = count;
            for (label, template, base) in &self.labels {
                with_param(ui, *label, template, &(base * count).to_string());
            }
        }
        if let Some(status) = self.status {
            let key = self.messages.get(message).cloned().unwrap_or_default();
            let table = self.string_table.as_deref();
            let text = ui.res.string(table, &key);
            let name = ui.res.string(table, &self.name);
            ui.set_text(status, &text.replace("@0", &name));
            ui.set_visible(status, !text.is_empty());
        }
        if let Some(icon) = self.icon {
            ui.node_mut(icon).color = if valid { 0xffff_ffff } else { 0xffff_0000 };
        }
        let Some([x0, y0, x1, y1]) = rect else {
            ui.set_visible(self.root, false);
            return;
        };
        ui.set_visible(self.root, true);
        let r = ui.node(self.root).rect;
        let (w, h) = ((r[2] - r[0]) as f32, (r[3] - r[1]) as f32);
        let y = if centre { (y0 + y1) * 0.5 } else { y0 };
        let target = (
            ((x0 + x1) * 0.5) as i32 as f32 - (w as i32 >> 1) as f32,
            (y as i32 - (h as i32 >> 1)) as f32,
        );
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

/// `BuildRequirementUI` OnOpened (`FUN_14005f120`): `imageIcon` and `textTitle` from the
/// template's ToolbarDescription, one `labelBuild{i}` per build requirement (raw material small
/// sprite and count), and `labelBuildMulti` for the multi flag. Shared by the CreateBar and the
/// toolbar's build tooltip (`Dialog/ToolTipToolBar.rsc`).
pub fn fill_requirements(
    ui: &mut Ui,
    archive: &Archive,
    root: NodeId,
    toolbar: &ToolbarDescription,
    build: Option<&BuildDescription>,
) -> Vec<(NodeId, String, i32)> {
    let mut shown = Vec::new();
    let find = |ui: &Ui, n: &str| ui.find(root, n);
    let table = toolbar.string_table.as_deref();
    if let Some(icon) = find(ui, "imageIcon") {
        if let Some(s) = ui
            .res
            .sprite(toolbar.sprite_sheet.as_deref(), &toolbar.sprite)
        {
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
            shown.push((label, template, *count));
        }
        if build.multi {
            if let Some(multi) = find(ui, "labelBuildMulti") {
                ui.set_visible(multi, true);
            }
        }
    }
    shown
}
