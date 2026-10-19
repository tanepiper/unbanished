//! Town-wide controls: immigration, the control buttons and resource limits.

use super::*;

/// `ImmigrationUI` (type `0xff9794df`): bind `FUN_14006bc90`, refresh `FUN_14006ba90`.
/// While nomads wait (state 0x11) `textNomad` shows how many (`@0`) and the town (`@1`) and
/// `buttonAllow` (id 39000) and `buttonDeny` (`0x9859`) work; else `textNone`.
pub struct ImmigrationUi {
    none: Option<NodeId>,
    nomad: Option<(NodeId, String)>,
    allow: Option<NodeId>,
    deny: Option<NodeId>,
}

/// `ControlUI` (type `0xb998cb9f`): bind `FUN_140060310`, refresh `FUN_1400601d0`, events
/// `FUN_140060150`. The element is a check button (id 50000); ControlUIConfig `+0x20` is the
/// control and `+0x24` the component's type id.
pub struct ControlUi {
    button: NodeId,
    index: i32,
    kind: u32,
}

/// `ControlUIConfig`: `+0x20` the control, `+0x24` the component's type id.
pub(super) fn control_config(data: &[u8]) -> Option<(i32, u32)> {
    use banished_formats::crs;
    crs::parse(data, |r: &mut crs::Reader, _| {
        r.block(|r, _| Ok((r.i32()?, r.u32()?)))
    })
    .ok()
    .map(|(_, v)| v)
}

/// `ResourceLimitUI` (vtable of type `0xf77895ff`): bind `FUN_14007ce40`, refresh
/// `FUN_14007cda0`, events `FUN_14007ccb0`. ResourceLimitUIConfig `+0x20` names the stock
/// slot whose limit the `editLimit` spinner (id `0x3039`, 0 to 999999) shows and sets.
pub struct ResourceLimitUi {
    spinner: Option<NodeId>,
    slot: usize,
}

/// A spinner's value and its number edit.
pub(crate) fn set_spinner(ui: &mut Ui, id: NodeId, value: i32) {
    // `SpinnerElement::SetValue` → `NumberEditElement::SetValue`: clamped to the range.
    let value = match ui.spinner_range(id) {
        Some((lo, hi)) => value.clamp(lo, hi),
        None => value,
    };
    let number = match &mut ui.node_mut(id).kind {
        Kind::Control(Control::Spinner {
            parts, value: v, ..
        }) => {
            *v = value;
            parts[0]
        }
        _ => None,
    };
    if let Some(n) = number {
        ui.set_text(n, &value.to_string());
    }
}

pub(crate) fn spinner_value(ui: &Ui, id: NodeId) -> Option<i32> {
    match &ui.node(id).kind {
        Kind::Control(Control::Spinner { value, .. }) => Some(*value),
        _ => None,
    }
}

/// `StatusIconUIConfig` (one bool at `+0x20`, `_showOnMainPage`).
pub(super) fn status_icon_config(data: &[u8]) -> Option<bool> {
    use banished_formats::crs;
    crs::parse(data, |r: &mut crs::Reader, _| r.block(|r, _| r.bool()))
        .ok()
        .map(|(_, v)| v)
}

/// `ResourceLimitUIConfig` (one i32, the stock slot, at `+0x20`).
pub(super) fn resource_limit_config(data: &[u8]) -> Option<usize> {
    use banished_formats::crs;
    crs::parse(data, |r: &mut crs::Reader, _| r.block(|r, _| r.i32()))
        .ok()
        .map(|(_, v)| v.clamp(0, 20) as usize)
}

impl ImmigrationUi {
    pub fn bind(ui: &mut Ui, element: NodeId) -> ImmigrationUi {
        ImmigrationUi {
            none: ui.find(element, "textNone"),
            nomad: ui.find(element, "textNomad").map(|n| (n, text_of(ui, n))),
            allow: ui.find(element, "buttonAllow"),
            deny: ui.find(element, "buttonDeny"),
        }
    }

    pub fn refresh(&mut self, ui: &mut Ui, data: &EntityData) {
        let Some((waiting, count)) = data.immigration else {
            return;
        };
        for b in [self.allow, self.deny].into_iter().flatten() {
            ui.node_mut(b).enabled = waiting;
        }
        if let Some((n, template)) = &self.nomad {
            ui.set_visible(*n, waiting);
            ui.set_text(
                *n,
                &template
                    .replace("@0", &count.to_string())
                    .replace("@1", data.town_name),
            );
        }
        if let Some(n) = self.none {
            ui.set_visible(n, !waiting);
        }
    }

    /// `0x9858` (`FUN_14006b9e0`): state message 7, take them in; `0x9859` (`FUN_14006b930`):
    /// message 8, turn them away.
    pub fn click(&self, node: NodeId, entity: usize) -> Option<WorkPlaceClick> {
        let message = if Some(node) == self.allow {
            7
        } else if Some(node) == self.deny {
            8
        } else {
            return None;
        };
        Some(WorkPlaceClick::Command(
            banished_game::work::professions::Command::StateMessage { entity, message },
        ))
    }
}

impl ControlUi {
    pub fn bind(archive: &Archive, element: NodeId, config: Option<&str>) -> Option<Self> {
        let (index, kind) = config
            .and_then(|c| archive.read_ref(&c.replace('\\', "/")).ok())
            .and_then(|d| control_config(&d))?;
        Some(ControlUi {
            button: element,
            index,
            kind,
        })
    }

    /// `FUN_1400601d0`: checked by bit 1, enabled by bit 0.
    pub fn refresh(&mut self, ui: &mut Ui, data: &EntityData) {
        let Some(control) = data.control else { return };
        let bits = control(self.kind, self.index);
        if ui.checked(self.button) != (bits & 2 != 0) {
            ui.set_checked(self.button, bits & 2 != 0);
        }
        ui.node_mut(self.button).enabled = bits & 1 != 0;
    }

    /// `FUN_140060150` (id 50000): the component's slot `+0x70`.
    pub fn click(&self, ui: &Ui, node: NodeId, entity: Option<usize>) -> Option<WorkPlaceClick> {
        if node != self.button && !ui.descendant(self.button, node) {
            return None;
        }
        Some(WorkPlaceClick::Command(
            banished_game::work::professions::Command::Control {
                entity: entity?,
                kind: self.kind,
                index: self.index,
            },
        ))
    }
}

impl ResourceLimitUi {
    pub fn bind(ui: &mut Ui, archive: &Archive, element: NodeId, config: Option<&str>) -> Self {
        let slot = config
            .and_then(|c| archive.read_ref(&c.replace('\\', "/")).ok())
            .and_then(|d| resource_limit_config(&d))
            .unwrap_or(0);
        // `FUN_14007ce40`: `SpinnerElement::SetMinMax(0, 999999)`.
        let spinner = ui.find(element, "editLimit");
        if let Some(s) = spinner {
            ui.set_spinner_range(s, 0, 999_999);
        }
        ResourceLimitUi { spinner, slot }
    }

    /// `FUN_14007cda0`: the slot's limit, unless the player is typing in it.
    pub fn refresh(&mut self, ui: &mut Ui, data: &EntityData) {
        self.refresh_limits(ui, &data.stock_limits);
    }

    /// [`Self::refresh`] from the stock manager's limits (a utility window has no entity).
    pub fn refresh_limits(&self, ui: &mut Ui, limits: &[i32; 21]) {
        let Some(s) = self.spinner else { return };
        let limit = limits[self.slot];
        if spinner_value(ui, s) != Some(limit) && !ui.focused().is_some_and(|f| ui.descendant(s, f))
        {
            set_spinner(ui, s, limit.clamp(0, 999_999));
        }
    }

    /// `FUN_14007ccb0` (event `0x15`, id `0x3039`): the slot's limit is the spinner's.
    pub fn click(&self, ui: &Ui, node: NodeId) -> Option<WorkPlaceClick> {
        let s = self.spinner?;
        if node != s && !ui.descendant(s, node) {
            return None;
        }
        let limit = spinner_value(ui, s)?.clamp(0, 999_999);
        Some(WorkPlaceClick::StockLimit {
            slot: self.slot,
            limit,
        })
    }
}
