//! Integration coverage using the player's packages; skipped when no game install is present.
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::rc::Rc;

use unbanished_formats::pkg::{Archive, Package};
use unbanished_formats::{crs, ui as format};
use unbanished_ui::{Control, Event, Key, Kind, Node, NodeId, Ui};

mod dialogs;
mod elements;
mod input;

fn game_ui() -> Option<Ui> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../game-data/original/WinData");
    if !dir.join("data0.pkg").exists() {
        eprintln!("game packages absent; skipping controls integration tests");
        return None;
    }
    let mut archive = Archive::default();
    for name in ["data1.pkg", "patch1.pkg", "patch0.pkg", "data0.pkg"] {
        archive.add(Package::parse(std::fs::read(dir.join(name)).unwrap()).unwrap());
    }
    Some(Ui::new(Rc::new(archive)))
}

fn click(ui: &mut Ui, root: NodeId, id: NodeId) {
    let [l, t, r, b] = ui.nodes[id].rect;
    assert!(
        r > l && b > t,
        "{} has no clickable area",
        ui.nodes[id].name
    );
    ui.pointer_move((l + r) / 2, (t + b) / 2, root);
    ui.pointer_button(true, root);
    ui.pointer_button(false, root);
}
