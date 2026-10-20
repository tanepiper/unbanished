//! `pathfind::regions` against the original labelling (`tools/emu/regions.py`; skipped without
//! its output in game-data/derived/emu).

use unbanished_game::pathfind::regions;
use unbanished_game::terrain::CellType;

#[test]
fn matches_the_original() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../game-data/derived/emu/regions.tsv"
    );
    let Ok(text) = std::fs::read_to_string(path) else {
        eprintln!("no emulator output; skipping");
        return;
    };
    let mut maps = 0;
    for line in text.lines() {
        let (hex, expected) = line.split_once('\t').unwrap();
        let types: Vec<CellType> = (0..hex.len() / 2)
            .map(|i| CellType(u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).unwrap()))
            .collect();
        let n = (types.len() as f64).sqrt() as usize;
        let expected: Vec<u16> = expected.split(',').map(|v| v.parse().unwrap()).collect();
        let got = regions(n, &types);
        let max = expected.iter().max().copied().unwrap_or(0);
        assert_eq!(got, expected, "map {maps} ({max} regions)");
        maps += 1;
    }
    assert_eq!(maps, 20);
}
