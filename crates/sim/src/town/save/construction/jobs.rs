//! Exporting the job manager and the fields of each job's work.

use super::*;

/// The original's class for a work kind.
pub(super) fn work_type(kind: &WorkKind) -> &'static str {
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

impl StartingTown {
    /// The fields of the work kinds beyond stage 2's (the inverse of `load::construction`'s
    /// kinds), as `(path, value)` to set.
    pub(super) fn work_fields(
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
    pub(super) fn export_jobs(&self, g: &mut GameState, refs: &mut Refs, ids: &Ids) {
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
