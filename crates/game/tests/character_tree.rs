//! `character_tree::CharacterTree` against the original's (`tools/emu/character_tree.py`;
//! skipped without its output in game-data/derived/emu): the same inserts, moves and removes
//! give the same slots, and every search the same characters in the same order.

use std::collections::HashMap;
use unbanished_game::behaviour::character_tree::{CharacterTree, Who};

#[test]
fn matches_the_original() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../game-data/derived/emu/character_tree.tsv"
    );
    let Ok(text) = std::fs::read_to_string(path) else {
        eprintln!("no emulator output; skipping");
        return;
    };
    let mut tree = CharacterTree::default();
    let mut slots: HashMap<usize, (i32, f32)> = HashMap::new();
    let (mut searches, mut found) = (0, 0);
    for line in text.lines() {
        let w: Vec<&str> = line.split(' ').collect();
        let f = |k: usize| w[k].parse::<f64>().unwrap() as f32;
        let id = |k: usize| w[k].parse::<usize>().unwrap();
        match w[0] {
            "insert" => {
                let i = id(1);
                let slot = tree.insert(Who::Citizen(i), [f(2), f(3), f(4)], f(5));
                assert_eq!(slot, w[6].parse::<i32>().unwrap(), "{line}");
                slots.insert(i, (slot, f(5)));
            }
            "move" => {
                let (slot, r) = slots[&id(1)];
                tree.place(slot, [f(2), f(3), f(4)], r);
            }
            "remove" => {
                let (slot, _) = slots.remove(&id(1)).unwrap();
                tree.remove(slot);
            }
            "within" => {
                let got: Vec<String> = tree
                    .within([f(1), f(2), f(3)], f(4), |_| true)
                    .into_iter()
                    .map(|w| match w {
                        Who::Citizen(i) => i.to_string(),
                        _ => unreachable!(),
                    })
                    .collect();
                let got = if got.is_empty() {
                    "-".to_string()
                } else {
                    got.join(",")
                };
                assert_eq!(got, w[5], "{line}");
                searches += 1;
                found += (w[5] != "-") as usize;
            }
            "nearest" => {
                let got = match tree.nearest([f(1), f(2), f(3)], f(4), |_| true) {
                    Some(Who::Citizen(i)) => i as i64,
                    _ => -1,
                };
                assert_eq!(got, w[5].parse::<i64>().unwrap(), "{line}");
                searches += 1;
                found += (got >= 0) as usize;
            }
            _ => unreachable!(),
        }
    }
    eprintln!("{searches} searches, {found} found something");
    assert!(found > 50);
}
