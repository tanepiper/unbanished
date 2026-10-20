//! Dialog instances resolve named template substitutions and controller attachment points.
//! Controller type IDs describe application code, so callers register their own handlers.
use crate::*;
use std::collections::HashMap;

#[derive(Debug, Clone)]
/// A controller attached to an element of a dialog.
pub struct Binding {
    /// The controller's type id (CRC-32 of its class name).
    pub controller: u32,
    /// The controller's configuration resource, if it has one.
    pub config: Option<String>,
    /// The element it is attached to.
    pub element: NodeId,
    /// The element's name.
    pub name: String,
}

#[derive(Debug, Clone)]
/// A dialog made from its resource: the element tree and its controllers.
pub struct DialogInstance {
    /// The root element: lay it out and draw it.
    pub root: NodeId,
    /// The controllers attached to its elements.
    pub bindings: Vec<Binding>,
    /// The dialogs it was built from: the dialog and those it includes.
    pub dialogs: Vec<String>,
    /// The background sprite drawn behind the dialog, if any.
    pub background: Option<SpriteRef>,
}

impl DialogInstance {
    /// Deliver an event to each controller attached to the element or one of its ancestors.
    /// The application implements the game-specific controller using its type ID and config.
    pub fn dispatch(&self, ui: &Ui, event: Event, mut handler: impl FnMut(&Binding, Event)) {
        let source = input::event_node(event);
        for b in &self.bindings {
            if ui.descendant(b.element, source) {
                handler(b, event);
            }
        }
    }
}

impl Ui {
    /// Build a dialog from its resource, for example `Dialog/MainMenu.rsc:resource`.
    pub fn instantiate_dialog(&mut self, reference: &str) -> Option<DialogInstance> {
        let d = unbanished_formats::ui::dialog(&self.res.archive.read_ref(reference).ok()?).ok()?;
        // Ghidra Dialog::CreateElement (0x180115760): match the requested path identifier
        // against the named list; a null replacement suppresses that slot.
        let overrides: HashMap<_, _> = d
            .named
            .iter()
            .map(|(r, name)| (name.clone(), r.clone()))
            .collect();
        self.overrides.push(overrides);
        let root = d.root.as_deref().and_then(|r| self.instantiate(r));
        self.overrides.pop();
        let root = root?;
        for (name, key) in [("buttonOk", 13), ("buttonCancel", 27)] {
            if let Some(id) = self.find(root, name) {
                self.set_shortcut(id, key);
            }
        }
        let mut bindings = Vec::new();
        for b in d.bindings {
            let element = if b.name.is_empty() {
                Some(root)
            } else {
                self.find(root, &b.name)
            };
            let Some(element) = element else {
                log::warn!("unresolved dialog controller host: {}", b.name);
                continue;
            };
            // Ghidra AddController (0x180115d00) optionally creates an element description
            // under the named host before attaching its controller.
            let attached = if let Some(reference) = b.source.as_deref() {
                if let Some(c) = self.instantiate(reference) {
                    self.add_child(element, c);
                    c
                } else {
                    element
                }
            } else {
                element
            };
            bindings.push(Binding {
                controller: b.unknown,
                config: b.target,
                element: attached,
                name: b.name,
            });
        }
        let background = self
            .res
            .sprite(d.background_sheet.as_deref(), &d.background_sprite);
        Some(DialogInstance {
            root,
            bindings,
            dialogs: d.dialogs.into_iter().flatten().collect(),
            background,
        })
    }

    /// Whether `node` is `root` or inside it.
    pub fn descendant(&self, root: NodeId, node: NodeId) -> bool {
        root == node
            || self
                .child_ids(root)
                .into_iter()
                .any(|child| self.descendant(child, node))
    }
}
