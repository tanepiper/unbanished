use super::tests_support::*;
use unbanished_formats::savegame::Value;

#[test]
fn vanilla_saves_restore_producers() {
    let Some(archive) = archive() else {
        eprintln!("game data missing; skipping");
        return;
    };
    for name in [
        "Save2.sav",
        "Save3.sav",
        "Save4.sav",
        "Save5.sav",
        "Save83.sav",
    ] {
        let Ok(bytes) = std::fs::read(root().join("save-game").join(name)) else {
            continue;
        };
        let saved = crate::saved::SavedGame::parse(&bytes).unwrap();
        let town = crate::town::StartingTown::from_save(&archive, &saved).unwrap();
        // FloorComponent slot `+0x58` (`FUN_140124a10`) on load: an active floor counts one
        // more on every cell under it (its mesh handle is not saved, so nothing is taken
        // away first), so the counts grow with every load in the original too.
        let n = town.cells.size as i32;
        let mut saved_floors: Vec<u8> = saved.state.map.cells.iter().map(|c| c.bytes[5]).collect();
        for (id, e) in saved.entities() {
            let active = saved
                .component(e, "FloorComponent")
                .is_some_and(|f| matches!(f.fields.at("floor.active"), Some(Value::Bool(true))));
            let Some(map) = saved.component(e, "MapComponent").filter(|_| active) else {
                continue;
            };
            let _ = id;
            let m = |k: &str| map.fields.i32(&format!("map.{k}")).unwrap_or(0);
            for y in m("y")..m("y") + m("h") {
                for x in m("x")..m("x") + m("w") {
                    saved_floors[(y * n + x) as usize] += 1;
                }
            }
        }
        let wrong: Vec<(usize, u8, u8)> = town
            .cells
            .floors
            .counts
            .iter()
            .zip(&saved_floors)
            .enumerate()
            .filter(|(_, (a, b))| a != b)
            .map(|(i, (a, b))| (i, *a, *b))
            .collect();
        if !wrong.is_empty() {
            let mut kinds = std::collections::BTreeMap::new();
            for &(i, _, _) in &wrong {
                let t = town.cells.objects[i]
                    .or(town.cells.occupants[i])
                    .map(|e| {
                        let t = &town.templates[town.placed[e].template];
                        format!("{} floor {}", t.reference, t.floor.is_some())
                    })
                    .unwrap_or_default();
                *kinds.entry(t).or_insert(0) += 1;
            }
            eprintln!("{name}: differing floor cells by object: {kinds:?}");
        }
        assert!(
            wrong.is_empty(),
            "{name}: {} floor cells differ (cell, ours, saved), first {:?}",
            wrong.len(),
            &wrong[..wrong.len().min(8)]
        );
        let c = &town.construction;
        let (mut crops, mut trees) = (0, 0);
        for site in c.sites.values() {
            if let Some(f) = &site.crop_field {
                for cell in f.cells.iter().filter(|c| c.state == 1) {
                    crops += 1;
                    let e = cell.entity.expect("growing crop without an entity");
                    assert!(town.placed[e].alive, "{name}: crop {e}");
                }
            }
            if let Some(o) = &site.orchard {
                for t in o.trees.iter().filter(|t| t.state == 1) {
                    trees += 1;
                    let e = t.entity.expect("standing tree without an entity");
                    assert!(
                        c.fruits.contains_key(&e),
                        "{name}: tree {e} without its fruit"
                    );
                }
            }
            if let Some(t) = &site.trade {
                let saved_offers = saved
                    .entities()
                    .filter_map(|(_, e)| saved.component(e, "TradeComponent"))
                    .map(|t| match t.fields.at("trade.stock") {
                        Some(Value::List(v)) => v.len(),
                        _ => 0,
                    })
                    .max()
                    .unwrap_or(0);
                assert!(t.offers.len() <= saved_offers);
            }
        }
        let floors = town.cells.floors.counts.iter().filter(|&&n| n > 0).count();
        eprintln!("{name}: {crops} growing crops, {trees} standing trees, {floors} floor cells");
    }
}
