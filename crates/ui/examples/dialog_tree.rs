//! Prints a dialog resource's element tree: each element's name, kind and text, indented by
//! depth, to find the parts a window can reuse.
//!
//! ```text
//! cargo run --release -p unbanished-ui --example dialog_tree -- 'Dialog/StandardDialog.rsc' [WINDATA]
//! ```

use std::rc::Rc;

use unbanished_formats::pkg::{Archive, Package};
use unbanished_ui::{Kind, NodeId, Ui};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let reference = args
        .first()
        .expect("a dialog reference, e.g. Dialog/StandardDialog.rsc");
    let dir = args
        .get(1)
        .cloned()
        .unwrap_or_else(|| "game-data/original/WinData".into());
    let mut archive = Archive::default();
    for name in ["data1.pkg", "patch1.pkg", "patch0.pkg", "data0.pkg"] {
        if let Ok(bytes) = std::fs::read(format!("{dir}/{name}")) {
            archive.add(Package::parse(bytes).expect("a package"));
        }
    }
    let mut ui = Ui::new(Rc::new(archive));
    let root = ui
        .instantiate(reference)
        .or_else(|| ui.instantiate_dialog(reference).map(|d| d.root));
    let Some(root) = root else {
        eprintln!("{reference}: not found in {dir}");
        std::process::exit(1);
    };
    print(&ui, root, 0);
}

fn print(ui: &Ui, id: NodeId, depth: usize) {
    let node = ui.node(id);
    let (kind, text) = match &node.kind {
        Kind::Group => ("group", String::new()),
        Kind::Ribbon { vertical, .. } => (if *vertical { "column" } else { "row" }, String::new()),
        Kind::Border { .. } => ("border", String::new()),
        Kind::Image { .. } => ("image", String::new()),
        Kind::Button { .. } => ("button", String::new()),
        Kind::Label { text, .. } => ("label", text.text.clone()),
        Kind::Text(t) => ("text", t.text.clone()),
        Kind::Control(c) => ("control", format!("{c:?}").chars().take(40).collect()),
        _ => ("other", String::new()),
    };
    println!(
        "{:indent$}{} [{kind}] {}",
        "",
        node.name,
        text.replace('\n', " "),
        indent = depth * 2
    );
    for c in ui.children(id) {
        print(ui, c, depth + 1);
    }
}
