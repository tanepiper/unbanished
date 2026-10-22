//! Reading and setting the menu dialogs' text, combos and edits.

use super::*;

/// `UI::TextElement::SetParameter`: replace `@0`, `@1`, … in a text element.
pub(super) fn set_parameters(ui: &mut Ui, id: NodeId, values: &[&str]) {
    let text = match &ui.node(id).kind {
        Kind::Text(t) | Kind::Label { text: t, .. } => t.text.clone(),
        _ => return,
    };
    let mut out = text;
    for (i, v) in values.iter().enumerate() {
        out = out.replace(&format!("@{i}"), v);
    }
    ui.set_text(id, &out);
}

pub(crate) fn combo_list(ui: &Ui, combo: NodeId) -> Option<NodeId> {
    match ui.node(combo).kind {
        Kind::Control(Control::Combo { list, .. }) => Some(list),
        _ => None,
    }
}

pub(super) fn combo_index(ui: &Ui, combo: NodeId) -> usize {
    combo_list(ui, combo)
        .and_then(|l| match &ui.node(l).kind {
            Kind::List(data) => data.selected,
            _ => None,
        })
        .unwrap_or(0)
}

/// `ComboElement::SetCurrentItem`: select an item and show its text on the button.
pub(crate) fn select_combo(ui: &mut Ui, combo: NodeId, index: usize) {
    ui.select_combo_item(combo, index);
}

pub(super) fn edit_text(ui: &Ui, edit: NodeId) -> Option<String> {
    match &ui.node(edit).kind {
        Kind::Control(Control::Edit(e)) => Some(e.text.text.clone()),
        _ => None,
    }
}
