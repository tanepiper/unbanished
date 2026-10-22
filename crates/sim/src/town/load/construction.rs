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

mod jobs;
mod piles;

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
}
