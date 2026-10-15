//! `sectors::SectorGraph` against the original graph build, lazy rebuild and path-rank
//! distance (`tools/emu/sectors.py`; skipped without its output in game-data/derived/emu).

use banished_game::pathfind::{regions, WALKABLE, WATER};
use banished_game::sectors::SectorGraph;
use banished_game::terrain::CellType;

/// `FUN_1400a55c0` with mask `0xe1f`.
fn reachable(types: &[CellType], region: &[u16], a: i32, b: i32) -> bool {
    let (ba, bb) = (
        1u32 << (types[a as usize].0 & 31),
        1u32 << (types[b as usize].0 & 31),
    );
    let mask = WALKABLE | ba | bb;
    if mask & WATER != 0 {
        return true;
    }
    let (ra, rb) = (region[a as usize], region[b as usize]);
    ra != 0 && ra == rb
}

#[test]
fn matches_the_original() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../game-data/derived/emu/sectors.tsv"
    );
    let Ok(text) = std::fs::read_to_string(path) else {
        eprintln!("no emulator output; skipping");
        return;
    };
    let mut types: Vec<CellType> = Vec::new();
    let mut graph: Option<SectorGraph> = None;
    let mut region: Vec<u16> = Vec::new();
    let mut n = 0usize;
    let (mut maps, mut queries) = (0, 0);
    for line in text.lines() {
        let cols: Vec<&str> = line.split('\t').collect();
        match cols[0] {
            "map" => {
                types = (0..cols[1].len() / 2)
                    .map(|i| CellType(u8::from_str_radix(&cols[1][2 * i..2 * i + 2], 16).unwrap()))
                    .collect();
                n = (types.len() as f64).sqrt() as usize;
                let g = SectorGraph::new(n, &types);
                let nodes: Vec<i32> = cols[2].split(',').map(|v| v.parse().unwrap()).collect();
                assert_eq!(g.cell_node, nodes, "node ids of map {maps}");
                region = regions(n, &types);
                graph = Some(g);
                maps += 1;
            }
            "q" => {
                let (a, b, d): (i32, i32, i32) = (
                    cols[1].parse().unwrap(),
                    cols[2].parse().unwrap(),
                    cols[3].parse().unwrap(),
                );
                let r = reachable(&types, &region, a, b);
                let got = graph.as_mut().unwrap().distance(a, b, r, false, &types);
                assert_eq!(got, d, "map {maps}: {a} -> {b}");
                queries += 1;
            }
            "change" => {
                let c: usize = cols[1].parse().unwrap();
                types[c] = CellType(8);
                graph.as_mut().unwrap().mark((c % n) as i32, (c / n) as i32);
                region = regions(n, &types);
            }
            "nodes2" => {
                let g = graph.as_mut().unwrap();
                g.rebuild(&types);
                let nodes: Vec<i32> = cols[1].split(',').map(|v| v.parse().unwrap()).collect();
                assert_eq!(g.cell_node, nodes, "node ids after changes, map {maps}");
            }
            _ => {}
        }
    }
    assert_eq!((maps, queries), (12, 3600));
}
