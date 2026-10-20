//! Building and destroying: the construction and demolition progress.

use super::*;

/// What DestroyUI reads (`FUN_140066460`): `FUN_140123b60` damaged, the StateMachine's
/// pending else current state, `+0x50`, `+0x54/+0x58` the deconstruction, and whether `+0x60`
/// is the building's own template (a rebuild) or another than the description's (an upgrade,
/// `FUN_140123ad0`).
#[derive(Debug, Clone, Copy, Default)]
pub struct DestroyInfo {
    pub entity: usize,
    pub is_damaged: bool,
    pub state: i8,
    pub damaged: bool,
    pub count: i32,
    pub target: i32,
    pub rebuild: bool,
    pub upgrade: bool,
}

/// `DestroyUI` (vtable `0x140161c30`): bind `FUN_140066ab0`, refresh `FUN_140066460`, events
/// `FUN_1400669e0`.
pub struct DestroyUi {
    /// `+0x30` `progressRemove`, `+0x38` `progressDestruct` (each with its text template).
    remove: Option<(NodeId, Option<(NodeId, String)>)>,
    destruct: Option<(NodeId, Option<(NodeId, String)>)>,
    /// `+0x40..+0x58`: `textRemove`, `textUpgrade`, `textDamage`, `textFixing`.
    text_remove: Option<NodeId>,
    text_upgrade: Option<NodeId>,
    text_damage: Option<NodeId>,
    text_fixing: Option<NodeId>,
    /// `+0x60` `buttonReclaim` (`0x1771`), `+0x68` `buttonDestroy` (`0x1773`), `+0x70`
    /// `buttonRebuild` (`0x1772`).
    reclaim: Option<NodeId>,
    destroy: Option<NodeId>,
    rebuild: Option<NodeId>,
    filled: bool,
}

/// `BuildUI`: bind `FUN_1400b7410`, refresh `FUN_1400b6f20`, events `FUN_1400b7370`. The
/// `storageTable` row 0 counts the cleared objects; rows 1.. are the BuildDescription's
/// materials, then the build work, then blanks.
pub struct BuildUi {
    /// `+0x48`: row 0's `textTotal`.
    cleared: Option<(NodeId, String)>,
    /// `+0x30/+0x38`: rows 1.. (`labelStorage`, `textTotal` and its template).
    rows: Vec<(Option<NodeId>, Option<(NodeId, String)>)>,
    /// `+0x40`, its content text template.
    progress: Option<NodeId>,
    progress_text: Option<(NodeId, String)>,
    /// Element id 6000.
    pause: Option<NodeId>,
    /// The bind's entity-dependent half runs on the first refresh, which has the entity.
    filled: bool,
}

/// What BuildUI reads from the selected building.
#[derive(Debug, Clone)]
pub struct BuildInfo {
    pub entity: usize,
    /// BuildComponent present: BuildDescription `+0x20` `(material, count × stages, delivered)`
    /// where delivered is the site storage's count less its reserved count; `+0x30` work ×
    /// stages and progress (`+0x48`).
    pub build: Option<BuildProgress>,
    /// ClearAreaComponent `+0x48` and `FUN_14011d800`.
    pub clear_total: i32,
    pub clear_remaining: i32,
    /// WorkPlaceComponent `+0x50` when there is one.
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone)]
pub struct BuildProgress {
    pub materials: Vec<(usize, i32, i32)>,
    pub work: i32,
    pub progress: i32,
}

impl DestroyUi {
    pub fn bind(ui: &mut Ui, element: NodeId) -> DestroyUi {
        let progress = |ui: &Ui, n: &str| {
            ui.find(element, n).map(|p| {
                let content = match &ui.node(p).kind {
                    Kind::Control(Control::Progress { content, .. }) => *content,
                    _ => None,
                };
                (p, content.map(|c| (c, text_of(ui, c))))
            })
        };
        DestroyUi {
            remove: progress(ui, "progressRemove"),
            destruct: progress(ui, "progressDestruct"),
            text_remove: ui.find(element, "textRemove"),
            text_upgrade: ui.find(element, "textUpgrade"),
            text_damage: ui.find(element, "textDamage"),
            text_fixing: ui.find(element, "textFixing"),
            reclaim: ui.find(element, "buttonReclaim"),
            destroy: ui.find(element, "buttonDestroy"),
            rebuild: ui.find(element, "buttonRebuild"),
            filled: false,
        }
    }

    pub(super) fn show(ui: &mut Ui, n: Option<NodeId>, visible: bool) {
        if let Some(n) = n {
            ui.set_visible(n, visible);
        }
    }

    pub(super) fn fill_progress(
        ui: &mut Ui,
        p: &Option<(NodeId, Option<(NodeId, String)>)>,
        fill: f32,
    ) {
        if let Some((node, text)) = p {
            ui.set_value(*node, fill);
            if let Some((t, template)) = text {
                let percent = (fill * 100.0 + 0.5) as i32;
                ui.set_text(*t, &template.replace("@0", &percent.to_string()));
            }
        }
    }

    pub fn refresh(&mut self, ui: &mut Ui, data: &EntityData) {
        let Some(d) = data.destroy else { return };
        if !self.filled {
            // The bind's entity half: damaged, no reclaiming.
            self.filled = true;
            if d.is_damaged || d.damaged {
                Self::show(ui, self.reclaim, false);
            }
        }
        if !d.is_damaged {
            Self::show(ui, self.rebuild, false);
            Self::show(ui, self.destroy, false);
            Self::show(ui, self.text_damage, false);
            Self::show(ui, self.reclaim, true);
            if let Some(storage) = data.storage {
                let fill = if storage.capacity < 1 {
                    0.0
                } else {
                    storage.used(data.materials) as f32 / storage.capacity as f32
                };
                Self::fill_progress(ui, &self.remove, fill);
            }
            let fill = if d.target == 0 {
                0.0
            } else {
                d.count as f32 / d.target as f32
            };
            Self::fill_progress(ui, &self.destruct, fill);
            let deconstructing = fill != 0.0;
            Self::show(ui, self.remove.as_ref().map(|p| p.0), !deconstructing);
            Self::show(ui, self.destruct.as_ref().map(|p| p.0), deconstructing);
            if d.rebuild {
                Self::show(ui, self.text_fixing, true);
                Self::show(ui, self.text_remove, false);
                Self::show(ui, self.text_upgrade, false);
            } else {
                Self::show(ui, self.text_remove, !d.upgrade);
                Self::show(ui, self.text_upgrade, d.upgrade);
                Self::show(ui, self.text_fixing, false);
            }
            return;
        }
        // Damaged (6) or rubble (7): rebuild or clear it, not while it burns (6).
        for b in [self.rebuild, self.destroy].into_iter().flatten() {
            ui.node_mut(b).enabled = d.state != 6;
        }
        Self::show(ui, self.rebuild, true);
        Self::show(ui, self.destroy, true);
        Self::show(ui, self.text_damage, true);
        for n in [
            self.text_fixing,
            self.text_remove,
            self.text_upgrade,
            self.reclaim,
        ] {
            Self::show(ui, n, false);
        }
        Self::show(ui, self.remove.as_ref().map(|p| p.0), false);
        Self::show(ui, self.destruct.as_ref().map(|p| p.0), false);
    }

    /// `FUN_1400669e0`: `0x1771` `FUN_1401238d0` (keep it), `0x1772` `FUN_140123940` (state
    /// message 0: rebuild), `0x1773` `FUN_1401239a0` (state message 1: clear it).
    pub fn click(&self, node: NodeId, data: &EntityData) -> Option<WorkPlaceClick> {
        let entity = data.destroy?.entity;
        use unbanished_game::work::professions::Command;
        if Some(node) == self.reclaim {
            return Some(WorkPlaceClick::Command(Command::CancelDemolition {
                entity,
            }));
        }
        let message = if Some(node) == self.rebuild {
            0
        } else if Some(node) == self.destroy {
            1
        } else {
            return None;
        };
        Some(WorkPlaceClick::Command(Command::StateMessage {
            entity,
            message,
        }))
    }
}

impl BuildUi {
    pub fn bind(ui: &mut Ui, element: NodeId) -> BuildUi {
        let param =
            |ui: &Ui, root: NodeId, n: &str| ui.find(root, n).map(|id| (id, text_of(ui, id)));
        let table = ui.find(element, "storageTable");
        let children = table.map(|t| ui.children(t)).unwrap_or_default();
        let cleared = children.first().and_then(|&c| param(ui, c, "textTotal"));
        let rows = children
            .iter()
            .skip(1)
            .map(|&c| (ui.find(c, "labelStorage"), param(ui, c, "textTotal")))
            .collect();
        let progress = ui.find(element, "progress");
        let progress_text = progress.and_then(|p| match &ui.node(p).kind {
            Kind::Control(Control::Progress { content, .. }) => {
                content.map(|c| (c, text_of(ui, c)))
            }
            _ => None,
        });
        BuildUi {
            cleared,
            rows,
            progress,
            progress_text,
            pause: ui.find(element, "buttonPause"),
            filled: false,
        }
    }

    /// The entity half of `FUN_1400b7410`.
    pub(super) fn fill(
        &mut self,
        ui: &mut Ui,
        archive: &Archive,
        data: &EntityData,
        b: &BuildInfo,
    ) {
        if let Some(p) = self.pause {
            match b.enabled {
                None => ui.node_mut(p).enabled = false,
                Some(on) => ui.set_checked(p, !on),
            }
        }
        if let Some((id, t)) = &self.cleared {
            ui.set_text(
                *id,
                &t.replace("@0", "0")
                    .replace("@1", &b.clear_total.to_string()),
            );
        }
        let materials = b.build.as_ref().map_or(&[][..], |p| &p.materials[..]);
        for (j, (label, total)) in self.rows.iter().enumerate() {
            if let Some(&(m, required, _)) = materials.get(j) {
                if let Some((id, t)) = total {
                    ui.set_text(*id, &t.replace("@1", &required.to_string()));
                }
                let d = material_file(&data.materials[m].reference);
                let d = archive
                    .read_ref(&format!("{d}:rawmaterial"))
                    .ok()
                    .and_then(|b| raw_material_description(&b).ok());
                if let (Some(l), Some(d)) = (*label, d) {
                    if let Some(s) = ui.res.sprite(d.sprite_sheet.as_deref(), &d.name) {
                        if let Kind::Label { icon, .. } = &mut ui.node_mut(l).kind {
                            *icon = Some(s);
                        }
                    }
                    let name = ui.res.string(d.string_table.as_deref(), &d.name);
                    ui.set_text(l, &name);
                    let table = ui.node(l).string_table.clone();
                    ui.node_mut(l).tooltip = ui.res.string(table.as_deref(), "ResourceTip");
                }
            } else if let (Some(p), true) = (&b.build, j == materials.len()) {
                if let Some((id, t)) = total {
                    ui.set_text(*id, &t.replace("@1", &p.work.to_string()));
                }
            } else {
                if let Some((id, _)) = total {
                    ui.set_text(*id, "");
                }
                if let Some(l) = *label {
                    ui.set_text(l, "");
                    if let Kind::Label { icon, .. } = &mut ui.node_mut(l).kind {
                        *icon = None;
                    }
                }
            }
        }
        // The totals keep their `@0` for the refresh: store the half-filled templates.
        for (_, total) in &mut self.rows {
            if let Some((id, t)) = total {
                *t = text_of(ui, *id);
            }
        }
        if let Some((_, t)) = &mut self.cleared {
            *t = t.replace("@1", &b.clear_total.to_string());
        }
    }

    pub fn refresh(&mut self, ui: &mut Ui, archive: &Archive, data: &EntityData) {
        let Some(b) = &data.build else { return };
        if !self.filled {
            self.filled = true;
            self.fill(ui, archive, data, b);
        }
        if let Some(p) = &b.build {
            if p.work <= p.progress {
                return;
            }
        }
        let removed = b.clear_total - b.clear_remaining;
        if let Some((id, t)) = &self.cleared {
            ui.set_text(*id, &t.replace("@0", &removed.to_string()));
        }
        let (total, done) = match &b.build {
            None => (b.clear_total, removed),
            Some(p) => {
                let n = self.rows.len().min(p.materials.len());
                let (mut delivered, mut required) = (0, 0);
                for (j, &(_, need, have)) in p.materials.iter().take(n).enumerate() {
                    if let Some((id, t)) = &self.rows[j].1 {
                        ui.set_text(*id, &t.replace("@0", &have.to_string()));
                    }
                    delivered += have;
                    required += need;
                }
                if let Some((_, Some((id, t)))) = self.rows.get(n) {
                    ui.set_text(*id, &t.replace("@0", &p.progress.to_string()));
                }
                (
                    p.work + b.clear_total + required,
                    p.progress + delivered + removed,
                )
            }
        };
        let fill = if total as f32 != 0.0 {
            done as f32 / total as f32
        } else {
            0.0
        };
        if let Some(p) = self.progress {
            ui.set_value(p, fill);
        }
        if let Some((id, t)) = &self.progress_text {
            let percent = (fill * 100.0 + 0.5) as i32;
            ui.set_text(*id, &t.replace("@0", &percent.to_string()));
        }
    }

    /// Element 6000: `SetEnabled(+0x50 == 0)`.
    pub fn click(&self, id: NodeId, data: &EntityData) -> Option<WorkPlaceClick> {
        use unbanished_game::work::professions::Command;
        let b = data.build.as_ref()?;
        let enabled = b.enabled?;
        (Some(id) == self.pause).then_some(WorkPlaceClick::Command(Command::SetEnabled {
            entity: b.entity,
            enabled: !enabled,
        }))
    }
}
