//! Saving, stage 2 (the inverse of `load::construction`): the sites' machines and components,
//! storage, piles, the town manager's registries and values, and the job manager.

use std::collections::BTreeMap;

use banished_formats::savegame::{Fields, GameState, ObjId, Value};
use banished_game::jobs::WorkKind;

use super::{component, fields_mut, Refs};
use crate::start::StartingTown;

/// The `Ext` ids of a saved map's keys, in the saved order. The original keys these maps by
/// object address, which the save records but nothing derives; the writer keeps that order and
/// puts new keys after it.
fn saved_order(list: Option<&Value>, key: &str) -> Vec<u32> {
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
fn in_saved_order(order: &[u32], mut items: Vec<(u32, Fields)>) -> Vec<Fields> {
    items.sort_by_key(|(id, _)| order.iter().position(|o| o == id).unwrap_or(usize::MAX));
    items.into_iter().map(|(_, f)| f).collect()
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
    pub(super) fn export_construction(
        &self,
        g: &mut GameState,
        refs: &mut Refs,
        entities: &[Option<ObjId>],
    ) {
        let entity = |p: usize| entities.get(p).copied().flatten();
        let citizen = |c: usize| {
            self.save_ids
                .as_ref()
                .and_then(|ids| ids.citizens.get(c).copied())
        };
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
        self.export_town(g, refs, entities);
        self.export_jobs(g, refs, entities);
    }

    /// The town manager's registries and values (`FUN_140094c30`).
    fn export_town(&self, g: &mut GameState, refs: &mut Refs, entities: &[Option<ObjId>]) {
        let entity = |p: usize| entities.get(p).copied().flatten();
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

    /// The job manager (`FUN_1400fe710`): each work's fields, then the id table, the category
    /// sets, the profession map and the counters.
    fn export_jobs(&self, g: &mut GameState, refs: &mut Refs, entities: &[Option<ObjId>]) {
        let entity = |p: usize| entities.get(p).copied().flatten();
        let citizen = |c: usize| {
            self.save_ids
                .as_ref()
                .and_then(|ids| ids.citizens.get(c).copied())
        };
        let jobs = &self.construction.jobs;
        // The work objects by id, as saved.
        let mut objects: BTreeMap<i32, ObjId> = BTreeMap::new();
        if let Some(Value::List(works)) = g.managers.at("jobs.works") {
            for w in works {
                if let (Some(key), Some(o)) = (w.i32("key"), w.ptr("work")) {
                    objects.insert(key, o);
                }
            }
        }
        for (&wid, w) in &jobs.works {
            let Some(&oid) = objects.get(&wid) else {
                continue;
            };
            let material = match &w.kind {
                WorkKind::Road(r) => r.material.map(|m| refs.id(g, &self.materials[m].reference)),
                _ => None,
            };
            let Some(o) = g.objects.get_mut(&oid) else {
                continue;
            };
            let f = &mut o.fields;
            f.set_i32("work.id", w.id);
            f.set_i32("work.priority", w.priority);
            f.set_i32("work.cell", w.cell);
            f.set_time("work.patience", w.patience);
            f.set_time("work.patience_base", w.patience_base);
            f.set_i32("work.far_count", w.far_count);
            if w.entity != usize::MAX {
                f.set("work.entity", Value::Ptr(entity(w.entity)));
            }
            let set_citizens =
                |f: &mut Fields, path: &str, set: &mut dyn Iterator<Item = usize>| {
                    f.set(path, ptr_items("citizen", set.map(citizen)));
                };
            match &w.kind {
                WorkKind::Road(r) => {
                    f.set_i32("road.cell", r.cell);
                    f.set_i32("road.type", r.ty as i32);
                    f.set("road.builder", Value::Ptr(r.builder.and_then(citizen)));
                    if let Some(m) = material {
                        f.set_i32("road.material", m as i32);
                    }
                    f.set_i32("road.old_kind", r.old_kind as i32);
                    f.set_i32("road.old_built", r.old_built as i32);
                    f.set_i32("road.old_type", r.old_type as i32);
                }
                WorkKind::Build(b) => {
                    f.set_bool("build.destroy", b.destroy);
                    f.set_i32("build.cursor", b.cursor);
                    f.set(
                        "build.occupied",
                        Value::List(
                            b.occupied
                                .iter()
                                .map(|&d| Fields(vec![("done", Value::Bool(d))]))
                                .collect(),
                        ),
                    );
                    set_citizens(f, "build.citizens", &mut b.workers.iter().copied());
                }
                WorkKind::Gather(gw) => {
                    let values = |v: &[i32]| {
                        Value::List(
                            v.iter()
                                .map(|&x| Fields(vec![("value", Value::I32(x))]))
                                .collect(),
                        )
                    };
                    f.set("gather.stored", values(&gw.stored));
                    f.set("gather.transit", values(&gw.transit));
                    set_citizens(f, "gather.citizens", &mut gw.workers.iter().copied());
                }
                WorkKind::Clear(cw) => {
                    f.set(
                        "clear.assigned",
                        Value::List(
                            cw.resources
                                .iter()
                                .map(|(&e, &c)| {
                                    Fields(vec![
                                        ("entity", Value::Ptr(entity(e))),
                                        ("citizen", Value::Ptr(citizen(c))),
                                    ])
                                })
                                .collect(),
                        ),
                    );
                    f.set(
                        "clear.keyed",
                        Value::List(
                            cw.cells
                                .iter()
                                .map(|(&k, &c)| {
                                    Fields(vec![
                                        ("key", Value::I32(k)),
                                        ("citizen", Value::Ptr(citizen(c))),
                                    ])
                                })
                                .collect(),
                        ),
                    );
                    f.set_bool("clear.clear_resources", cw.clear_resources);
                    f.set_bool("clear.clear_cells", cw.clear_cells);
                    f.set_bool("clear.clear_piles", cw.clear_piles);
                }
                WorkKind::Produce(p) => {
                    f.set(
                        "consume.citizens",
                        ptr_items("citizen", p.workers.iter().map(|w| w.and_then(citizen))),
                    );
                    f.set(
                        "consume.points",
                        Value::List(
                            p.points
                                .iter()
                                .map(|&s| Fields(vec![("state", Value::I32(s as i32))]))
                                .collect(),
                        ),
                    );
                    f.set_bool("consume.full", p.full);
                    f.set_i32("consume.cycles", p.cycles);
                }
                WorkKind::Pickup(_) => {
                    if let Some(pile) = self.construction.piles.get(&w.entity) {
                        f.set_i32("pickup.state", pile.state);
                        f.set("pickup.claimer", Value::Ptr(pile.claimer.and_then(citizen)));
                    }
                }
                _ => {}
            }
        }
        // The id table, the category sets and the profession map, from the port's indexes.
        let entry = |id: i32| {
            Fields(vec![
                ("key", Value::I32(id)),
                ("work", Value::Ptr(objects.get(&id).copied())),
            ])
        };
        let m = &mut g.managers;
        m.set(
            "jobs.works",
            Value::List(jobs.works.keys().map(|&id| entry(id)).collect()),
        );
        for c in 1..=14u32 {
            m.set(
                &format!("jobs.category_{c}"),
                Value::List(jobs.category(c).into_iter().map(entry).collect()),
            );
        }
        let (next_id, next_priority) = jobs.counters();
        m.set_i32("jobs.next_id", next_id);
        m.set_i32("jobs.next_priority", next_priority);
        m.set_i32("jobs.pass", jobs.pass);
        let order = saved_order(g.managers.at("jobs.by_profession"), "profession");
        let by_profession: Vec<(u32, Fields)> = jobs
            .by_profession()
            .into_iter()
            .map(|(p, ids)| {
                // `usize::MAX`: the null profession.
                let id = match self.construction.professions.list.get(p) {
                    Some(p) => refs.id(g, &p.reference),
                    None => u32::MAX,
                };
                (
                    id,
                    Fields(vec![
                        ("profession", Value::Ext(id)),
                        ("work", Value::List(ids.into_iter().map(entry).collect())),
                    ]),
                )
            })
            .collect();
        let by_profession = in_saved_order(&order, by_profession);
        g.managers
            .set("jobs.by_profession", Value::List(by_profession));
    }
}
