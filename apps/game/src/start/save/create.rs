//! Saving: objects made since the load (docs/SAVE-WRITING.md). A new entity gets its
//! ComponentEntity and one component per description of its template that makes one, in the
//! template's order (`FUN_1400c8bf0`), each with its base (entity, flags, description) and
//! default fields for the writer to fill.

use banished_formats::crc::type_id;
use banished_formats::pkg::Archive;
use banished_formats::savegame::{GameState, ObjId, Value};

use super::Refs;
use crate::start::{normalise, read};

/// A description resource's component: `FooDescription` makes a `FooComponent` (by type id).
fn component_for(description_type: u32) -> Option<&'static str> {
    banished_formats::savegame_schema::type_names()
        .filter(|n| n.ends_with("Component"))
        .find(|n| type_id(&n.replace("Component", "Description")) == description_type)
}

/// A template's components in order, with each one's description path.
pub(crate) fn template_components(
    archive: &Archive,
    template: &str,
) -> Vec<(&'static str, String)> {
    let Some(desc) = read(archive, &normalise(template))
        .and_then(|d| banished_formats::game::component_description(&d).ok())
    else {
        return Vec::new();
    };
    desc.descriptions
        .iter()
        .flatten()
        .filter_map(|d| {
            let data = read(archive, &normalise(d))?;
            let header = banished_formats::crs::header(&data).ok()?;
            // Slot `+0x40` gives the component type; UIDescription's only with `+0x20` above 0
            // (`FUN_14003a040`).
            if header.type_id == type_id("UIDescription")
                && banished_formats::game::ui_description(&data).map_or(true, |u| u.value <= 0)
            {
                return None;
            }
            Some((component_for(header.type_id)?, normalise(d)))
        })
        .collect()
}

/// A new entity of `template`: its ComponentEntity and components, linked both ways. Returns
/// the ComponentEntity's id.
pub(crate) fn new_entity(
    archive: &Archive,
    g: &mut GameState,
    refs: &mut Refs,
    template: &str,
) -> Option<ObjId> {
    let entity = g.new_object("ComponentEntity")?;
    let template_id = refs.id(g, template);
    let mut components = Vec::new();
    for (name, description) in template_components(archive, template) {
        let Some(c) = g.new_object(name) else {
            continue;
        };
        let description = refs.id(g, &description);
        let f = &mut g.objects.get_mut(&c).unwrap().fields;
        f.set("base.entity", Value::Ptr(Some(entity)));
        f.set_i32("base.description", description as i32);
        components.push(c);
    }
    let f = &mut g.objects.get_mut(&entity).unwrap().fields;
    f.set_i32("entity.template", template_id as i32);
    f.set(
        "entity.components",
        Value::List(
            components
                .into_iter()
                .map(|c| {
                    banished_formats::savegame::Fields(vec![("component", Value::Ptr(Some(c)))])
                })
                .collect(),
        ),
    );
    Some(entity)
}

#[cfg(test)]
mod tests {
    use crate::start::load::tests_support::{archive, root};

    /// Every saved entity's components are what its template predicts, in order.
    #[test]
    fn templates_predict_the_saved_components() {
        let Some(archive) = archive() else {
            eprintln!("game data missing; skipping");
            return;
        };
        for name in ["Save5.sav", "Save83.sav"] {
            let Ok(bytes) = std::fs::read(root().join("save-game").join(name)) else {
                continue;
            };
            let saved = crate::saved::SavedGame::parse(&bytes).unwrap();
            let mut checked = std::collections::BTreeMap::new();
            let mut wrong = std::collections::BTreeMap::new();
            for (_, e) in saved.entities() {
                let Some(t) = saved.ext(&e.fields, "entity.template") else {
                    continue;
                };
                if checked.contains_key(t) {
                    continue;
                }
                let have: Vec<&str> = saved.components(e).iter().map(|c| c.type_name).collect();
                let ours: Vec<&str> = super::template_components(&archive, t)
                    .iter()
                    .map(|c| c.0)
                    .collect();
                checked.insert(t.to_string(), ());
                if have != ours {
                    wrong.insert(t.to_string(), (have, ours));
                }
            }
            for (t, (have, ours)) in &wrong {
                eprintln!("{name} {t}:\n  saved {have:?}\n  ours  {ours:?}");
            }
            assert!(
                wrong.is_empty(),
                "{name}: {} of {} templates differ",
                wrong.len(),
                checked.len()
            );
        }
    }
}

impl crate::start::StartingTown {
    /// New placed entities get their objects (appended to the entity list, their bucket and the
    /// schedules); dead ones leave the lists, swapping the last entry into their place as the
    /// original's lists do. Returns the extended id map.
    pub(super) fn sync_entities(
        &self,
        archive: &Archive,
        g: &mut GameState,
        refs: &mut Refs,
        saved: &crate::start::SaveIds,
    ) -> super::Ids {
        let mut ids = super::Ids {
            entities: saved.entities.clone(),
            citizens: saved.citizens.iter().map(|&c| Some(c)).collect(),
        };
        ids.entities.resize(self.placed.len(), None);
        // Dead entities leave the entity list and their bucket.
        for (p, placed) in self.placed.iter().enumerate() {
            if placed.alive {
                continue;
            }
            if let Some(id) = ids.entities[p].take() {
                remove_entity(g, id);
            }
        }
        // New ones join.
        for (p, placed) in self.placed.iter().enumerate() {
            if !placed.alive || ids.entities[p].is_some() {
                continue;
            }
            let template = self.templates[placed.template].reference.clone();
            let Some(id) = new_entity(archive, g, refs, &template) else {
                continue;
            };
            add_entity(g, id, placed.bucket, self.scheduler.time);
            ids.entities[p] = Some(id);
        }
        ids
    }
}

/// Append an entity to the entity manager (`FUN_1400c9d70`): its list, its bucket and its
/// schedule (list index, bucket slot or −1, bucket, last update time).
pub(crate) fn add_entity(g: &mut GameState, id: ObjId, bucket: usize, time: i64) {
    let k = g.entities.len() as i32;
    g.entities.push(Some(id));
    let slot = match g.buckets.get_mut(bucket) {
        Some(list) if bucket < 5 => {
            list.push(Some(id));
            list.len() as i32 - 1
        }
        _ => -1,
    };
    g.schedules.push((k, slot, bucket as i32, time));
}

/// Take an entity out of the entity manager's list and its bucket, the last entry of each
/// moving into its place (its schedule's index and slot follow).
pub(crate) fn remove_entity(g: &mut GameState, id: ObjId) {
    let Some(k) = g.entities.iter().position(|&e| e == Some(id)) else {
        return;
    };
    let (_, slot, bucket, _) = g.schedules[k];
    g.entities.swap_remove(k);
    g.schedules.swap_remove(k);
    if k < g.schedules.len() {
        g.schedules[k].0 = k as i32;
    }
    if (0..5).contains(&bucket) && slot >= 0 {
        let list = &mut g.buckets[bucket as usize];
        let slot = slot as usize;
        if slot < list.len() {
            list.swap_remove(slot);
            // The entry moved into the slot gets it in its schedule.
            if let Some(Some(moved)) = list.get(slot).copied() {
                if let Some(m) = g.entities.iter().position(|&e| e == Some(moved)) {
                    g.schedules[m].1 = slot as i32;
                }
            }
        }
    }
}
