//! List a save's job-manager works: id, type, entity's template, priority (analysis helper).
//! `cargo run -p banished-formats --example savegame_works -- IN.sav`
use banished_formats::save::Save;
use banished_formats::savegame::{GameState, Value};

fn main() {
    let path = std::env::args().nth(1).unwrap();
    let g = GameState::decode(&Save::parse(&std::fs::read(path).unwrap()).unwrap().state).unwrap();
    let template = |e: Option<i32>| {
        e.and_then(|e| g.objects.get(&e))
            .and_then(|o| o.fields.i32("entity.template"))
            .and_then(|t| g.references.get(&(t as u32)))
            .cloned()
            .unwrap_or_default()
    };
    if let Some(Value::List(works)) = g.managers.at("jobs.works") {
        for w in works {
            let o = &g.objects[&w.ptr("work").unwrap()];
            println!(
                "key {:?} id {:?} priority {:?} {} {}",
                w.i32("key"),
                o.fields.i32("work.id"),
                o.fields.i32("work.priority"),
                o.type_name,
                template(o.fields.ptr("work.entity"))
            );
        }
    }
    println!("next id {:?}", g.managers.i32("jobs.next_id"));
    if std::env::var("STACKS").is_ok() {
        stacks(&g);
    }
    // Each entity's WorkComponent `+0x28` and the works owned by it, where they differ.
    let mut owned: std::collections::BTreeMap<i32, Vec<(i32, &str)>> = Default::default();
    if let Some(Value::List(works)) = g.managers.at("jobs.works") {
        for w in works {
            let o = &g.objects[&w.ptr("work").unwrap()];
            if let Some(e) = o.fields.ptr("work.entity") {
                owned
                    .entry(e)
                    .or_default()
                    .push((o.fields.i32("work.id").unwrap(), o.type_name));
            }
        }
    }
    for e in g.entities.iter().flatten() {
        let ent = &g.objects[e];
        let Some(Value::List(cs)) = ent.fields.at("entity.components") else {
            continue;
        };
        for c in cs.iter().filter_map(|c| c.ptr("component")) {
            let c = &g.objects[&c];
            if c.type_name != "WorkComponent" {
                continue;
            }
            let id = c.fields.i32("work.work_id").unwrap();
            let mine = owned.get(e).cloned().unwrap_or_default();
            if mine.iter().map(|m| m.0).collect::<Vec<_>>()
                != (if id >= 0 { vec![id] } else { vec![] })
            {
                println!(
                    "entity {e} {}: +0x28 {id}, owns {mine:?}",
                    template(Some(*e))
                );
            }
        }
    }
}

/// Each character's saved stack: top index, the actions in the array, the "on empty" action.
#[allow(dead_code)]
fn stacks(g: &GameState) {
    for o in g
        .objects
        .values()
        .filter(|o| o.type_name == "CharacterComponent")
    {
        let names: Vec<String> = match o.fields.at("character.stack") {
            Some(Value::List(items)) => items
                .iter()
                .map(|a| {
                    a.ptr("action")
                        .and_then(|a| g.objects.get(&a))
                        .map_or("-".into(), |a| {
                            let started = match a.fields.at("action.started") {
                                Some(Value::Bool(true)) => "*",
                                _ => "",
                            };
                            format!("{}{started}", a.type_name)
                        })
                })
                .collect(),
            _ => Vec::new(),
        };
        let on_empty = o
            .fields
            .ptr("character.on_empty")
            .and_then(|a| g.objects.get(&a))
            .map(|a| a.type_name);
        println!(
            "stack top {:?} {names:?} on_empty {on_empty:?}",
            o.fields.i32("character.stack_top")
        );
    }
}
