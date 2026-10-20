//! Saving, stage 2 (the inverse of `load::construction`): the sites' machines and components,
//! storage, piles, the town manager's registries and values, and the job manager.

use std::collections::BTreeMap;

use unbanished_formats::savegame::{Fields, GameState, ObjId, Value};
use unbanished_game::work::jobs::WorkKind;

use super::{component, fields_mut, Ids, Refs};
use crate::start::StartingTown;

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

/// The original's class for a work kind.
fn work_type(kind: &WorkKind) -> &'static str {
    match kind {
        WorkKind::Gather(_) => "GatherRawMaterials",
        WorkKind::Build(_) => "BuildStructure",
        WorkKind::Clear(_) => "ClearArea",
        WorkKind::Pickup(_) => "Pickup",
        WorkKind::Produce(_) => "ConsumeProduce",
        WorkKind::Location(_) => "LocationWork",
        WorkKind::Herbalist(_) => "HerbalistWork",
        WorkKind::Forester(_) => "ForesterWork",
        WorkKind::Hunt(_) => "HuntWork",
        WorkKind::PlantArea(_) => "PlantArea",
        WorkKind::TendArea(_) => "TendArea",
        WorkKind::Harvest(_) => "HarvestArea",
        WorkKind::PlantOrchard(_) => "PlantOrchard",
        WorkKind::PickFruit(_) => "HarvestOrchard",
        WorkKind::Pasture(_) => "PastureWork",
        WorkKind::ClearStorage(_) => "ClearStorage",
        WorkKind::Fire(_) => "FireWork",
        WorkKind::Trade(_) => "TradeWork",
        WorkKind::Road(_) => "BuildRoad",
        WorkKind::Market(_) => "MarketWork",
        WorkKind::Fishing(_) => "FishingWork",
        WorkKind::Test(_) => "Work",
    }
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

    /// The fields of the work kinds beyond stage 2's (the inverse of `load::construction`'s
    /// kinds), as `(path, value)` to set.
    fn work_fields(
        &self,
        g: &mut GameState,
        refs: &mut Refs,
        ids: &Ids,
        kind: &WorkKind,
    ) -> Vec<(String, Value)> {
        let citizen = |c: usize| ids.citizen(c);
        let citizens = |block: &str, set: &mut dyn Iterator<Item = usize>| {
            (
                format!("{block}.citizens"),
                ptr_items("citizen", set.map(citizen)),
            )
        };
        let area = |block: &str, a: &unbanished_game::production::farm::AreaWork| {
            vec![
                (
                    format!("{block}.keyed"),
                    Value::List(
                        a.claims
                            .iter()
                            .map(|(&k, &c)| {
                                Fields(vec![
                                    ("key", Value::I32(k)),
                                    ("citizen", Value::Ptr(citizen(c))),
                                ])
                            })
                            .collect(),
                    ),
                ),
                (
                    format!("{block}.cells"),
                    Value::List(
                        a.need
                            .iter()
                            .map(|&d| Fields(vec![("done", Value::Bool(d))]))
                            .collect(),
                    ),
                ),
            ]
        };
        let herb = |block: &str,
                    h: &unbanished_game::production::herbalist::HerbalistWork,
                    misses: Option<&str>| {
            let mut out = vec![citizens(block, &mut h.workers.iter().copied())];
            if let Some(m) = misses {
                out.push((format!("{block}.{m}"), Value::I32(h.misses)));
            }
            out
        };
        match kind {
            WorkKind::Forester(h) => herb("forester", h, Some("value_58")),
            WorkKind::Herbalist(h) => herb("herbalist", h, Some("value_58")),
            WorkKind::TendArea(h) => herb("tend", h, Some("value_58")),
            WorkKind::Hunt(h) => herb("hunt", h, None),
            WorkKind::Pasture(h) => herb("pasture", h, None),
            WorkKind::ClearStorage(h) => herb("clear_storage", h, None),
            WorkKind::Fire(h) => herb("fire", h, None),
            WorkKind::Fishing(fw) => vec![
                (
                    "fishing.citizens".into(),
                    ptr_items("citizen", fw.workers.iter().map(|w| w.and_then(citizen))),
                ),
                (
                    "fishing.values".into(),
                    Value::List(
                        fw.points
                            .iter()
                            .map(|&v| Fields(vec![("value", Value::I32(v))]))
                            .collect(),
                    ),
                ),
                ("fishing.value_68".into(), Value::I32(fw.misses)),
            ],
            WorkKind::Location(l) => vec![(
                "location.citizen".into(),
                Value::Ptr(l.worker.and_then(citizen)),
            )],
            WorkKind::Trade(t) => {
                let orders = t
                    .haulers
                    .iter()
                    .map(|(&c, &(m, n))| {
                        Fields(vec![
                            ("citizen", Value::Ptr(citizen(c))),
                            ("count", Value::I32(n)),
                            (
                                "description",
                                Value::Ext(refs.id(g, &self.materials[m].reference)),
                            ),
                        ])
                    })
                    .collect();
                vec![("trade.orders".into(), Value::List(orders))]
            }
            WorkKind::Market(mw) => {
                let orders = mw
                    .plans
                    .iter()
                    .map(|(&c, items)| {
                        let items = items
                            .iter()
                            .map(|&(m, n)| {
                                Fields(vec![
                                    (
                                        "description",
                                        Value::Ext(refs.id(g, &self.materials[m].reference)),
                                    ),
                                    ("count", Value::I32(n)),
                                ])
                            })
                            .collect();
                        Fields(vec![
                            ("citizen", Value::Ptr(citizen(c))),
                            ("items", Value::List(items)),
                        ])
                    })
                    .collect();
                vec![("market.orders".into(), Value::List(orders))]
            }
            WorkKind::PlantArea(a) | WorkKind::PlantOrchard(a) => {
                let mut out = area("plant", a);
                if let Some(t) = a.crop {
                    out.push((
                        "plant.description".into(),
                        Value::Ext(refs.id(g, &self.templates[t].reference)),
                    ));
                }
                out
            }
            WorkKind::Harvest(a) | WorkKind::PickFruit(a) => area("harvest", a),
            _ => Vec::new(),
        }
    }

    /// The job manager (`FUN_1400fe710`): each work's fields, then the id table, the category
    /// sets, the profession map and the counters.
    fn export_jobs(&self, g: &mut GameState, refs: &mut Refs, ids: &Ids) {
        let entity = |p: usize| ids.entity(p);
        let citizen = |c: usize| ids.citizen(c);
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
        // Works made since the load get their object (the work's class by its kind).
        for (&wid, w) in &jobs.works {
            if objects.contains_key(&wid) {
                continue;
            }
            let Some(oid) = g.new_object(work_type(&w.kind)) else {
                continue;
            };
            if w.entity != usize::MAX {
                let e = entity(w.entity);
                let wc = e.and_then(|e| component(g, e, "WorkComponent"));
                let f = &mut g.objects.get_mut(&oid).unwrap().fields;
                f.set("work.entity", Value::Ptr(e));
                f.set("work.component", Value::Ptr(wc));
            }
            objects.insert(wid, oid);
        }
        for (&wid, w) in &jobs.works {
            let Some(&oid) = objects.get(&wid) else {
                continue;
            };
            let material = match &w.kind {
                WorkKind::Road(r) => r.material.map(|m| refs.id(g, &self.materials[m].reference)),
                _ => None,
            };
            // The other kinds' state, by their class block's fields.
            let extra = self.work_fields(g, refs, ids, &w.kind);
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
                    let value = like_saved(f.at(path), ptr_items("citizen", set.map(citizen)));
                    f.set(path, value);
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
                    let assigned = like_saved(
                        f.at("clear.assigned"),
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
                    f.set("clear.assigned", assigned);
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
                _ => {
                    for (path, value) in extra {
                        let value = like_saved(f.at(&path), value);
                        f.set(&path, value);
                    }
                }
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
