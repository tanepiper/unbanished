//! Finding elements by a short query, for tests and scripted runs (not in the original).
//!
//! A query is a list of steps separated by spaces. Each step finds elements inside the elements
//! the step before it found (anywhere below them, not only their children). A step is one of:
//!
//! | Step | Finds |
//! |---|---|
//! | `buttonExit` | the element with this name in its resource |
//! | `text:Exit` or `text:"New Game"` | the element whose text is this (letter case ignored) |
//! | `kind:button` | elements of this kind (see [`Ui::kind_name`]) |
//! | `ref:PauseMenu` | elements whose resource reference contains this (letter case ignored) |
//! | `*` | any element |
//!
//! Only shown elements count: a hidden element and everything inside it are skipped. For
//! example, `ref:Dialog/Options buttonVideo` is the Video tab of the Options dialog.

use crate::{Control, Kind, NodeId, Ui};

/// One step of a query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// An element name, compared exactly.
    Name(String),
    /// The element's text, letter case ignored.
    Text(String),
    /// The element's kind ([`Ui::kind_name`]).
    Kind(String),
    /// Part of the element's resource reference, letter case ignored.
    Reference(String),
    /// Any element.
    Any,
}

/// A parsed query: its steps, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Query(pub Vec<Step>);

impl Query {
    /// Parse a query. Text in double quotes may contain spaces. An empty query is an error.
    pub fn parse(text: &str) -> Result<Query, String> {
        let mut steps = Vec::new();
        let mut chars = text.trim().chars().peekable();
        while chars.peek().is_some() {
            let mut word = String::new();
            let mut quoted = false;
            while let Some(&c) = chars.peek() {
                chars.next();
                if c == '"' {
                    quoted = !quoted;
                } else if c.is_whitespace() && !quoted {
                    break;
                } else {
                    word.push(c);
                }
            }
            if quoted {
                return Err(format!("unclosed quote in {text:?}"));
            }
            while chars.peek().is_some_and(|c| c.is_whitespace()) {
                chars.next();
            }
            if word.is_empty() {
                continue;
            }
            steps.push(match word.split_once(':') {
                Some(("text", t)) => Step::Text(t.to_string()),
                Some(("kind", k)) => Step::Kind(k.to_ascii_lowercase()),
                Some(("ref", r)) => Step::Reference(r.to_string()),
                Some((other, _)) if !other.is_empty() && !other.contains('/') => {
                    return Err(format!("unknown step {word:?} (use text:, kind: or ref:)"));
                }
                _ if word == "*" => Step::Any,
                _ => Step::Name(word),
            });
        }
        if steps.is_empty() {
            return Err("empty query".into());
        }
        Ok(Query(steps))
    }
}

/// What a query reports about one element.
#[derive(Debug, Clone, PartialEq)]
pub struct ElementInfo {
    /// The element.
    pub id: NodeId,
    /// Its name in its resource.
    pub name: String,
    /// The resource it was made from.
    pub reference: String,
    /// Its kind ([`Ui::kind_name`]).
    pub kind: &'static str,
    /// Its text, or its label's text for a button; empty when it has none.
    pub text: String,
    /// Its rectangle after the last layout: left, top, right, bottom.
    pub rect: [i32; 4],
    /// Whether it can be used (enabled and not locked).
    pub enabled: bool,
    /// For a check box: whether it is checked.
    pub checked: Option<bool>,
    /// For a slider, scroll bar or progress bar: its value.
    pub value: Option<f32>,
}

impl Ui {
    /// Every shown element below `roots` (and the roots themselves) that matches `query`, in
    /// tree order with duplicates removed.
    pub fn query(&self, roots: &[NodeId], query: &Query) -> Vec<NodeId> {
        let mut found: Vec<NodeId> = roots
            .iter()
            .copied()
            .filter(|&r| self.nodes[r].visible())
            .collect();
        for (i, step) in query.0.iter().enumerate() {
            let mut next = Vec::new();
            for &start in &found {
                // The first step may match a root itself; later steps look strictly inside.
                self.collect(start, step, i == 0, &mut next);
            }
            let mut seen = std::collections::BTreeSet::new();
            next.retain(|id| seen.insert(*id));
            found = next;
        }
        found
    }

    fn collect(&self, id: NodeId, step: &Step, include_self: bool, out: &mut Vec<NodeId>) {
        if !self.nodes[id].visible() {
            return;
        }
        if include_self && self.matches(id, step) {
            out.push(id);
        }
        for c in self.child_ids(id) {
            self.collect(c, step, true, out);
        }
    }

    fn matches(&self, id: NodeId, step: &Step) -> bool {
        let n = &self.nodes[id];
        match step {
            Step::Name(name) => &n.name == name,
            Step::Text(text) => !text.is_empty() && self.text_of(id).eq_ignore_ascii_case(text),
            Step::Kind(kind) => self.kind_name(id) == kind,
            Step::Reference(r) => n
                .reference
                .to_ascii_lowercase()
                .contains(&r.to_ascii_lowercase()),
            Step::Any => true,
        }
    }

    /// A short lower-case name for an element's kind: `group`, `ribbon`, `border`, `image`,
    /// `animation`, `button`, `text`, `label`, `viewport`, `list`, `lines`, `picture`,
    /// `unsupported`, or for a control `check`, `slider`, `scroll`, `edit`, `combo`, and so on.
    pub fn kind_name(&self, id: NodeId) -> &'static str {
        match &self.nodes[id].kind {
            Kind::Group => "group",
            Kind::Ribbon { .. } => "ribbon",
            Kind::Border { .. } => "border",
            Kind::Image { .. } => "image",
            Kind::AnimatedImage { .. } => "animation",
            Kind::Button { .. } => "button",
            Kind::Text(_) => "text",
            Kind::Label { .. } => "label",
            Kind::Viewport => "viewport",
            Kind::List(_) => "list",
            Kind::Lines { .. } => "lines",
            Kind::Picture { .. } => "picture",
            Kind::Unsupported(_) => "unsupported",
            Kind::Control(c) => control_name(c),
        }
    }

    /// An element's text: its own for text, labels and edits, its content's for a button.
    pub fn text_of(&self, id: NodeId) -> String {
        match &self.nodes[id].kind {
            Kind::Text(t) => t.text.clone(),
            Kind::Label { text, .. } => text.text.clone(),
            Kind::Button { content, .. } => content.map(|c| self.text_of(c)).unwrap_or_default(),
            Kind::Control(Control::Edit(e)) => e.text.text.clone(),
            _ => String::new(),
        }
    }

    /// What a query reports about `id`.
    pub fn info(&self, id: NodeId) -> ElementInfo {
        let n = &self.nodes[id];
        let control = matches!(n.kind, Kind::Control(_));
        ElementInfo {
            id,
            name: n.name.clone(),
            reference: n.reference.clone(),
            kind: self.kind_name(id),
            text: self.text_of(id),
            rect: n.rect,
            enabled: n.usable(),
            checked: matches!(n.kind, Kind::Control(Control::Check { .. }))
                .then(|| self.checked(id)),
            value: if control { self.value(id) } else { None },
        }
    }
}

/// The lower-case name of a control's kind.
fn control_name(c: &Control) -> &'static str {
    match c {
        Control::Slider(_) => "slider",
        Control::Scroll { .. } => "scroll",
        Control::Progress { .. } => "progress",
        Control::Edit(_) => "edit",
        Control::Graph { .. } => "graph",
        Control::Table { .. } => "table",
        Control::Layout(_) => "layout",
        Control::Tabs { .. } => "tabs",
        Control::Clip { .. } => "clip",
        Control::Container { .. } => "container",
        Control::Check { .. } => "check",
        Control::Combo { .. } => "combo",
        Control::Spinner { .. } => "spinner",
        Control::Page { .. } => "page",
        Control::Radio { .. } => "radio",
        Control::Document { .. } => "document",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_names_kinds_text_and_references() {
        let q =
            Query::parse(r#"ref:Dialog/Options  kind:button text:"New Game" buttonOk *"#).unwrap();
        assert_eq!(
            q.0,
            vec![
                Step::Reference("Dialog/Options".into()),
                Step::Kind("button".into()),
                Step::Text("New Game".into()),
                Step::Name("buttonOk".into()),
                Step::Any,
            ]
        );
    }

    #[test]
    fn rejects_bad_queries() {
        assert!(Query::parse("").is_err());
        assert!(Query::parse("text:\"open").is_err());
        assert!(Query::parse("colour:red").is_err());
    }
}
