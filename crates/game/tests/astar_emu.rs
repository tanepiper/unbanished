//! `pathfind::Pathfinder::find` against the original A* (`tools/emu/astar.py`; skipped without
//! its output in game-data/derived/emu).

use banished_game::pathfind::{Pathfinder, WALKABLE};
use banished_game::terrain::CellType;

#[test]
fn matches_the_original() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../game-data/derived/emu/astar.tsv"
    );
    let Ok(text) = std::fs::read_to_string(path) else {
        eprintln!("no emulator output; skipping");
        return;
    };
    let mut finder: Option<Pathfinder> = None;
    let (mut searches, mut found) = (0, 0);
    for line in text.lines() {
        let cols: Vec<&str> = line.split('\t').collect();
        if cols[0] == "grid" {
            let bytes: Vec<CellType> = (0..cols[1].len() / 2)
                .map(|i| CellType(u8::from_str_radix(&cols[1][2 * i..2 * i + 2], 16).unwrap()))
                .collect();
            let n = (bytes.len() as f64).sqrt() as usize;
            let mut f = Pathfinder::new(n, &bytes);
            f.set_mode(0);
            f.mask = WALKABLE;
            finder = Some(f);
            continue;
        }
        let f = finder.as_mut().unwrap();
        let (a, b): (i32, i32) = (cols[0].parse().unwrap(), cols[1].parse().unwrap());
        let got = f.find(a, b);
        let expected: Option<Vec<i32>> =
            (cols[2] != "-").then(|| cols[2].split(',').map(|v| v.parse().unwrap()).collect());
        assert_eq!(got, expected, "search {a} -> {b}");
        searches += 1;
        found += expected.is_some() as usize;
    }
    assert_eq!(searches, 600);
    eprintln!("{found} of {searches} searches found a path");
}
