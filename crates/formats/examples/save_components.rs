//! Print the components of the first saved entity of each template, in the entity's own order
//! (analysis helper). The entity manager runs an entity's components in this order.
//! `cargo run -p unbanished-formats --example save_components -- IN.sav [TEMPLATE-SUBSTRING]`
use std::collections::BTreeSet;
use unbanished_formats::save::Save;
use unbanished_formats::savegame::{GameState, Value};

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().unwrap();
    let filter = args.next().unwrap_or_default();
    let g = GameState::decode(&Save::parse(&std::fs::read(path).unwrap()).unwrap().state).unwrap();
    let mut seen = BTreeSet::new();
    for o in g.objects.values() {
        let Some(Value::List(components)) = o.fields.at("entity.components") else {
            continue;
        };
        let template = match o.fields.at("entity.template") {
            Some(Value::Ext(i)) | Some(Value::U32(i)) => g.references.get(i).cloned(),
            Some(Value::I32(i)) => g.references.get(&(*i as u32)).cloned(),
            _ => None,
        }
        .unwrap_or_default();
        if !template.contains(&filter) || !seen.insert(template.clone()) {
            continue;
        }
        let names: Vec<&str> = components
            .iter()
            .filter_map(|c| c.ptr("component"))
            .filter_map(|c| g.objects.get(&c))
            .map(|c| c.type_name)
            .collect();
        println!("{template}: {}", names.join(", "));
    }
}
