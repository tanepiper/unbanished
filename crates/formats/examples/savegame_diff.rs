//! Compare two saves' game state (analysis helper): clocks, Random, and object counts by type.
//! `cargo run -p banished-formats --example savegame_diff -- A.sav B.sav`
use banished_formats::save::Save;
use banished_formats::savegame::{GameState, Value};
use std::collections::BTreeMap;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let load = |p: &str| {
        let save = Save::parse(&std::fs::read(p).unwrap()).unwrap();
        (
            save.metadata().unwrap(),
            GameState::decode(&save.state).unwrap(),
        )
    };
    let (ma, a) = load(&args[0]);
    let (mb, b) = load(&args[1]);
    println!(
        "names {:?} / {:?}; mods {} / {}",
        ma.name,
        mb.name,
        ma.mods.len(),
        mb.mods.len()
    );
    let cal = |g: &GameState| {
        let t = match g.managers.at("calendar.time") {
            Some(Value::I64(t)) => *t,
            _ => 0,
        };
        (g.managers.i32("calendar.value"), t)
    };
    println!("calendar {:?} / {:?}", cal(&a), cal(&b));
    println!(
        "entity clock {} / {} (delta {})",
        a.entity_time,
        b.entity_time,
        b.entity_time - a.entity_time
    );
    println!(
        "random index {} / {}; seed {} / {}",
        a.random_index, b.random_index, a.seed, b.seed
    );
    println!("entities {} / {}", a.entities.len(), b.entities.len());
    let count = |g: &GameState| {
        let mut m: BTreeMap<&str, i64> = BTreeMap::new();
        for o in g.objects.values() {
            *m.entry(o.type_name).or_default() += 1;
        }
        m
    };
    let (ca, cb) = (count(&a), count(&b));
    let mut keys: Vec<_> = ca.keys().chain(cb.keys()).collect();
    keys.sort();
    keys.dedup();
    for k in keys {
        let (x, y) = (
            ca.get(k).copied().unwrap_or(0),
            cb.get(k).copied().unwrap_or(0),
        );
        if x != y {
            println!("  {k}: {x} -> {y}");
        }
    }
    // Entities by template, where the counts differ.
    let templates = |g: &GameState| {
        let mut m: BTreeMap<String, i64> = BTreeMap::new();
        for e in g.entities.iter().flatten() {
            let id = g.objects[e].fields.i32("entity.template").unwrap_or(-1) as u32;
            *m.entry(g.references.get(&id).cloned().unwrap_or_default())
                .or_default() += 1;
        }
        m
    };
    let (ta, tb) = (templates(&a), templates(&b));
    let mut keys: Vec<_> = ta.keys().chain(tb.keys()).cloned().collect();
    keys.sort();
    keys.dedup();
    for k in keys {
        let (x, y) = (
            ta.get(&k).copied().unwrap_or(0),
            tb.get(&k).copied().unwrap_or(0),
        );
        if x != y {
            println!("  entity {k}: {x} -> {y}");
        }
    }
}
