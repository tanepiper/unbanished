//! Restoring the job manager: every work, its categories, profession, claims and workers.

use super::*;

impl StartingTown {
    /// The job manager (`FUN_1400fe710`): every work in the id table, with the categories it is
    /// filed under, its profession from the profession map, and its claims and workers
    /// (`order` gives each CitizenComponent's citizen index). Called after the citizens.
    pub(in crate::town) fn restore_jobs(
        &mut self,
        archive: &Archive,
        saved: &SavedGame,
        index: &BTreeMap<ObjId, usize>,
        order: &BTreeMap<ObjId, usize>,
    ) {
        let citizen = |id: Option<ObjId>| id.and_then(|c| order.get(&c).copied());
        let m = &saved.state.managers;
        let mut categories: BTreeMap<ObjId, Vec<u32>> = BTreeMap::new();
        for c in 1..=14u32 {
            for w in list(m, &format!("jobs.category_{c}")) {
                if let Some(w) = w.ptr("work") {
                    categories.entry(w).or_default().push(c);
                }
            }
        }
        let mut professions: BTreeMap<ObjId, usize> = BTreeMap::new();
        let mut keys = Vec::new();
        for p in list(m, "jobs.by_profession") {
            // A null profession is a key of its own (`usize::MAX`).
            let profession = match p.i32("profession") {
                Some(-1) => usize::MAX,
                _ => match saved
                    .ext(p, "profession")
                    .and_then(|r| self.profession_index(r))
                {
                    Some(p) => p,
                    None => continue,
                },
            };
            keys.push(profession);
            for w in list(p, "work") {
                if let Some(w) = w.ptr("work") {
                    professions.insert(w, profession);
                }
            }
        }
        let laborer = self.construction.professions.laborer;
        let mut works = Vec::new();
        let mut skipped: BTreeMap<&str, usize> = BTreeMap::new();
        for w in list(m, "jobs.works") {
            let Some((id, o)) = w.ptr("work").and_then(|id| Some((id, saved.object(id)?))) else {
                continue;
            };
            let f = &o.fields;
            // A road's work has no entity (`usize::MAX` in the port, `FUN_1400a1e30`).
            let entity = match f.ptr("work.entity") {
                None if o.type_name == "BuildRoad" => usize::MAX,
                e => match e.and_then(|e| index.get(&e).copied()) {
                    Some(e) => e,
                    None => {
                        *skipped.entry(o.type_name).or_default() += 1;
                        continue;
                    }
                },
            };
            // The class block's name for its fields.
            let block = match o.type_name {
                "ForesterWork" => "forester",
                "HerbalistWork" => "herbalist",
                "HuntWork" => "hunt",
                "PastureWork" => "pasture",
                "TendArea" => "tend",
                "ClearStorage" => "clear_storage",
                "FireWork" => "fire",
                "PlantArea" | "PlantOrchard" => "plant",
                "HarvestArea" | "HarvestOrchard" => "harvest",
                _ => "",
            };
            let citizens = |path: &str| -> std::collections::BTreeSet<usize> {
                list(f, path)
                    .iter()
                    .filter_map(|c| citizen(c.ptr("citizen")))
                    .collect()
            };
            // `HerbalistWork`-shaped state: the workers and the misses in a row (`+0x58`).
            let workers =
                |misses: Option<&str>| unbanished_game::production::herbalist::HerbalistWork {
                    workers: citizens(&format!("{block}.citizens")),
                    misses: misses.map_or(0, |m| int(f, &format!("{block}.{m}"))),
                };
            let area = |crop: Option<usize>| unbanished_game::production::farm::AreaWork {
                claims: list(f, &format!("{block}.keyed"))
                    .iter()
                    .filter_map(|k| Some((int(k, "key"), citizen(k.ptr("citizen"))?)))
                    .collect(),
                need: list(f, &format!("{block}.cells"))
                    .iter()
                    .map(|b| flag(b, "done"))
                    .collect(),
                crop,
            };
            let kind = match o.type_name {
                "BuildRoad" => {
                    let cell = int(f, "road.cell");
                    self.construction
                        .pending_roads
                        .insert(cell, int(f, "work.id"));
                    WorkKind::Road(RoadWork {
                        builder: citizen(f.ptr("road.builder")),
                        material: saved
                            .ext(f, "road.material")
                            .and_then(|r| material_index(archive, &mut self.materials, r)),
                        cell,
                        ty: int(f, "road.type") as u8,
                        old_kind: int(f, "road.old_kind") as u8,
                        old_built: int(f, "road.old_built") as u8,
                        old_type: int(f, "road.old_type") as u8,
                    })
                }
                "BuildStructure" => WorkKind::Build(BuildWork {
                    destroy: flag(f, "build.destroy"),
                    cursor: int(f, "build.cursor"),
                    occupied: list(f, "build.occupied")
                        .iter()
                        .map(|b| flag(b, "done"))
                        .collect(),
                    workers: citizens("build.citizens"),
                }),
                "ClearArea" => WorkKind::Clear(ClearWork {
                    resources: list(f, "clear.assigned")
                        .iter()
                        .filter_map(|a| {
                            Some((
                                index.get(&a.ptr("entity")?).copied()?,
                                citizen(a.ptr("citizen"))?,
                            ))
                        })
                        .collect(),
                    cells: list(f, "clear.keyed")
                        .iter()
                        .filter_map(|k| Some((int(k, "key"), citizen(k.ptr("citizen"))?)))
                        .collect(),
                    filter: saved.ext(f, "clear.filter").map(normalise),
                    clear_resources: flag(f, "clear.clear_resources"),
                    clear_cells: flag(f, "clear.clear_cells"),
                    clear_piles: flag(f, "clear.clear_piles"),
                }),
                "GatherRawMaterials" => {
                    let values = |name: &str| {
                        list(f, &format!("gather.{name}"))
                            .iter()
                            .map(|v| int(v, "value"))
                            .collect()
                    };
                    WorkKind::Gather(Gather {
                        stored: values("stored"),
                        transit: values("transit"),
                        workers: citizens("gather.citizens"),
                    })
                }
                "Pickup" => {
                    if let Some(pile) = self.construction.piles.get_mut(&entity) {
                        pile.state = int(f, "pickup.state");
                        pile.claimer = citizen(f.ptr("pickup.claimer"));
                    }
                    WorkKind::Pickup(PickupWork)
                }
                "ConsumeProduce" => WorkKind::Produce(ProduceWork {
                    workers: list(f, "consume.citizens")
                        .iter()
                        .map(|c| citizen(c.ptr("citizen")))
                        .collect(),
                    points: list(f, "consume.points")
                        .iter()
                        .map(|v| int(v, "state") as u8)
                        .collect(),
                    full: flag(f, "consume.full"),
                    cycles: int(f, "consume.cycles"),
                }),
                "ForesterWork" => WorkKind::Forester(workers(Some("value_58"))),
                "HerbalistWork" => WorkKind::Herbalist(workers(Some("value_58"))),
                "HuntWork" => WorkKind::Hunt(workers(None)),
                "PastureWork" => WorkKind::Pasture(workers(None)),
                "TendArea" => WorkKind::TendArea(workers(Some("value_58"))),
                "ClearStorage" => WorkKind::ClearStorage(workers(None)),
                "FireWork" => WorkKind::Fire(workers(None)),
                "FishingWork" => {
                    WorkKind::Fishing(unbanished_game::production::fishing::FishingWork {
                        workers: list(f, "fishing.citizens")
                            .iter()
                            .map(|c| citizen(c.ptr("citizen")))
                            .collect(),
                        points: list(f, "fishing.values")
                            .iter()
                            .map(|v| int(v, "value"))
                            .collect(),
                        misses: int(f, "fishing.value_68"),
                    })
                }
                "LocationWork" => {
                    WorkKind::Location(unbanished_game::people::services::LocationWork {
                        worker: citizen(f.ptr("location.citizen")),
                    })
                }
                "TradeWork" => WorkKind::Trade(unbanished_game::commerce::trade::TradeWork {
                    haulers: list(f, "trade.orders")
                        .iter()
                        .filter_map(|o| {
                            let c = citizen(o.ptr("citizen"))?;
                            let m = material_index(
                                archive,
                                &mut self.materials,
                                &normalise(saved.ext(o, "description")?),
                            )?;
                            Some((c, (m, int(o, "count"))))
                        })
                        .collect(),
                }),
                "MarketWork" => WorkKind::Market(unbanished_game::commerce::market::MarketWork {
                    plans: list(f, "market.orders")
                        .iter()
                        .filter_map(|o| {
                            let c = citizen(o.ptr("citizen"))?;
                            let items = list(o, "items")
                                .iter()
                                .filter_map(|i| {
                                    let m = material_index(
                                        archive,
                                        &mut self.materials,
                                        &normalise(saved.ext(i, "description")?),
                                    )?;
                                    Some((m, int(i, "count")))
                                })
                                .collect();
                            Some((c, items))
                        })
                        .collect(),
                }),
                "PlantArea" | "PlantOrchard" => {
                    let crop = saved
                        .ext(f, "plant.description")
                        .and_then(|r| load_template(archive, &normalise(r), &mut self.templates));
                    if o.type_name == "PlantArea" {
                        WorkKind::PlantArea(area(crop))
                    } else {
                        WorkKind::PlantOrchard(area(crop))
                    }
                }
                "HarvestArea" => WorkKind::Harvest(area(None)),
                "HarvestOrchard" => WorkKind::PickFruit(area(None)),
                other => {
                    *skipped.entry(other).or_default() += 1;
                    continue;
                }
            };
            let mut work = Work::new(
                entity,
                int(f, "work.cell"),
                *professions.get(&id).unwrap_or(&laborer),
                kind,
            );
            work.id = int(f, "work.id");
            work.priority = int(f, "work.priority");
            work.patience = time(f, "work.patience");
            work.patience_base = time(f, "work.patience_base");
            work.far_count = int(f, "work.far_count");
            works.push((work, categories.remove(&id).unwrap_or_default()));
        }
        if !skipped.is_empty() {
            log::warn!("works not restored yet: {skipped:?}");
        }
        log::info!("restored {} works", works.len());
        self.construction.jobs.restore(
            int(m, "jobs.next_id"),
            int(m, "jobs.next_priority"),
            int(m, "jobs.pass"),
            works,
            &keys,
        );
        // NaturalResourceComponent flag `0x100000`: a removal sequence holds the resource.
        for (id, entity) in saved.entities() {
            let Some(&e) = index.get(&id) else { continue };
            if let Some(n) = saved.component(entity, "NaturalResourceComponent") {
                if int(&n.fields, "base.flags") as u32 & 0x10_0000 != 0 {
                    self.construction.claimed_resources.insert(e);
                }
            }
        }
    }
}
