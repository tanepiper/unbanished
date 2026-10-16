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
