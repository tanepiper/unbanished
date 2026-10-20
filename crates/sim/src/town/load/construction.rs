//! Stage 2 of loading a save (docs/SAVE-LOADING.md): the buildings' state machines and
//! components, storage, loose piles, the town manager's building registries and the job
//! manager's works.
//!
//! Each site is created as a placed building is (`add_site`, the components' constructors) and
//! then takes its saved fields, as the original's load does: construct, then deserialize. It is
//! not activated again; its saved state machine already holds the state it was in.
//!
//! Citizens are not restored yet (stage 3), so whatever refers to one is dropped: work
//! claims, workers, a pile's claimer. The works stay registered and free.

use std::collections::BTreeMap;

use unbanished_formats::pkg::Archive;
use unbanished_formats::savegame::{Fields, ObjId, Value};
use unbanished_game::work::construction::{
    BuildWork, ClearWork, Decal, Gather, PickupWork, Pile, ProduceWork,
};
use unbanished_game::work::jobs::{Work, WorkKind};
use unbanished_game::work::road::RoadWork;
use unbanished_game::work::stock::Item;

use super::super::{load_template, material_index, normalise, StartingTown};
use crate::saved::SavedGame;

/// WorkComponent `+0x2c` with no kept priority.
const NO_PRIORITY: i32 = i32::MIN;

fn list<'a>(f: &'a Fields, path: &str) -> &'a [Fields] {
    match f.at(path) {
        Some(Value::List(items)) => items,
        _ => &[],
    }
}

fn time(f: &Fields, path: &str) -> i64 {
    match f.at(path) {
        Some(Value::I64(t)) => *t,
        _ => 0,
    }
}

/// A `Bool` field as the game reads it (any non-zero byte).
fn flag(f: &Fields, path: &str) -> bool {
    match f.at(path) {
        Some(Value::Bool(b)) => *b,
        Some(Value::I8(b)) => *b != 0,
        _ => false,
    }
}

fn int(f: &Fields, path: &str) -> i32 {
    f.i32(path).unwrap_or(0)
}

/// The `u32` ids of an `I32sBy` reference array.
fn ids(f: &Fields, path: &str) -> Vec<u32> {
    match f.at(path) {
        Some(Value::U32s(v)) => v.clone(),
        _ => Vec::new(),
    }
}

/// A ComponentEntity, or a component's entity (`base.entity`), as a placed index.
pub(super) fn entity_or_component(
    saved: &SavedGame,
    index: &BTreeMap<ObjId, usize>,
    id: Option<ObjId>,
) -> Option<usize> {
    let id = id?;
    index.get(&id).copied().or_else(|| {
        let e = saved.object(id)?.fields.ptr("base.entity")?;
        index.get(&e).copied()
    })
}

impl StartingTown {
    pub(in crate::town) fn restore_construction(
        &mut self,
        archive: &Archive,
        saved: &SavedGame,
        index: &BTreeMap<ObjId, usize>,
    ) {
        let placed_of = |id: Option<ObjId>| id.and_then(|i| index.get(&i).copied());
        let path_of = |id: u32| saved.reference(id).map(normalise);
        self.restore_sites(archive, saved, index);
        self.restore_piles(archive, saved, index);

        // The town manager's registries (`FUN_140094c30`).
        let town = &saved.state.town;
        // The lists hold entities (`+0x00..+0x30`) or a component of one (the others).
        let entities = |name: &str| -> Vec<usize> {
            list(town, &format!("town.{name}"))
                .iter()
                .filter_map(|e| entity_or_component(saved, index, e.ptr("entity")))
                .collect()
        };
        // The storages (`+0x00`), then the markets (`+0x10`), join as their state 2 entry has
        // them join (`StorageEnter2`, `MarketEnter2`), in the saved order.
        for e in entities("storages").into_iter().chain(entities("markets")) {
            self.construction.storage_changes.push((e, true));
        }
        self.apply_storage_changes();
        self.construction.site_list = entities("sites");
        self.construction.workforce.residences = entities("residences");
        self.construction.healers = entities("healers");
        self.construction.cemeteries = entities("cemeteries");
        self.construction.happiness = entities("happiness");
        self.construction.trading_posts = entities("trading_posts");
        for w in list(town, "town.workplaces") {
            let Some(profession) = w
                .i32("profession")
                .and_then(|id| path_of(id as u32))
                .and_then(|p| self.profession_index(&p))
            else {
                continue;
            };
            // An entry stays when its last workplace leaves (`unregister` keeps it).
            self.construction
                .workforce
                .registry
                .entry(profession)
                .or_default();
            for e in list(w, "entities")
                .iter()
                .filter_map(|e| placed_of(e.ptr("entity")))
            {
                self.construction.workforce.register(e, profession);
            }
        }
        self.construction.known_materials = ids(town, "town.known_materials")
            .into_iter()
            .filter_map(|id| material_index(archive, &mut self.materials, &path_of(id)?))
            .collect();
        self.construction.known = ids(town, "town.known")
            .into_iter()
            .filter_map(|id| load_template(archive, &path_of(id)?, &mut self.templates))
            .collect();

        self.construction.counters = saved.stock_counters();
        // Town manager values: the visitors' disease risk and how long it lasts (`+0x1a8`,
        // `+0x1a0`), the value traded away (`+0x1ac`), the nomads taken in (`+0x1b0`) and the
        // commute clock (`+0x190`).
        self.construction.visitors = (
            town.f32("town.visitor_risk").unwrap_or(0.0),
            time(town, "town.visitor_time"),
        );
        self.construction.trade_value = int(town, "town.trade_value");
        self.construction.nomads_admitted = int(town, "town.nomads_admitted");
        self.construction.commute.timer = time(town, "town.commute_time");
    }

    /// `Game\Profession\Profession.rsc:key` → index in the profession list.
    fn profession_index(&self, path: &str) -> Option<usize> {
        let key = normalise(path);
        self.construction
            .professions
            .list
            .iter()
            .position(|p| normalise(&p.reference) == key)
    }

    /// Every entity with a StateMachineComponent becomes a site with its saved machine, build
    /// progress, work registration, clearing count, storage, workplace and decal.
    fn restore_sites(
        &mut self,
        archive: &Archive,
        saved: &SavedGame,
        index: &BTreeMap<ObjId, usize>,
    ) {
        for (id, entity) in saved.entities() {
            let Some(&p) = index.get(&id) else { continue };
            let Some(machine) = saved.component(entity, "StateMachineComponent") else {
                continue;
            };
            // Trader boats are characters, not sites (stage 3c).
            if saved.component(entity, "BoatComponent").is_some() {
                continue;
            }
            self.add_site(archive, p);
            // Not activated again: the machine is restored below.
            self.construction.placed.retain(|&e| e != p);
            let workplace = saved.component(entity, "WorkPlaceComponent").map(|c| {
                let profession = saved
                    .ext(&c.fields, "workplace.profession")
                    .and_then(|r| self.profession_index(r));
                (profession, flag(&c.fields, "workplace.enabled"))
            });
            let Some(site) = self.construction.sites.get_mut(&p) else {
                log::warn!(
                    "{}: no site template",
                    self.templates[self.placed[p].template].reference
                );
                continue;
            };
            let m = &machine.fields;
            let byte = |f: &str| int(m, &format!("machine.{f}")) as i8;
            let mm = &mut site.machine;
            mm.state = byte("state");
            mm.index = byte("index");
            mm.pending = byte("pending");
            mm.transition = byte("transition");
            // The interface is the last controller holding the state (`FUN_1400aeb30`).
            let last = mm.controllers.len().saturating_sub(1);
            mm.current = (0..mm.controllers.len())
                .rev()
                .find(|&n| mm.controllers[n].find(mm.state).is_some())
                .unwrap_or(last);
            // Inferred: built once past the clearing (0, 0x15, 1) and building (8) states.
            site.complete = mm.state >= 2 && mm.state != 8 && mm.state != 0x15;
            if let Some(b) = saved.component(entity, "BuildComponent") {
                site.progress = int(&b.fields, "build.progress");
                site.stages = int(&b.fields, "build.stages");
            }
            let mut most_workers = 0;
            if let Some(w) = saved.component(entity, "WorkComponent") {
                site.work_id = int(&w.fields, "work.work_id");
                let kept = w.fields.i32("work.kept_priority").unwrap_or(NO_PRIORITY);
                site.work_priority = (kept != NO_PRIORITY).then_some(kept);
                most_workers = int(&w.fields, "work.most_workers");
            }
            if let Some(c) = saved.component(entity, "ClearAreaComponent") {
                site.clear_total = int(&c.fields, "clear.total");
            }
            let storage = site.storage;
            if let Some(d) = saved.component(entity, "DecalComponent") {
                let alpha = d.fields.f32("decal.alpha").unwrap_or(0.0);
                let material = int(&d.fields, "decal.material");
                // Inferred: a decal still in its setup state keeps the description's alpha and
                // material 0; one made opaque or switched is recorded.
                if alpha >= 1.0 || material != 0 {
                    self.construction.decals.insert(
                        p,
                        Decal {
                            opaque: alpha >= 1.0,
                            material,
                        },
                    );
                }
            }
            if let (Some(wp), Some((profession, enabled))) = (
                self.construction.workforce.workplaces.get_mut(&p),
                workplace,
            ) {
                wp.profession = profession;
                wp.enabled = enabled;
                wp.max_workers = most_workers;
            }
            if let Some(s) = saved.component(entity, "StorageComponent") {
                let f = &s.fields;
                let items = list(f, "storage.items")
                    .iter()
                    .filter_map(|i| {
                        let material = saved
                            .ext(i, "material")
                            .and_then(|r| material_index(archive, &mut self.materials, r))?;
                        Some(Item {
                            material,
                            count: int(i, "count"),
                            reserved: int(i, "reserved"),
                        })
                    })
                    .collect();
                let st = &mut self.storages[storage];
                st.capacity = int(f, "storage.capacity");
                st.accepts = int(f, "storage.accepts") as u32;
                st.items = items;
            }
        }
    }

    /// RawMaterialComponent entities: loose piles, their Pickup work set by the job restore.
    fn restore_piles(
        &mut self,
        archive: &Archive,
        saved: &SavedGame,
        index: &BTreeMap<ObjId, usize>,
    ) {
        for (id, entity) in saved.entities() {
            let Some(&p) = index.get(&id) else { continue };
            let Some(raw) = saved.component(entity, "RawMaterialComponent") else {
                continue;
            };
            let reference = self.templates[self.placed[p].template].reference.clone();
            let Some(material) = material_index(archive, &mut self.materials, &reference) else {
                log::warn!("{reference}: no raw material");
                continue;
            };
            let work = saved
                .component(entity, "WorkComponent")
                .map_or(-1, |w| int(&w.fields, "work.work_id"));
            self.construction.piles.insert(
                p,
                Pile {
                    material,
                    count: int(&raw.fields, "raw.count"),
                    age: time(&raw.fields, "raw.age"),
                    work,
                    mask: unbanished_game::town::pile_mask(self.materials[material].flags),
                    priority: None,
                    auto_pickup: None,
                    state: 0,
                    claimer: None,
                },
            );
        }
    }

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
