//! Reading and setting the dialog's controls: combos, sliders, checks, tabs and strings.

use super::*;

/// `StringTable::GetText(table, index)`: the `index`th entry's text.
pub(super) fn table_text(ui: &Ui, table: &str, index: usize) -> String {
    ui.res
        .archive
        .read_ref(&format!("{STRINGS}:{table}"))
        .ok()
        .and_then(|d| unbanished_formats::ui::string_table(&d).ok())
        .and_then(|t| t.into_iter().nth(index).map(|(_, v)| v))
        .unwrap_or_default()
}

/// `GameGlobal::KeyName` (`FUN_14009f0f0`).
pub(super) fn key_name(ui: &mut Ui, binding: Option<Binding>) -> String {
    let key = keymap::key_name_key(binding);
    ui.res
        .string(Some(r"Dialog\StringTable.rsc:keyNames"), &key)
}

/// The action's name: `gameKeys` entry `id` (`GameGlobal +200`).
pub(super) fn action_name(ui: &Ui, id: usize) -> String {
    table_text(ui, "gameKeys", id)
}

pub(super) fn combo_selected(ui: &Ui, combo: Option<NodeId>) -> Option<usize> {
    let list = combo_list(ui, combo?)?;
    match &ui.node(list).kind {
        Kind::List(l) => l.selected,
        _ => None,
    }
}

/// `ComboElement::RemoveAllItems` + `AddItem`s; returns nothing selected.
pub(super) fn fill_combo(ui: &mut Ui, combo: Option<NodeId>, items: Vec<String>) {
    if let Some(c) = combo {
        ui.set_choices(c, items);
        if let Some(list) = combo_list(ui, c) {
            ui.list_select(list, None);
        }
        ui.set_text(c, "");
    }
}

/// `ComboElement::SetCurrentItem` (−1 or out of range clears the selection).
pub(super) fn set_combo(ui: &mut Ui, combo: Option<NodeId>, index: i32) {
    let Some(c) = combo else { return };
    if index >= 0 {
        select_combo(ui, c, index as usize);
    }
}

pub(super) fn set_slider(ui: &mut Ui, slider: Option<NodeId>, range: [f32; 2], value: f32) {
    if let Some(s) = slider {
        ui.set_slider_range(s, range);
        ui.set_value(s, value);
    }
}

pub(super) fn slider(ui: &Ui, slider: Option<NodeId>) -> f32 {
    slider.and_then(|s| ui.value(s)).unwrap_or(0.0)
}

pub(super) fn check(ui: &Ui, check: Option<NodeId>) -> bool {
    check.is_some_and(|c| ui.checked(c))
}

pub(super) fn set_check(ui: &mut Ui, check: Option<NodeId>, value: bool) {
    if let Some(c) = check {
        ui.set_checked(c, value);
    }
}

pub(super) fn active_tab(ui: &Ui, tabs: Option<NodeId>) -> i32 {
    match tabs.map(|t| &ui.node(t).kind) {
        Some(Kind::Control(Control::Tabs { selected, .. })) => *selected as i32,
        _ => 0,
    }
}

/// `TabElement::SetActiveTab`.
pub(super) fn set_active_tab(ui: &mut Ui, tabs: Option<NodeId>, index: i32) {
    if let Some(Kind::Control(Control::Tabs { tabs, selected, .. })) =
        tabs.map(|t| &mut ui.node_mut(t).kind)
    {
        if index >= 0 && (index as usize) < tabs.len() {
            *selected = index as usize;
        }
    }
}

pub(super) fn slider_value(ui: &Ui, id: NodeId) -> i32 {
    ui.value(id).unwrap_or(0.0) as i32
}
