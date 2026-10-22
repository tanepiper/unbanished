//! Saving, stage 2 (the inverse of `load::construction`): the sites' machines and components,
//! storage, piles, the town manager's registries and values, and the job manager.

mod jobs;

use std::collections::BTreeMap;

use unbanished_formats::savegame::{Fields, GameState, ObjId, Value};
use unbanished_game::work::jobs::WorkKind;

use super::{component, fields_mut, Ids, Refs};
use crate::town::StartingTown;

/// The `Ext` ids of a saved map's keys, in the saved order. The original keys these maps by
/// object address, which the save records but nothing derives; the writer keeps that order and
/// puts new keys after it.
pub(super) fn saved_order(list: Option<&Value>, key: &str) -> Vec<u32> {
    match list {
        Some(Value::List(items)) => items
            .iter()
            .filter_map(|i| match i.get(key) {
                Some(Value::Ext(id)) => Some(*id),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// Sort `(ext id, item)` by the saved order, new ids last.
pub(super) fn in_saved_order(order: &[u32], mut items: Vec<(u32, Fields)>) -> Vec<Fields> {
    items.sort_by_key(|(id, _)| order.iter().position(|o| o == id).unwrap_or(usize::MAX));
    items.into_iter().map(|(_, f)| f).collect()
}

/// A set written as a list of single pointers, in the order the save had its members (the
/// original keeps these sets keyed by object address); new members after them.
pub(super) fn like_saved(current: Option<&Value>, new: Value) -> Value {
    let (Some(Value::List(old)), Value::List(mut items)) = (current, new.clone()) else {
        return new;
    };
    // Only sets keyed by a pointer (an address in the original).
    if !items
        .first()
        .and_then(|f| f.0.first())
        .is_some_and(|(_, v)| matches!(v, Value::Ptr(_)))
    {
        return new;
    }
    let first = |f: &Fields| f.0.first().map(|(_, v)| v.clone());
    let position = |f: &Fields| {
        let v = first(f);
        old.iter().position(|o| first(o) == v).unwrap_or(usize::MAX)
    };
    items.sort_by_key(position);
    Value::List(items)
}

/// `Value::List` of single-field items.
fn ptr_items(name: &'static str, ids: impl IntoIterator<Item = Option<ObjId>>) -> Value {
    Value::List(
        ids.into_iter()
            .map(|id| Fields(vec![(name, Value::Ptr(id))]))
            .collect(),
    )
}

impl StartingTown {
    pub(super) fn export_construction(&self, g: &mut GameState, refs: &mut Refs, ids: &Ids) {
        let entity = |p: usize| ids.entity(p);
        let citizen = |c: usize| ids.citizen(c);
        let c = &self.construction;
        for (&e, site) in &c.sites {
            let Some(id) = entity(e) else { continue };
            if let Some(m) = fields_mut(g, id, "StateMachineComponent") {
                let mm = &site.machine;
                m.set_i32("machine.state", mm.state as i32);
                m.set_i32("machine.index", mm.index as i32);
                m.set_i32("machine.pending", mm.pending as i32);
                m.set_i32("machine.transition", mm.transition as i32);
            }
            if let Some(b) = fields_mut(g, id, "BuildComponent") {
                b.set_i32("build.progress", site.progress);
                b.set_i32("build.stages", site.stages);
            }
            if let Some(w) = fields_mut(g, id, "WorkComponent") {
                w.set_i32("work.work_id", site.work_id);
                w.set_i32("work.kept_priority", site.work_priority.unwrap_or(i32::MIN));
                if let Some(wp) = c.workforce.workplaces.get(&e) {
                    w.set_i32("work.most_workers", wp.max_workers);
                }
            }
            if let Some(f) = fields_mut(g, id, "ClearAreaComponent") {
                f.set_i32("clear.total", site.clear_total);
            }
            if let (Some(d), Some(f)) = (c.decals.get(&e), fields_mut(g, id, "DecalComponent")) {
                if d.opaque {
                    f.set_f32("decal.alpha", 1.0);
                }
                f.set_i32("decal.material", d.material);
            }
            if let Some(wp) = c.workforce.workplaces.get(&e) {
                let profession = wp
                    .profession
                    .map(|p| refs.id(g, &c.professions.list[p].reference));
                if let Some(f) = fields_mut(g, id, "WorkPlaceComponent") {
                    f.set_bool("workplace.enabled", wp.enabled);
                    if let Some(p) = profession {
                        f.set_i32("workplace.profession", p as i32);
                    }
                    f.set(
                        "workplace.workers",
                        ptr_items("worker", wp.workers.iter().map(|&w| citizen(w))),
                    );
                }
            }
            if let Some(st) = self.storages.get(site.storage) {
                let items: Vec<Fields> = st
                    .items
                    .iter()
                    .map(|i| {
                        Fields(vec![
                            ("count", Value::I32(i.count)),
                            ("reserved", Value::I32(i.reserved)),
                            (
                                "material",
                                Value::Ext(refs.id(g, &self.materials[i.material].reference)),
                            ),
                        ])
                    })
                    .collect();
                if let Some(f) = fields_mut(g, id, "StorageComponent") {
                    f.set("storage.items", Value::List(items));
                    f.set_i32("storage.capacity", st.capacity);
                    f.set_i32("storage.accepts", st.accepts as i32);
                }
            }
        }
        for (&e, pile) in &c.piles {
            let Some(id) = entity(e) else { continue };
            if let Some(f) = fields_mut(g, id, "RawMaterialComponent") {
                f.set_i32("raw.count", pile.count);
                f.set_time("raw.age", pile.age);
            }
            if let Some(f) = fields_mut(g, id, "WorkComponent") {
                f.set_i32("work.work_id", pile.work);
            }
        }
        self.export_town(g, refs, ids);
        self.export_jobs(g, refs, ids);
    }

    /// The town manager's registries and values (`FUN_140094c30`).
    fn export_town(&self, g: &mut GameState, refs: &mut Refs, ids: &Ids) {
        let entity = |p: usize| ids.entity(p);
        let c = &self.construction;
        // Lists of entities, and lists of a component of each entity.
        let as_entities = |list: &[usize]| list.iter().map(|&e| entity(e)).collect::<Vec<_>>();
        let as_components = |g: &GameState, list: &[usize], t: &str| {
            list.iter()
                .map(|&e| entity(e).and_then(|id| component(g, id, t)))
                .collect::<Vec<_>>()
        };
        let storages: Vec<usize> = self
            .storage_sites
            .iter()
            .filter(|s| s.radius.is_none())
            .map(|s| s.entity)
            .collect();
        let markets: Vec<usize> = self
            .storage_sites
            .iter()
            .filter(|s| s.radius.is_some())
            .map(|s| s.entity)
            .collect();
        let lists: Vec<(&str, Vec<Option<ObjId>>)> = vec![
            ("town.storages", as_entities(&storages)),
            ("town.markets", as_entities(&markets)),
            ("town.residences", as_entities(&c.workforce.residences)),
            ("town.trading_posts", as_entities(&c.trading_posts)),
            (
                "town.sites",
                as_components(g, &c.site_list, "BuildComponent"),
            ),
            (
                "town.healers",
                as_components(g, &c.healers, "HealerComponent"),
            ),
            (
                "town.cemeteries",
                as_components(g, &c.cemeteries, "CemeteryComponent"),
            ),
            (
                "town.happiness",
                as_components(g, &c.happiness, "HappinessComponent"),
            ),
            (
                "town.attendance",
                as_components(g, &c.attendance_list, "AttendanceComponent"),
            ),
        ];
        let order = saved_order(g.town.at("town.workplaces"), "profession");
        let workplaces: Vec<(u32, Fields)> = c
            .workforce
            .registry
            .iter()
            .map(|(&p, es)| {
                let id = refs.id(g, &c.professions.list[p].reference);
                (
                    id,
                    Fields(vec![
                        ("profession", Value::Ext(id)),
                        (
                            "entities",
                            ptr_items("entity", es.iter().map(|&e| entity(e))),
                        ),
                    ]),
                )
            })
            .collect();
        let workplaces = in_saved_order(&order, workplaces);
        let known_materials: Vec<u32> = c
            .known_materials
            .iter()
            .map(|&m| refs.id(g, &self.materials[m].reference))
            .collect();
        let known: Vec<u32> = c
            .known
            .iter()
            .map(|&t| refs.id(g, &self.templates[t].reference))
            .collect();
        let town = &mut g.town;
        for (path, ids) in lists {
            town.set(path, ptr_items("entity", ids));
        }
        town.set("town.workplaces", Value::List(workplaces));
        town.set_i32("town.known_material_count", known_materials.len() as i32);
        town.set("town.known_materials", Value::U32s(known_materials));
        town.set_i32("town.known_count", known.len() as i32);
        town.set("town.known", Value::U32s(known));
        town.set_f32("town.visitor_risk", c.visitors.0);
        town.set_time("town.visitor_time", c.visitors.1);
        town.set_i32("town.trade_value", c.trade_value);
        town.set_i32("town.nomads_admitted", c.nomads_admitted);
        town.set_time("town.commute_time", c.commute.timer);
        if let Some(Value::List(slots)) = town.at_mut("town.stock") {
            for (k, s) in slots.iter_mut().enumerate() {
                s.set_i32("used", c.counters.used.get(k).copied().unwrap_or(0));
                s.set_i32("made", c.counters.made.get(k).copied().unwrap_or(0));
            }
        }
    }
}
