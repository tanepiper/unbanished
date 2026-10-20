//! Where two saves' game states differ, field by field (analysis helper): the top-level blocks,
//! then each object's fields. Pairs with `savegame_same`, which only says whether they differ.
//! `cargo run -p unbanished-formats --example savegame_fields_diff -- A.sav B.sav [LIMIT]`
use unbanished_formats::save::Save;
use unbanished_formats::savegame::GameState;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let load = |p: &str| {
        GameState::decode(&Save::parse(&std::fs::read(p).unwrap()).unwrap().state).unwrap()
    };
    let (a, b) = (load(&args[0]), load(&args[1]));
    let limit: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(40);
    let mut shown = 0;
    let mut report = |what: String| {
        if shown < limit {
            println!("{what}");
        }
        shown += 1;
    };
    macro_rules! top {
        ($($f:ident),*) => {$(
            if format!("{:?}", a.$f) != format!("{:?}", b.$f) {
                report(format!("block {} differs", stringify!($f)));
            }
        )*};
    }
    top!(
        references,
        random_index,
        random_state,
        entity_time,
        entities,
        buckets,
        schedules,
        achievements,
        animations,
        audio_scene,
        camera,
        town,
        managers
    );
    for (i, (x, y)) in a.map.cells.iter().zip(&b.map.cells).enumerate() {
        if x != y {
            report(format!("map cell {i}: {x:?} / {y:?}"));
        }
    }
    for (id, x) in &a.objects {
        let Some(y) = b.objects.get(id) else {
            report(format!("object {id:?} ({}) only in A", x.type_name));
            continue;
        };
        if x.type_name != y.type_name {
            report(format!(
                "object {id:?}: type {} / {}",
                x.type_name, y.type_name
            ));
            continue;
        }
        for ((k, v), (_, w)) in x.fields.0.iter().zip(&y.fields.0) {
            if v != w {
                report(format!("object {id:?} {}.{k}: {v:?} / {w:?}", x.type_name));
            }
        }
    }
    for (id, y) in &b.objects {
        if !a.objects.contains_key(id) {
            report(format!("object {id:?} ({}) only in B", y.type_name));
        }
    }
    println!("{shown} differences");
    std::process::exit(i32::from(shown > 0));
}
