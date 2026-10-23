//! List the animation states saved in each ModelComponent (analysis helper): the entity's
//! template, then each layer's group, clip, speed, clock, blend, instance and started; then the
//! AnimationManager's shared entries with their users and each instance's clip and clock.
//! `cargo run -p unbanished-formats --example save_anim_states -- IN.sav [LIMIT]`
use unbanished_formats::save::Save;
use unbanished_formats::savegame::{GameState, Value};

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().unwrap();
    let limit: usize = args.next().map_or(usize::MAX, |n| n.parse().unwrap());
    let g = GameState::decode(&Save::parse(&std::fs::read(path).unwrap()).unwrap().state).unwrap();
    let name = |r: Option<&Value>| match r {
        Some(Value::Ext(i)) => g.references.get(i).cloned().unwrap_or_default(),
        _ => String::new(),
    };
    let mut shown = 0;
    for o in g.objects.values() {
        if o.type_name != "ModelComponent" {
            continue;
        }
        let Some(Value::List(states)) = o.fields.at("model.animations") else {
            continue;
        };
        if states.is_empty() {
            continue;
        }
        let entity = o.fields.ptr("base.entity").and_then(|e| g.objects.get(&e));
        let template = name(entity.and_then(|e| e.fields.at("entity.template")));
        println!("{template} animating {:?}", o.fields.at("model.animating"));
        for s in states {
            println!(
                "  {} clip {:?} speed {:?} elapsed {:?} blend {:?} left {:?} instance {:?} started {:?}",
                name(s.get("animation")),
                s.i32("clip"),
                s.f32("speed"),
                s.get("elapsed"),
                s.get("blend"),
                s.get("left"),
                s.i32("instance"),
                s.get("started")
            );
        }
        shown += 1;
        if shown >= limit {
            break;
        }
    }
    if let Some(Value::List(entries)) = g.animations.at("animations.entries") {
        for (i, e) in entries.iter().enumerate() {
            println!(
                "entry {i} (id {:#x}): {} clip {:?} users {:?}",
                i << 8,
                name(e.get("group")),
                e.i32("clip"),
                e.i32("users")
            );
            if std::env::var_os("DETAIL").is_some() {
                if let Some(Value::List(nodes)) = e.get("nodes") {
                    println!("  nodes: {}", nodes.len());
                    for n in nodes.iter().take(3) {
                        println!("    {n:?}");
                    }
                }
            }
            if let Some(Value::List(instances)) = e.get("instances") {
                for (k, inst) in instances.iter().enumerate() {
                    let Some(Value::List(states)) = inst.get("states") else {
                        continue;
                    };
                    let clocks: Vec<_> = states
                        .iter()
                        .map(|s| (s.i32("clip"), s.get("elapsed").cloned()))
                        .collect();
                    println!("  instance {k}: {clocks:?}");
                    if std::env::var_os("DETAIL").is_some() {
                        for key in ["flag", "pairs", "vector", "pose"] {
                            match inst.get(key) {
                                Some(Value::List(l)) => {
                                    println!("    {key}: {} items, first {:?}", l.len(), l.first())
                                }
                                v => println!("    {key}: {v:?}"),
                            }
                        }
                    }
                }
            }
        }
    }
}
