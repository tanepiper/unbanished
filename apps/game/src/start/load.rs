//! A town restored from a save (docs/SAVE-LOADING.md). Stage 1: the gameplay map's cells and
//! every placed entity (map footprint, model, natural resource growth and spawning). Stage 2
//! (`construction.rs` here): the buildings' state machines, storage, piles, the town's
//! registries and the job manager's works. Citizens come in stage 3.

use std::collections::BTreeMap;

use banished_formats::game::map_generator;
use banished_formats::pkg::Archive;
use banished_formats::savegame::{ObjId, Value};
use banished_game::growth::Growth;
use banished_game::scheduler::EntityScheduler;
use banished_game::seeding::{Cells, Placed};
use banished_game::spawn::Spawn;
use banished_game::terrain::CellType;

use super::{citizen_rules, load_template, read, StartingTown, TownParts};

mod construction;
use crate::saved::SavedGame;

/// NaturalResourceComponent capability bits in the component flags (`Component +0x20`).
const ALLOW_NEGATIVE: u32 = 0x10_0000;
const REMOVAL_REQUESTED: u32 = 0x20_0000;

/// Fallback when the save's citizen description is missing (as `households` uses).
const RULES: banished_game::citizens::CitizenRules = banished_game::citizens::CitizenRules {
    adult_age: 2.0,
    start_age: (2.2, 8.0),
    lifespan: (15.0, 3.0),
    education: 0,
};

impl StartingTown {
    /// The town of a saved game. Entities without a map footprint (citizens, animals) are not
    /// restored yet.
    pub fn from_save(archive: &Archive, saved: &SavedGame) -> Option<StartingTown> {
        let g = &saved.state;
        // A mod's start condition may be missing; only its citizen template is read here,
        // so the base Medium profile stands in.
        let profile_ref = saved.reference(g.start_condition).unwrap_or_default();
        let profile_data = read(archive, &super::normalise(profile_ref)).or_else(|| {
            log::warn!("{profile_ref}: missing; using Medium");
            read(archive, r"Game\StartCondition\Medium.rsc:resource")
        })?;
        let profile = map_generator(&profile_data)
            .map_err(|e| log::warn!("{profile_ref}: {e}"))
            .ok()?;
        let n = g.map.size.max(0) as usize;
        let mut templates = Vec::new();
        let mut placed = Vec::new();
        let mut scheduler = EntityScheduler::default();
        // The saved entity behind each placed one, and back.
        let mut index: BTreeMap<ObjId, usize> = BTreeMap::new();
        // What was not restored, by template (for the log).
        let mut skipped: BTreeMap<String, usize> = BTreeMap::new();
        for (k, (id, entity)) in saved.entities().enumerate() {
            let reference = saved.ext(&entity.fields, "entity.template").unwrap_or("?");
            let Some(template) = load_template(archive, reference, &mut templates) else {
                *skipped.entry(format!("{reference} (missing)")).or_default() += 1;
                continue;
            };
            let Some(map) = saved.component(entity, "MapComponent") else {
                *skipped.entry(reference.to_string()).or_default() += 1;
                continue;
            };
            let m = |f: &str| map.fields.i32(&format!("map.{f}")).unwrap_or(0);
            let model = saved.component(entity, "ModelComponent");
            let model_f32s = |f: &str| match model.and_then(|m| m.fields.at(&format!("model.{f}")))
            {
                Some(Value::F32s(v)) => v.clone(),
                _ => Vec::new(),
            };
            let position = match model_f32s("position").as_slice() {
                &[x, y, z] => [x, y, z],
                _ => [0.0; 3],
            };
            let natural = saved.component(entity, "NaturalResourceComponent");
            let natural_flags = natural
                .and_then(|c| c.fields.i32("base.flags"))
                .unwrap_or(0) as u32;
            let growth_component = saved.component(entity, "GrowthComponent");
            let mut scale = [1.0; 4];
            let growth_state = growth_component.and_then(|c| {
                let smoothed = c.fields.f32("growth.growth")?;
                if let Some(d) = &templates[template].growth {
                    scale = banished_game::growth::scale(smoothed, d.tall);
                }
                Some(Growth {
                    maximum: c.fields.f32("growth.maximum")?,
                    timer: match c.fields.at("growth.timer")? {
                        Value::I64(t) => *t,
                        _ => 0,
                    },
                    smoothed,
                    scheduled: true,
                    smoothing: false,
                    allow_negative: natural_flags & ALLOW_NEGATIVE != 0,
                })
            });
            // One clock per spawn entry of the template, saved values first (a mod's
            // template can list a different number).
            let entries = templates[template].spawn.len();
            let spawn_state = saved
                .component(entity, "SpawnComponent")
                .map(|c| Spawn {
                    timers: match c.fields.at("spawn.timers") {
                        Some(Value::List(items)) => items
                            .iter()
                            .filter_map(|t| match t.get("timer") {
                                Some(Value::I64(v)) => Some(*v),
                                _ => None,
                            })
                            .collect(),
                        _ => Vec::new(),
                    },
                })
                .map(|mut s| {
                    s.timers.resize(entries, 0);
                    s
                });
            // Entity `+0x1f`: the bucket it is registered in (the schedule's third value).
            let bucket = g
                .schedules
                .get(k)
                .map_or(templates[template].bucket, |s| s.2 as usize);
            let model_i = |f: &str| {
                model
                    .and_then(|m| m.fields.i32(&format!("model.{f}")))
                    .unwrap_or(0)
                    .max(0) as usize
            };
            let p = placed.len();
            placed.push(Placed {
                template,
                bucket,
                cell: (m("x"), m("y")),
                footprint: (m("w"), m("h")),
                position,
                yaw: model
                    .and_then(|m| m.fields.f32("model.yaw"))
                    .unwrap_or(m("rotation") as f32),
                variant: model_i("variant"),
                mesh: model_i("mesh"),
                growth: natural
                    .and_then(|c| c.fields.f32("natural.value_28"))
                    .unwrap_or(1.0),
                growth_state,
                spawn_state,
                alive: true,
                removal_requested: natural_flags & REMOVAL_REQUESTED != 0,
                animation: None,
                scale,
            });
            scheduler.register(p, bucket);
            index.insert(id, p);
        }
        let cells = restore_cells(archive, saved, n, &index, &mut templates);
        let water = banished_game::fire::Water::new(&cells);
        log::info!(
            "loaded {}: {} placed entities, {} templates; not restored yet: {skipped:?}",
            g.town_name,
            placed.len(),
            templates.len()
        );
        let rules = citizen_rules(archive, &profile.citizen).unwrap_or(RULES);
        let mut town = Self::assemble(
            archive,
            &profile.citizen,
            TownParts {
                start: (n as i32 / 2, n as i32 / 2),
                citizens: Vec::new(),
                rules,
                materials: Vec::new(),
                storages: Vec::new(),
                storage_sites: Vec::new(),
                templates,
                placed,
                cells,
                scheduler,
                removed: Vec::new(),
                animation_events: Vec::new(),
                wildlife: Default::default(),
                known: Vec::new(),
                known_materials: Vec::new(),
                water,
            },
        );
        town.restore_construction(archive, saved, &index);
        Some(town)
    }
}

/// The gameplay map's cells as `FUN_14001b570` wrote them (the 48-byte cell layout is in
/// `banished_game::seeding::Cells`).
fn restore_cells(
    archive: &Archive,
    saved: &SavedGame,
    n: usize,
    index: &BTreeMap<ObjId, usize>,
    templates: &mut Vec<banished_game::seeding::Template>,
) -> Cells {
    let map = &saved.state.map;
    let types = map.cells.iter().map(|c| CellType(c.bytes[1])).collect();
    let mut cells = Cells::new(n, types);
    let entity = |e: Option<ObjId>| e.and_then(|e| index.get(&e).copied());
    for (i, c) in map.cells.iter().enumerate() {
        cells.occupants[i] = entity(c.entities[0]);
        cells.objects[i] = entity(c.entities[1]);
        cells.previous_template[i] = saved
            .reference(c.description)
            .and_then(|r| load_template(archive, r, templates));
        // `+0x1c..+0x24` and the u16 at `+0x26` (`+0x25` is not saved).
        cells.layers[i][..9].copy_from_slice(&c.flags);
        cells.layers[i][10..12].copy_from_slice(&c.value.to_le_bytes());
        cells.flags[i] = c.bytes[0];
        cells.road_kind[i] = c.bytes[2];
        cells.road_built[i] = c.bytes[3];
        cells.growth_bytes[i] = c.bytes[4];
    }
    cells.layer_totals[..9].copy_from_slice(&map.trailer);
    cells.road_revision += 1;
    cells
}

#[cfg(test)]
mod tests {
    use super::*;
    use banished_formats::pkg::Package;

    fn root() -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    fn archive() -> Option<Archive> {
        let dir = root().join("game-data/original/WinData");
        let mut a = Archive::default();
        for name in ["data1.pkg", "patch1.pkg", "patch0.pkg", "data0.pkg"] {
            a.add(Package::parse(std::fs::read(dir.join(name)).ok()?).ok()?);
        }
        Some(a)
    }

    /// The vanilla sample saves (no mods): every entity with a map footprint is placed where it
    /// was, each stamped cell points at an entity covering it, and growth and spawn state carry
    /// over.
    #[test]
    fn vanilla_saves_restore_their_placed_entities() {
        let Some(archive) = archive() else {
            eprintln!("game data missing; skipping");
            return;
        };
        for name in ["Save2.sav", "Save3.sav", "Save4.sav", "Save5.sav"] {
            let Ok(bytes) = std::fs::read(root().join("save-game").join(name)) else {
                eprintln!("{name} missing; skipping");
                continue;
            };
            let saved = SavedGame::parse(&bytes).unwrap();
            assert!(saved.metadata.mods.is_empty(), "{name} is not vanilla");
            let town = StartingTown::from_save(&archive, &saved).unwrap();
            let mapped: Vec<_> = saved
                .entities()
                .filter(|(_, e)| saved.component(e, "MapComponent").is_some())
                .collect();
            assert_eq!(town.placed.len(), mapped.len(), "{name}");
            for (p, (_, e)) in town.placed.iter().zip(&mapped) {
                let map = &saved.component(e, "MapComponent").unwrap().fields;
                assert_eq!(
                    p.cell,
                    (map.i32("map.x").unwrap(), map.i32("map.y").unwrap())
                );
                assert_eq!(
                    p.growth_state.is_some(),
                    saved.component(e, "GrowthComponent").is_some()
                );
                if let Some(s) = &p.spawn_state {
                    assert_eq!(s.timers.len(), town.templates[p.template].spawn.len());
                }
            }
            let n = town.cells.size as i32;
            let mut stamped = 0;
            for (i, o) in town.cells.objects.iter().enumerate() {
                let Some(o) = *o else { continue };
                stamped += 1;
                let (x, y) = (i as i32 % n, i as i32 / n);
                let p = &town.placed[o];
                let (w, h) = p.footprint;
                assert!(
                    (p.cell.0..p.cell.0 + w.max(1)).contains(&x)
                        && (p.cell.1..p.cell.1 + h.max(1)).contains(&y),
                    "{name}: cell ({x}, {y}) points at entity {o} at {:?} {:?}",
                    p.cell,
                    p.footprint
                );
            }
            eprintln!(
                "{name}: {} placed, {stamped} stamped cells",
                town.placed.len()
            );
        }
    }
}

/// Stage 2 against the vanilla saves: the restored storages give the stock manager's saved
/// counts; each site's machine is consistent; every site's and pile's work is registered with
/// its entity; planned roads and storage registrations match the saved lists.
#[cfg(test)]
mod stage2_tests {
    use super::tests_support::*;
    use banished_formats::savegame::Value;

    #[test]
    fn vanilla_saves_restore_construction() {
        let Some(archive) = archive() else {
            eprintln!("game data missing; skipping");
            return;
        };
        for name in ["Save2.sav", "Save3.sav", "Save4.sav", "Save5.sav"] {
            let Ok(bytes) = std::fs::read(root().join("save-game").join(name)) else {
                continue;
            };
            let saved = crate::saved::SavedGame::parse(&bytes).unwrap();
            let town = crate::start::StartingTown::from_save(&archive, &saved).unwrap();
            let c = &town.construction;

            // The stock manager's counts are recomputed from storages (`FUN_14008f3a0`).
            let table =
                banished_game::stock::stock_table(&town.registered_storages(), &town.materials);
            assert_eq!(table, saved.stock_warnings().count, "{name}: stock");

            let machines = saved
                .entities()
                .filter(|(_, e)| saved.component(e, "StateMachineComponent").is_some())
                .count();
            assert_eq!(c.sites.len(), machines, "{name}: sites");
            for (&e, site) in &c.sites {
                let m = &site.machine;
                if m.state >= 0 {
                    assert_eq!(
                        m.controllers[m.current].find(m.state),
                        Some(m.index as usize),
                        "{name}: site {e} state {} index {}",
                        m.state,
                        m.index
                    );
                }
                // WorkComponent `+0x28` keeps the id of a work the manager has since removed
                // (a finished build); the port leaves it too and checks it is registered.
                if let Some(w) = c.jobs.works.get(&site.work_id) {
                    assert_eq!(
                        w.entity, e,
                        "{name}: site {e} ({}) work {}",
                        town.templates[town.placed[e].template].reference, site.work_id
                    );
                }
                assert!(
                    !c.placed.contains(&e),
                    "{name}: site {e} queued for activation"
                );
            }
            for (&e, pile) in &c.piles {
                assert_eq!(
                    c.jobs.works.get(&pile.work).map(|w| w.entity),
                    Some(e),
                    "{name}: pile {e} work"
                );
            }
            let saved_works = match saved.state.managers.at("jobs.works") {
                Some(Value::List(w)) => w.len(),
                _ => 0,
            };
            assert_eq!(c.jobs.works.len(), saved_works, "{name}: works");
            let roads = c
                .jobs
                .works
                .values()
                .filter(|w| matches!(w.kind, banished_game::jobs::WorkKind::Road(_)))
                .count();
            assert_eq!(c.pending_roads.len(), roads, "{name}: planned roads");
            let listed = ["town.storages", "town.markets"]
                .iter()
                .map(|l| match saved.state.town.at(l) {
                    Some(Value::List(v)) => v.len(),
                    _ => 0,
                })
                .sum::<usize>();
            assert_eq!(
                town.storage_sites.len(),
                listed,
                "{name}: storage registrations"
            );
            eprintln!(
                "{name}: {} sites, {} piles, {} works ({roads} roads), stock {:?}",
                c.sites.len(),
                c.piles.len(),
                c.jobs.works.len(),
                &table[..8]
            );
        }
    }
}

#[cfg(test)]
pub(crate) mod tests_support {
    use banished_formats::pkg::{Archive, Package};

    pub fn root() -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    pub fn archive() -> Option<Archive> {
        let dir = root().join("game-data/original/WinData");
        let mut a = Archive::default();
        for name in ["data1.pkg", "patch1.pkg", "patch0.pkg", "data0.pkg"] {
            a.add(Package::parse(std::fs::read(dir.join(name)).ok()?).ok()?);
        }
        Some(a)
    }
}
