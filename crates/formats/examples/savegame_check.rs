//! Decode saves' game state and report the first failure (analysis helper).
fn main() {
    for path in std::env::args().skip(1) {
        let save = banished_formats::save::Save::parse(&std::fs::read(&path).unwrap()).unwrap();
        // `SAVE_STATE_DIR`: also write the decompressed game state there.
        if let Ok(dir) = std::env::var("SAVE_STATE_DIR") {
            let name = std::path::Path::new(&path).file_stem().unwrap();
            std::fs::write(std::path::Path::new(&dir).join(name), &save.state).unwrap();
        }
        match banished_formats::savegame::GameState::decode(&save.state) {
            Ok(g) => {
                if let Ok(dir) = std::env::var("SAVE_STATE_DIR") {
                    let name = std::path::Path::new(&path).file_stem().unwrap();
                    let out = std::path::Path::new(&dir)
                        .join(name)
                        .with_extension("written");
                    let state = g.encode().unwrap();
                    let file =
                        banished_formats::save::Save::write(&save.metadata().unwrap(), &state);
                    println!("{path}: rewritten file {} bytes", file.len());
                    std::fs::write(out.with_extension("sav"), file).unwrap();
                    std::fs::write(out, state).unwrap();
                }
                let mut kinds = std::collections::BTreeMap::new();
                for o in g.objects.values() {
                    *kinds.entry(o.type_name).or_insert(0) += 1;
                }
                let mut roads = std::collections::BTreeMap::new();
                for c in &g.map.cells {
                    *roads.entry((c.bytes[2], c.bytes[3])).or_insert(0) += 1;
                }
                println!("road cells (kind, built): {roads:?}");
                // `SAVE_DUMP_ENTITIES=n`: the first n entities with their components' fields.
                let dump: usize = std::env::var("SAVE_DUMP_ENTITIES")
                    .ok()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0);
                for e in g.entities.iter().flatten().take(dump) {
                    let o = &g.objects[e];
                    let template = o.fields.i32("entity.template").unwrap_or(-1) as u32;
                    println!("entity {e} {:?}", g.references.get(&template));
                    if let Some(banished_formats::savegame::Value::List(cs)) =
                        o.fields.at("entity.components")
                    {
                        for c in cs.iter().filter_map(|c| c.ptr("component")) {
                            let c = &g.objects[&c];
                            println!("  {} {:?}", c.type_name, c.fields.0.get(1));
                        }
                    }
                }
                println!(
                    "{path}: ok, {} entities, {} objects, town {:?}: {kinds:?}",
                    g.entities.len(),
                    g.objects.len(),
                    g.town_name
                );
            }
            Err(e) => println!("{path}: {e}"),
        }
    }
}
