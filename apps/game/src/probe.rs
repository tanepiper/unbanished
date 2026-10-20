//! The probe: one command language to look at and drive a running game, for tests and scripted
//! runs (not in the original). Headless runs take it with `--probe "COMMAND@SECONDS"`; the
//! browser build with `window.unbanished.probe("COMMAND")`. Every answer is JSON.
//!
//! | Command | Answer |
//! |---|---|
//! | `state` | the current loop (`Game/Menu`, `Game/Game`, …) and the open dialogs |
//! | `ui QUERY` | every shown element matching the query ([`banished_ui::Query`]) |
//! | `click QUERY` | clicks the first usable match through the normal pointer path |
//! | `combo NAME=ITEM` | selects a combo item by its text |
//! | `type NAME=TEXT` | clicks an edit, empties it and types the text |
//! | `town` | the town's numbers: date, population, health, happiness, food, buildings |
//! | `stock` | the Status Bar's stock |
//! | `buildings [FILTER]` | built and unfinished buildings, counted by template |
//! | `citizens [FILTER]` | adults, students, children, and workers by profession |
//! | `animals [FILTER]` | animals by template |
//! | `script COMMAND` | a capture script command (`speed 10`, `pause`, `place TEMPLATE`, …) |
//!
//! Queries only read. `click`, `combo` and `type` act as a player's input does, so a scripted
//! game is the same game a player would get from the same input.

use crate::Game;
use banished_ui::Query;
use serde_json::{json, Value};

/// Run one probe command against `game`, laid out for a window of `size` physical pixels.
pub fn run(game: &mut Game, size: (u32, u32), command: &str) -> Result<Value, String> {
    let command = command.trim();
    let (verb, rest) = command.split_once(' ').unwrap_or((command, ""));
    let rest = rest.trim();
    match verb {
        "state" => Ok(state(game)),
        "ui" => {
            let query = Query::parse(rest)?;
            let found = find(game, size, &query);
            Ok(Value::Array(
                found.into_iter().map(|id| element(game, id)).collect(),
            ))
        }
        "click" => {
            let query = Query::parse(rest)?;
            let found = find(game, size, &query);
            let Some(&id) = found.iter().find(|&&id| game.ui.node(id).usable()) else {
                return Err(if found.is_empty() {
                    format!("click {rest}: nothing matches")
                } else {
                    format!("click {rest}: {} match, none usable", found.len())
                });
            };
            let info = element(game, id);
            game.press_node(id);
            Ok(info)
        }
        "combo" => {
            let (name, item) = rest.split_once('=').ok_or("combo NAME=ITEM")?;
            game.select_combo(name.trim(), item.trim(), size)
                .then_some(json!({ "selected": item.trim() }))
                .ok_or_else(|| format!("combo {rest}: not found"))
        }
        "type" => {
            let (name, text) = rest.split_once('=').ok_or("type NAME=TEXT")?;
            game.type_into(name.trim(), text, size)
                .then_some(json!({ "typed": text }))
                .ok_or_else(|| format!("type {rest}: not found"))
        }
        "script" => {
            if game.script(rest) {
                Ok(json!({ "done": rest }))
            } else {
                Err(format!(
                    "script {rest}: not taken by {}",
                    game.main.current().name()
                ))
            }
        }
        "town" | "stock" | "buildings" | "citizens" | "animals" => {
            let filter = (!rest.is_empty()).then_some(rest);
            game.main
                .current()
                .probe_world(verb, filter)
                .unwrap_or_else(|| {
                    Err(format!("{verb}: no town in {}", game.main.current().name()))
                })
        }
        _ => Err(format!("unknown probe command {verb:?}")),
    }
}

/// The current loop and its open dialogs, top first.
fn state(game: &Game) -> Value {
    let current = game.main.current();
    let dialogs: Vec<Value> = current.screen().map_or(Vec::new(), |s| {
        s.dialogs
            .iter()
            .rev()
            .map(|d| json!(game.ui.node(d.root).reference))
            .collect()
    });
    json!({ "loop": current.name(), "dialogs": dialogs, "quit": game.main.quit })
}

/// The shown elements matching `query` in the current screen's dialogs, top dialog first.
fn find(game: &mut Game, size: (u32, u32), query: &Query) -> Vec<banished_ui::NodeId> {
    let size = game.logical(size);
    let Some(screen) = game.main.current().screen() else {
        return Vec::new();
    };
    screen.layout(&mut game.ui, size);
    let roots: Vec<_> = screen.dialogs.iter().rev().map(|d| d.root).collect();
    game.ui.query(&roots, query)
}

fn element(game: &Game, id: banished_ui::NodeId) -> Value {
    let i = game.ui.info(id);
    let mut v = json!({
        "name": i.name,
        "kind": i.kind,
        "text": i.text,
        "ref": i.reference,
        "rect": i.rect,
        "enabled": i.enabled,
    });
    if let Some(c) = i.checked {
        v["checked"] = json!(c);
    }
    if let Some(x) = i.value {
        v["value"] = json!(x);
    }
    v
}

/// The browser's probe queue: the page asks with [`probe`], the game answers between frames
/// ([`answer_pending`]), and the page collects the answer with [`probe_result`].
#[cfg(target_arch = "wasm32")]
pub mod web {
    use std::cell::RefCell;
    use std::collections::{HashMap, VecDeque};
    use wasm_bindgen::prelude::*;

    thread_local! {
        static NEXT: RefCell<u32> = const { RefCell::new(1) };
        static PENDING: RefCell<VecDeque<(u32, String)>> = const { RefCell::new(VecDeque::new()) };
        static DONE: RefCell<HashMap<u32, String>> = RefCell::new(HashMap::new());
    }

    /// Queue a probe command; returns its number for [`probe_result`].
    #[wasm_bindgen(js_name = unbanishedProbe)]
    pub fn probe(command: String) -> u32 {
        let id = NEXT.with(|n| {
            let mut n = n.borrow_mut();
            *n += 1;
            *n - 1
        });
        PENDING.with(|p| p.borrow_mut().push_back((id, command)));
        id
    }

    /// The answer to probe `id` as JSON text (`{"ok":…}` or `{"error":…}`), once it is ready.
    #[wasm_bindgen(js_name = unbanishedProbeResult)]
    pub fn probe_result(id: u32) -> Option<String> {
        DONE.with(|d| d.borrow_mut().remove(&id))
    }

    /// Answer every queued probe (the frame loop calls this between frames).
    pub fn answer_pending(game: &mut crate::Game, size: (u32, u32)) {
        while let Some((id, command)) = PENDING.with(|p| p.borrow_mut().pop_front()) {
            let answer = match super::run(game, size, &command) {
                Ok(v) => serde_json::json!({ "ok": v }),
                Err(e) => serde_json::json!({ "error": e }),
            };
            DONE.with(|d| d.borrow_mut().insert(id, answer.to_string()));
        }
    }
}
