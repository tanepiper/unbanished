//! Compare the AnimationManager's per-group data across slots and instances (analysis helper):
//! whether each instance's pairs, and each slot's nodes, are the same for every slot of a group,
//! and whether the vector is the pose's root translation.
//! `cargo run -p unbanished-formats --example save_anim_compare -- IN.sav`
use std::collections::BTreeMap;
use unbanished_formats::save::Save;
use unbanished_formats::savegame::{GameState, Value};

fn main() {
    let path = std::env::args().nth(1).unwrap();
    let g = GameState::decode(&Save::parse(&std::fs::read(path).unwrap()).unwrap().state).unwrap();
    let Some(Value::List(entries)) = g.animations.at("animations.entries") else {
        return;
    };
    let mut nodes: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut pairs: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let (mut vec_root, mut vec_other) = (0, 0);
    for e in entries {
        let group = match e.get("group") {
            Some(Value::Ext(i)) => g.references.get(i).cloned().unwrap_or_default(),
            _ => continue,
        };
        nodes
            .entry(group.clone())
            .or_default()
            .push(format!("{:?}", e.get("nodes")));
        if let Some(Value::List(instances)) = e.get("instances") {
            for i in instances {
                pairs
                    .entry(group.clone())
                    .or_default()
                    .push(format!("{:?}", i.get("pairs")));
                let root = match i.get("pose") {
                    Some(Value::List(p)) => p.first().and_then(|r| r.get("translation")).cloned(),
                    _ => None,
                };
                if root.as_ref() == i.get("vector") {
                    vec_root += 1;
                } else {
                    vec_other += 1;
                }
            }
        }
    }
    for (group, list) in &nodes {
        let distinct: std::collections::BTreeSet<_> = list.iter().collect();
        let empty = Vec::new();
        let p = pairs.get(group).unwrap_or(&empty);
        let pd: std::collections::BTreeSet<_> = p.iter().collect();
        println!(
            "{group}: {} slots, {} distinct node lists; {} instances, {} distinct pair lists",
            list.len(),
            distinct.len(),
            p.len(),
            pd.len()
        );
    }
    println!("vector is the pose's root translation: {vec_root}, not: {vec_other}");
    for e in entries {
        if e.i32("clip") == Some(-1) {
            println!("free slot: {:?}", e);
        }
    }
    if std::env::var_os("DETAIL").is_some() {
        let e = &entries[6];
        if let Some(Value::List(instances)) = e.get("instances") {
            let i = &instances[0];
            println!("states: {:?}", i.get("states"));
            if let (Some(Value::List(pose)), Some(Value::List(pairs))) =
                (i.get("pose"), i.get("pairs"))
            {
                for k in 0..pose.len().min(6) {
                    println!(
                        "node {k}: pose {:?} {:?}",
                        pose[k].get("rotation"),
                        pose[k].get("translation")
                    );
                    println!(
                        "        pair {:?} {:?} | {:?} {:?}",
                        pairs[k].get("rotation"),
                        pairs[k].get("translation"),
                        pairs[k].get("rotation_2"),
                        pairs[k].get("translation_2")
                    );
                }
            }
        }
        if let Some(Value::List(nodes)) = e.get("nodes") {
            for k in 0..6 {
                println!(
                    "bone {k}: {:?} {:?} {:?} | {:?} {:?}",
                    nodes[k].get("name"),
                    nodes[k].get("rotation"),
                    nodes[k].get("translation"),
                    nodes[k].get("rotation_2"),
                    nodes[k].get("translation_2")
                );
            }
        }
    }
}
