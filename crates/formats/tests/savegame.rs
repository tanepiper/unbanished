//! Full decoding of real saves (`save-game/*.sav`, local only): every block is consumed exactly,
//! and the result agrees with the older scanning parsers in `save.rs`.

use banished_formats::save::Save;
use banished_formats::savegame::{Fields, GameState, ObjId, Value};
use std::collections::BTreeSet;

fn ptrs(fields: &Fields, out: &mut Vec<ObjId>) {
    for (_, v) in &fields.0 {
        value_ptrs(v, out);
    }
}

fn value_ptrs(v: &Value, out: &mut Vec<ObjId>) {
    match v {
        Value::Ptr(Some(p)) => out.push(*p),
        Value::List(items) => items.iter().for_each(|f| ptrs(f, out)),
        Value::Block(_, f) => ptrs(f, out),
        _ => {}
    }
}

fn list_len(fields: &Fields, path: &str) -> usize {
    match fields.at(path) {
        Some(Value::List(items)) => items.len(),
        other => panic!("{path} is {other:?}"),
    }
}

#[test]
fn saves_decode_fully() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../save-game");
    let Ok(entries) = std::fs::read_dir(dir) else {
        eprintln!("no saves; skipping");
        return;
    };
    let mut decoded = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|e| e != "sav") {
            continue;
        }
        let save = Save::parse(&std::fs::read(&path).unwrap()).unwrap();
        let g = GameState::decode(&save.state).unwrap_or_else(|e| panic!("{path:?}: {e}"));
        decoded += 1;

        assert_eq!(g.seed, save.loop_header().unwrap().seed);
        assert_eq!(
            g.terrain_map.as_ref().unwrap().heights,
            save.map_heights().unwrap().heights
        );

        // The map cells against the pattern-located reader.
        let cells = save.map_cells().unwrap();
        assert_eq!(g.map.size as usize, cells.size);
        assert_eq!(g.map.trailer, cells.trailer);
        assert_eq!(g.map.cells.len(), cells.cells.len());
        for (a, b) in g.map.cells.iter().zip(&cells.cells) {
            assert_eq!(a.entities.map(|e| e.is_some()), b.objects);
            assert_eq!(
                (a.description, a.flags, a.value, a.bytes),
                (b.external, b.flags, b.value, b.bytes)
            );
        }

        // The event log against the block scanner.
        if let Some(log) = save.event_log() {
            let entries = match g.managers.at("event_log.entries") {
                Some(Value::List(items)) => items,
                other => panic!("event log is {other:?}"),
            };
            assert_eq!(entries.len(), log.entries.len());
            for (a, b) in entries.iter().zip(&log.entries) {
                assert_eq!(a.get("text"), Some(&Value::Str(b.text.clone())));
                assert_eq!(a.i32("cell"), Some(b.cell));
            }
        }

        // The town's registries hold every citizen, and every pointer anywhere is defined.
        let citizens = g
            .objects
            .values()
            .filter(|o| o.type_name == "CitizenComponent")
            .count();
        assert_eq!(list_len(&g.town, "town.citizens"), citizens, "{path:?}");
        let mut refs = Vec::new();
        ptrs(&g.town, &mut refs);
        ptrs(&g.managers, &mut refs);
        for o in g.objects.values() {
            ptrs(&o.fields, &mut refs);
        }
        refs.extend(g.entities.iter().flatten());
        refs.extend(g.map.cells.iter().flat_map(|c| c.entities.iter().flatten()));
        let defined: BTreeSet<_> = g.objects.keys().collect();
        let missing: BTreeSet<_> = refs.iter().filter(|r| !defined.contains(r)).collect();
        assert!(
            missing.is_empty(),
            "{path:?}: undefined objects {missing:?}"
        );
    }
    eprintln!("decoded {decoded} saves");
}
