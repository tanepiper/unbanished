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

mod actions;
mod citizens;
mod construction;
mod producers;
mod wildlife;
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
        banished_game::animation::shared::reset();
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
        // The entity manager's clock (`FUN_1400c9970`).
        scheduler.time = g.entity_time;
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
                // Wildlife and boats have no map footprint: a placed entry as `Wild::place`
                // makes (the cell under them), registered in its bucket except a boat's.
                let wild = [
                    "PopulationComponent",
                    "HerdComponent",
                    "AnimalComponent",
                    "BoatComponent",
                ]
                .iter()
                .find(|c| saved.component(entity, c).is_some());
                let Some(&kind) = wild else {
                    *skipped.entry(reference.to_string()).or_default() += 1;
                    continue;
                };
                let position = saved
                    .component(entity, "ModelComponent")
                    .and_then(|m| match m.fields.at("model.position") {
                        Some(Value::F32s(v)) if v.len() == 3 => Some([v[0], v[1], v[2]]),
                        _ => None,
                    })
                    .unwrap_or([0.0; 3]);
                // `Wild::cell_of`.
                let half = (n >> 1) as f32;
                let cell = (
                    ((position[0] + half) as i32).clamp(0, n as i32 - 1),
                    ((position[1] + half) as i32).clamp(0, n as i32 - 1),
                );
                let bucket = g
                    .schedules
                    .get(k)
                    .map_or(templates[template].bucket, |s| s.2 as usize);
                let p = placed.len();
                placed.push(Placed {
                    template,
                    bucket,
                    cell,
                    footprint: (1, 1),
                    position,
                    yaw: 0.0,
                    variant: 0,
                    mesh: usize::MAX,
                    growth: 0.0,
                    growth_state: None,
                    spawn_state: None,
                    alive: true,
                    removal_requested: false,
                    animation: None,
                    scale: [1.0; 4],
                });
                // Boats too (bucket 1 with the citizens), as a new game's are when placed; a
                // placed entity's handle is its index.
                let _ = kind;
                let handle = scheduler.register(p, bucket);
                debug_assert_eq!(handle, p);
                index.insert(id, p);
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
                // −1, no mesh yet (a building before its first stage), is `usize::MAX` in the
                // port (`Sim::set_mesh`).
                mesh: match model.and_then(|m| m.fields.i32("model.mesh")) {
                    Some(m) if m >= 0 => m as usize,
                    Some(_) => usize::MAX,
                    None => 0,
                },
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
        let (citizens, order) = citizens::saved_citizens(saved, &rules);
        let mut town = Self::assemble(
            archive,
            &profile.citizen,
            TownParts {
                start: (n as i32 / 2, n as i32 / 2),
                citizens,
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
        town.restore_citizens(archive, saved, &index, &order);
        town.restore_wildlife(archive, saved, &index);
        town.restore_producers(archive, saved, &index);
        town.restore_jobs(archive, saved, &index, &order);
        town.restore_actions(archive, saved, &index, &order);
        town.restore_water(saved, &index);
        town.restore_character_tree(saved, &index, &order);
        let mut ids = super::SaveIds {
            entities: vec![None; town.placed.len()],
            citizens: vec![0; town.citizens.len()],
            foreign: Vec::new(),
        };
        for (&id, &p) in &index {
            ids.entities[p] = Some(id);
        }
        for (&id, &c) in &order {
            ids.citizens[c] = id;
        }
        town.restore_schedule(saved, &mut ids);
        town.save_ids = Some(ids);
        Some(town)
    }

    /// The town's water sources as saved (`+0xe0`, `+0x130`, `+0x140`): the wells by their
    /// WellComponents, filed in the slots they were saved in. A save before version 5 has no
    /// blocks; the points and blocks are then the scan's, with the wells filed again.
    /// The characters' tree (`Game +0x88`) as the load fills it: each CharacterComponent
    /// put in as it is read (`FUN_14010ef80` → `FUN_140018e00`), in the objects' order.
    fn restore_character_tree(
        &mut self,
        saved: &SavedGame,
        index: &BTreeMap<ObjId, usize>,
        order: &BTreeMap<ObjId, usize>,
    ) {
        use banished_game::character_tree::Who;
        let mut found: Vec<(ObjId, Who)> = Vec::new();
        for (id, e) in saved.entities() {
            let Some(k) = saved.component(e, "CharacterComponent") else {
                continue;
            };
            let who = match index.get(&id).copied() {
                Some(p) if self.wildlife.animals.contains_key(&p) => Who::Animal(p),
                Some(p) if self.wildlife.boats.contains_key(&p) => Who::Boat(p),
                Some(p) if self.wildlife.nomads.contains_key(&p) => Who::Nomad(p),
                _ => match saved
                    .component(e, "CitizenComponent")
                    .and_then(|c| order.get(&c.index).copied())
                {
                    Some(c) => Who::Citizen(c),
                    None => continue,
                },
            };
            found.push((k.index, who));
        }
        found.sort_by_key(|f| f.0);
        let w = &mut self.wildlife;
        for (_, who) in found {
            let ch = match who {
                Who::Citizen(c) => self.agents.characters.get_mut(c),
                Who::Animal(a) => w.animals.get_mut(&a).map(|x| &mut x.character),
                Who::Boat(b) => w.boats.get_mut(&b).map(|x| &mut x.character),
                Who::Nomad(n) => w.nomads.get_mut(&n).map(|x| &mut x.character),
            };
            let Some(ch) = ch else { continue };
            if who_alive(who, &self.citizens) && ch.tree_slot < 0 {
                ch.tree_slot = w.tree.insert(who, ch.position, ch.desc.radius);
                w.tree.set_direction(ch.tree_slot, ch.direction);
            }
        }
        fn who_alive(who: Who, citizens: &[banished_game::citizens::Citizen]) -> bool {
            match who {
                Who::Citizen(c) => citizens.get(c).is_some_and(|c| c.alive),
                _ => true,
            }
        }
    }

    fn restore_water(&mut self, saved: &SavedGame, index: &BTreeMap<ObjId, usize>) {
        let t = &saved.state.town;
        let water = &mut self.construction.water;
        let wells: Vec<Option<usize>> = match t.at("town.wells") {
            Some(Value::List(items)) => items
                .iter()
                .map(|w| {
                    let entity = saved.object(w.ptr("entity")?)?.fields.ptr("base.entity")?;
                    index.get(&entity).copied()
                })
                .collect(),
            _ => Vec::new(),
        };
        let blocks = match t.at("town.values_140") {
            Some(Value::List(items)) if items.len() == water.blocks.len() => Some(
                items
                    .iter()
                    .map(|b| match b.get("values") {
                        Some(Value::U32s(v)) => v.iter().map(|&v| v as i32).collect(),
                        _ => Vec::new(),
                    })
                    .collect::<Vec<Vec<i32>>>(),
            ),
            _ => None,
        };
        let points = match t.at("town.water_sources") {
            Some(Value::List(items)) => items
                .iter()
                .map(|p| (p.i32("a").unwrap_or(0), p.i32("b").unwrap_or(0)))
                .collect(),
            _ => Vec::new(),
        };
        match blocks {
            Some(blocks) => {
                water.points = points;
                water.wells = wells;
                water.blocks = blocks;
            }
            None => {
                for (i, e) in wells.iter().enumerate() {
                    let Some(e) = *e else { continue };
                    let cell = self.placed[e].cell;
                    water.add_entry(cell.0, cell.1, -1 - i as i32);
                }
                water.wells = wells;
            }
        }
    }

    /// The entity manager's buckets as saved (`FUN_1400c9970`): every saved entity gets a
    /// handle (placed ones and citizens theirs, the rest a foreign entry in its saved bucket),
    /// then each bucket takes its saved order, its cursor (`+0xbc`) and each entity its last
    /// visit time.
    fn restore_schedule(&mut self, saved: &SavedGame, ids: &mut super::SaveIds) {
        let g = &saved.state;
        let mut handle_of: std::collections::HashMap<ObjId, usize> = Default::default();
        for (p, e) in ids.entities.iter().enumerate() {
            if let (Some(e), true) = (e, self.scheduler.contains(p)) {
                handle_of.insert(*e, p);
            }
        }
        for (c, &citizen) in ids.citizens.iter().enumerate() {
            let entity = g
                .objects
                .get(&citizen)
                .and_then(|o| o.fields.ptr("base.entity"));
            if let (Some(e), Some(Some(h))) = (entity, self.citizen_handles.get(c)) {
                handle_of.insert(e, *h);
            }
        }
        for (k, e) in g.entities.iter().enumerate() {
            let Some(e) = e else { continue };
            if handle_of.contains_key(e) {
                continue;
            }
            let bucket = g.schedules.get(k).map_or(6, |s| s.2);
            let bucket = if (0..5).contains(&bucket) {
                bucket as usize
            } else {
                6
            };
            let h = self.register_foreign(bucket, ids.foreign.len());
            ids.foreign.push((h, *e));
            handle_of.insert(*e, h);
        }
        let order: [Vec<usize>; 5] = std::array::from_fn(|b| {
            g.buckets.get(b).map_or_else(Vec::new, |l| {
                l.iter()
                    .flatten()
                    .filter_map(|e| handle_of.get(e).copied())
                    .collect()
            })
        });
        let cursors = std::array::from_fn(|b| g.bucket_values[1][b].max(0) as usize);
        let schedules: Vec<(usize, usize, i64)> = g
            .entities
            .iter()
            .zip(&g.schedules)
            .filter_map(|(e, s)| {
                let bucket = if (0..5).contains(&s.2) {
                    s.2 as usize
                } else {
                    6
                };
                Some((*handle_of.get(&(*e)?)?, bucket, s.3))
            })
            .collect();
        self.scheduler.restore(&order, cursors, &schedules);
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
        // `+0x2d`: the floors over the cell, as saved (the active FloorComponents add theirs
        // again when the town is restored, as the original's load does).
        cells.floors.counts[i] = c.bytes[5];
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

    /// The saved wells are the town's again, each a placed well in its saved slot, and the
    /// water's points and blocks are the saved ones.
    #[test]
    fn vanilla_saves_restore_their_wells() {
        let Some(archive) = archive() else {
            eprintln!("game data missing; skipping");
            return;
        };
        let mut total = 0;
        for name in [
            "Save2.sav",
            "Save3.sav",
            "Save4.sav",
            "Save5.sav",
            "Save83.sav",
        ] {
            let Ok(bytes) = std::fs::read(root().join("save-game").join(name)) else {
                continue;
            };
            let saved = SavedGame::parse(&bytes).unwrap();
            let town = StartingTown::from_save(&archive, &saved).unwrap();
            let w = &town.construction.water;
            let Some(Value::List(wells)) = saved.state.town.at("town.wells") else {
                panic!("{name}: no wells");
            };
            assert_eq!(w.wells.len(), wells.len(), "{name}");
            for (slot, e) in w.wells.iter().enumerate() {
                let Some(e) = *e else { continue };
                total += 1;
                let t = &town.templates[town.placed[e].template];
                assert!(t.reference.contains("Well"), "{name}: {}", t.reference);
                assert!(
                    w.blocks.iter().flatten().any(|&v| v == -1 - slot as i32),
                    "{name}: well {slot} is in no block"
                );
            }
            if let Some(Value::List(points)) = saved.state.town.at("town.water_sources") {
                assert_eq!(w.points.len(), points.len(), "{name}");
            }
        }
        eprintln!("{total} wells restored");
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
        for name in [
            "Save2.sav",
            "Save3.sav",
            "Save4.sav",
            "Save5.sav",
            "Save83.sav",
        ] {
            let Ok(bytes) = std::fs::read(root().join("save-game").join(name)) else {
                eprintln!("{name} missing; skipping");
                continue;
            };
            let saved = SavedGame::parse(&bytes).unwrap();
            assert!(saved.metadata.mods.is_empty(), "{name} is not vanilla");
            let town = StartingTown::from_save(&archive, &saved).unwrap();
            // Entities with a footprint, then wildlife and boats (placed since stage 3c).
            let placed: Vec<_> = saved
                .entities()
                .filter(|(_, e)| {
                    [
                        "MapComponent",
                        "PopulationComponent",
                        "HerdComponent",
                        "AnimalComponent",
                        "BoatComponent",
                    ]
                    .iter()
                    .any(|c| saved.component(e, c).is_some())
                })
                .collect();
            assert_eq!(town.placed.len(), placed.len(), "{name}");
            for (p, (_, e)) in town.placed.iter().zip(&placed) {
                let Some(map) = saved.component(e, "MapComponent") else {
                    continue;
                };
                let map = &map.fields;
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
        for name in [
            "Save2.sav",
            "Save3.sav",
            "Save4.sav",
            "Save5.sav",
            "Save83.sav",
        ] {
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

            let machines: Vec<String> = saved
                .entities()
                // Trader boats are characters, not sites (stage 3c).
                .filter(|(_, e)| {
                    saved.component(e, "StateMachineComponent").is_some()
                        && saved.component(e, "BoatComponent").is_none()
                })
                .map(|(_, e)| {
                    saved
                        .ext(&e.fields, "entity.template")
                        .unwrap_or("?")
                        .to_string()
                })
                .collect();
            let mut missing = machines.clone();
            for site in c.sites.values() {
                let r = &town.templates[town.placed[site.entity].template].reference;
                if let Some(i) = missing
                    .iter()
                    .position(|m| crate::start::normalise(m) == *r)
                {
                    missing.remove(i);
                }
            }
            assert_eq!(
                c.sites.len(),
                machines.len(),
                "{name}: sites; not restored: {missing:?}"
            );
            let mut bad = Vec::new();
            for (&e, site) in &c.sites {
                let m = &site.machine;
                if m.state >= 0 && m.controllers[m.current].find(m.state) != Some(m.index as usize)
                {
                    bad.push(format!(
                        "{} state {} index {} controllers {:?} current {}",
                        town.templates[town.placed[e].template].reference,
                        m.state,
                        m.index,
                        m.controllers,
                        m.current
                    ));
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
            bad.sort();
            bad.dedup();
            assert!(bad.is_empty(), "{name}: machines {bad:#?}");
            for (&e, pile) in &c.piles {
                assert_eq!(
                    c.jobs.works.get(&pile.work).map(|w| w.entity),
                    Some(e),
                    "{name}: pile {e} work"
                );
            }
            // Every work in the save (all kinds are restored since stage 3b).
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

/// Stage 3a against the vanilla saves: every citizen restored, and the town's relations agree
/// with each other as the original keeps them.
#[cfg(test)]
mod stage3_tests {
    use super::tests_support::*;

    #[test]
    fn vanilla_saves_restore_citizens() {
        let Some(archive) = archive() else {
            eprintln!("game data missing; skipping");
            return;
        };
        for name in [
            "Save2.sav",
            "Save3.sav",
            "Save4.sav",
            "Save5.sav",
            "Save83.sav",
        ] {
            let Ok(bytes) = std::fs::read(root().join("save-game").join(name)) else {
                continue;
            };
            let saved = crate::saved::SavedGame::parse(&bytes).unwrap();
            let town = crate::start::StartingTown::from_save(&archive, &saved).unwrap();
            let n = saved
                .state
                .objects
                .values()
                .filter(|o| o.type_name == "CitizenComponent")
                .count();
            assert_eq!(town.citizens.len(), n, "{name}: citizens");
            assert_eq!(town.agents.states.len(), n);
            let c = &town.construction;
            for (i, s) in town.agents.states.iter().enumerate() {
                // A family slot's home lists the citizen in that family.
                if let (Some(h), true) = (s.home, s.home_slot >= 0) {
                    let home = c
                        .homes
                        .get(&h)
                        .or_else(|| c.sites.get(&h).and_then(|x| x.home.as_ref()));
                    let family = home.and_then(|h| h.families.get(s.home_slot as usize));
                    assert!(
                        family.is_some_and(|f| f.contains(&i)),
                        "{name}: citizen {i} not in its home"
                    );
                }
                if let Some(sp) = town.citizens[i].spouse {
                    assert_eq!(town.citizens[sp].spouse, Some(i), "{name}: spouse of {i}");
                }
                for &d in &s.dependents {
                    assert_eq!(
                        town.citizens[d].parent,
                        Some(i),
                        "{name}: dependent {d} of {i}"
                    );
                }
                if let Some(w) = s.workplace {
                    if let Some(wp) = c.workforce.workplaces.get(&w) {
                        assert!(
                            wp.workers.contains(&i),
                            "{name}: citizen {i} not among its workplace's workers"
                        );
                    }
                }
            }
            for (&e, wp) in &c.workforce.workplaces {
                for &w in &wp.workers {
                    assert_eq!(
                        town.agents.states[w].workplace,
                        Some(e),
                        "{name}: worker {w} of {e}"
                    );
                }
            }
            let housed = town
                .agents
                .states
                .iter()
                .filter(|s| s.home.is_some())
                .count();
            let working = town
                .agents
                .states
                .iter()
                .filter(|s| s.workplace.is_some())
                .count();
            eprintln!(
                "{name}: {n} citizens, {housed} housed, {working} with a workplace, {} homes",
                c.homes.len()
            );
        }
    }
}

/// Stage 3b against the vanilla saves: every citizen's stack restored at its saved depth, and
/// every work's workers hold that work as their job (Citizen `+0x30`).
#[cfg(test)]
mod stage3b_tests {
    use super::tests_support::*;
    use banished_game::jobs::WorkKind;

    #[test]
    fn vanilla_saves_restore_stacks_and_claims() {
        let Some(archive) = archive() else {
            eprintln!("game data missing; skipping");
            return;
        };
        for name in [
            "Save2.sav",
            "Save3.sav",
            "Save4.sav",
            "Save5.sav",
            "Save83.sav",
        ] {
            let Ok(bytes) = std::fs::read(root().join("save-game").join(name)) else {
                continue;
            };
            let saved = crate::saved::SavedGame::parse(&bytes).unwrap();
            let town = crate::start::StartingTown::from_save(&archive, &saved).unwrap();
            // Saved depths in the citizens' order (`top + 1`).
            let depths: Vec<usize> = match saved.state.town.at("town.citizens") {
                Some(banished_formats::savegame::Value::List(cs)) => cs
                    .iter()
                    .map(|c| {
                        let character = c
                            .ptr("entity")
                            .and_then(|c| saved.object(c))
                            .and_then(|c| c.fields.ptr("base.entity"))
                            .and_then(|e| saved.object(e))
                            .and_then(|e| saved.component(e, "CharacterComponent"));
                        character
                            .and_then(|k| k.fields.i32("character.stack_top"))
                            .map_or(0, |t| (t + 1).max(0) as usize)
                    })
                    .collect(),
                _ => Vec::new(),
            };
            for (c, d) in depths.iter().enumerate() {
                assert_eq!(
                    town.agents.characters[c].stack.depth(),
                    *d,
                    "{name}: stack of {c}"
                );
            }
            let mut held = 0;
            for w in town.construction.jobs.works.values() {
                let workers: Vec<usize> = match &w.kind {
                    WorkKind::Gather(g) => g.workers.iter().copied().collect(),
                    WorkKind::Build(b) => b.workers.iter().copied().collect(),
                    WorkKind::Herbalist(h)
                    | WorkKind::Forester(h)
                    | WorkKind::Hunt(h)
                    | WorkKind::TendArea(h)
                    | WorkKind::Pasture(h)
                    | WorkKind::ClearStorage(h)
                    | WorkKind::Fire(h) => h.workers.iter().copied().collect(),
                    _ => Vec::new(),
                };
                for c in workers {
                    held += 1;
                    assert_eq!(
                        town.agents.states[c].job, w.id,
                        "{name}: worker {c} of work {}",
                        w.id
                    );
                }
            }
            eprintln!(
                "{name}: {} stacks, {held} workers on their works",
                depths.len()
            );
        }
    }
}

/// Stage 3c against the vanilla saves: every population, herd, animal and boat restored; each
/// herd's animals point back at it and each pasture's livestock names it.
#[cfg(test)]
mod stage3c_tests {
    use super::tests_support::*;

    #[test]
    fn vanilla_saves_restore_wildlife() {
        let Some(archive) = archive() else {
            eprintln!("game data missing; skipping");
            return;
        };
        for name in [
            "Save2.sav",
            "Save3.sav",
            "Save4.sav",
            "Save5.sav",
            "Save83.sav",
        ] {
            let Ok(bytes) = std::fs::read(root().join("save-game").join(name)) else {
                continue;
            };
            let saved = crate::saved::SavedGame::parse(&bytes).unwrap();
            let town = crate::start::StartingTown::from_save(&archive, &saved).unwrap();
            let count = |t: &str| {
                saved
                    .state
                    .objects
                    .values()
                    .filter(|o| o.type_name == t)
                    .count()
            };
            let w = &town.wildlife;
            assert_eq!(
                w.populations.len(),
                count("PopulationComponent"),
                "{name}: populations"
            );
            assert_eq!(w.herds.len(), count("HerdComponent"), "{name}: herds");
            assert_eq!(w.animals.len(), count("AnimalComponent"), "{name}: animals");
            assert_eq!(w.boats.len(), count("BoatComponent"), "{name}: boats");
            for (h, herd) in w.herds.iter().enumerate() {
                for a in &herd.animals {
                    assert_eq!(
                        w.animals.get(a).and_then(|a| a.herd),
                        Some(h),
                        "{name}: animal {a} of herd {h}"
                    );
                }
            }
            let mut kept = 0;
            for (&p, pasture) in &w.pastures {
                for a in &pasture.livestock {
                    kept += 1;
                    assert_eq!(
                        w.animals
                            .get(a)
                            .and_then(|a| a.livestock)
                            .and_then(|l| l.pasture),
                        Some(p),
                        "{name}: animal {a} of pasture {p}"
                    );
                }
            }
            eprintln!(
                "{name}: {} herds, {} animals ({kept} in pastures), {} boats",
                w.herds.len(),
                w.animals.len(),
                w.boats.len()
            );
        }
    }
}

/// Stage 3d against the vanilla saves: the floors rebuilt from the FloorComponents count what
/// the saved map cells hold (`+0x2d`); growing crops and standing trees point at their
/// entities, the trees with their fruit; the trading posts' goods resolve.
#[cfg(test)]
mod stage3d_tests {
    use super::tests_support::*;
    use banished_formats::savegame::Value;

    #[test]
    fn vanilla_saves_restore_producers() {
        let Some(archive) = archive() else {
            eprintln!("game data missing; skipping");
            return;
        };
        for name in [
            "Save2.sav",
            "Save3.sav",
            "Save4.sav",
            "Save5.sav",
            "Save83.sav",
        ] {
            let Ok(bytes) = std::fs::read(root().join("save-game").join(name)) else {
                continue;
            };
            let saved = crate::saved::SavedGame::parse(&bytes).unwrap();
            let town = crate::start::StartingTown::from_save(&archive, &saved).unwrap();
            // FloorComponent slot `+0x58` (`FUN_140124a10`) on load: an active floor counts one
            // more on every cell under it (its mesh handle is not saved, so nothing is taken
            // away first), so the counts grow with every load in the original too.
            let n = town.cells.size as i32;
            let mut saved_floors: Vec<u8> =
                saved.state.map.cells.iter().map(|c| c.bytes[5]).collect();
            for (id, e) in saved.entities() {
                let active = saved.component(e, "FloorComponent").is_some_and(|f| {
                    matches!(f.fields.at("floor.active"), Some(Value::Bool(true)))
                });
                let Some(map) = saved.component(e, "MapComponent").filter(|_| active) else {
                    continue;
                };
                let _ = id;
                let m = |k: &str| map.fields.i32(&format!("map.{k}")).unwrap_or(0);
                for y in m("y")..m("y") + m("h") {
                    for x in m("x")..m("x") + m("w") {
                        saved_floors[(y * n + x) as usize] += 1;
                    }
                }
            }
            let wrong: Vec<(usize, u8, u8)> = town
                .cells
                .floors
                .counts
                .iter()
                .zip(&saved_floors)
                .enumerate()
                .filter(|(_, (a, b))| a != b)
                .map(|(i, (a, b))| (i, *a, *b))
                .collect();
            if !wrong.is_empty() {
                let mut kinds = std::collections::BTreeMap::new();
                for &(i, _, _) in &wrong {
                    let t = town.cells.objects[i]
                        .or(town.cells.occupants[i])
                        .map(|e| {
                            let t = &town.templates[town.placed[e].template];
                            format!("{} floor {}", t.reference, t.floor.is_some())
                        })
                        .unwrap_or_default();
                    *kinds.entry(t).or_insert(0) += 1;
                }
                eprintln!("{name}: differing floor cells by object: {kinds:?}");
            }
            assert!(
                wrong.is_empty(),
                "{name}: {} floor cells differ (cell, ours, saved), first {:?}",
                wrong.len(),
                &wrong[..wrong.len().min(8)]
            );
            let c = &town.construction;
            let (mut crops, mut trees) = (0, 0);
            for site in c.sites.values() {
                if let Some(f) = &site.crop_field {
                    for cell in f.cells.iter().filter(|c| c.state == 1) {
                        crops += 1;
                        let e = cell.entity.expect("growing crop without an entity");
                        assert!(town.placed[e].alive, "{name}: crop {e}");
                    }
                }
                if let Some(o) = &site.orchard {
                    for t in o.trees.iter().filter(|t| t.state == 1) {
                        trees += 1;
                        let e = t.entity.expect("standing tree without an entity");
                        assert!(
                            c.fruits.contains_key(&e),
                            "{name}: tree {e} without its fruit"
                        );
                    }
                }
                if let Some(t) = &site.trade {
                    let saved_offers = saved
                        .entities()
                        .filter_map(|(_, e)| saved.component(e, "TradeComponent"))
                        .map(|t| match t.fields.at("trade.stock") {
                            Some(Value::List(v)) => v.len(),
                            _ => 0,
                        })
                        .max()
                        .unwrap_or(0);
                    assert!(t.offers.len() <= saved_offers);
                }
            }
            let floors = town.cells.floors.counts.iter().filter(|&&n| n > 0).count();
            eprintln!(
                "{name}: {crops} growing crops, {trees} standing trees, {floors} floor cells"
            );
        }
    }
}

/// Stage 4 against the vanilla saves: the environment's state with snow cover agreeing with
/// the saved temperature (`FUN_1400988f0`), the event log with its texts, and the Town Info
/// history.
#[cfg(test)]
mod stage4_tests {
    use super::tests_support::*;
    use banished_formats::savegame::Value;

    #[test]
    fn vanilla_saves_restore_environment_and_logs() {
        let Some(archive) = archive() else {
            eprintln!("game data missing; skipping");
            return;
        };
        for name in [
            "Save2.sav",
            "Save3.sav",
            "Save4.sav",
            "Save5.sav",
            "Save83.sav",
        ] {
            let Ok(bytes) = std::fs::read(root().join("save-game").join(name)) else {
                continue;
            };
            let saved = crate::saved::SavedGame::parse(&bytes).unwrap();
            let climate = saved
                .reference(saved.state.climate)
                .unwrap()
                .replace('\\', "/");
            let generator =
                banished_formats::game::climate_generator(&archive.read_ref(&climate).unwrap())
                    .unwrap();
            let mut env = banished_game::environment::Environment::new(
                banished_game::environment::Climate::from(&generator),
            );
            saved.restore_environment(&mut env);
            let m = &saved.state.managers;
            assert_eq!(
                Some(env.temperature),
                m.f32("environment.temperature"),
                "{name}"
            );
            assert_eq!(
                env.season.snow,
                (32.0 - env.temperature).clamp(0.0, 1.0),
                "{name}: snow"
            );
            let log = saved.event_log();
            let entries = match m.at("event_log.entries") {
                Some(Value::List(v)) => v.len(),
                _ => 0,
            };
            assert_eq!(log.entries.len(), entries, "{name}: events");
            assert!(
                log.entries.iter().all(|e| e.text.is_some()),
                "{name}: event texts"
            );
            let stats = saved.statistics();
            let history = match saved.state.town.at("town.history") {
                Some(Value::List(v)) => v.len(),
                _ => 0,
            };
            assert!(history <= stats.series.len(), "{name}: {history} series");
            eprintln!(
                "{name}: {:.1}°F state {} level {}, {} events, {} samples in the first series",
                env.temperature,
                env.state,
                env.level,
                log.entries.len(),
                stats.series.first().map_or(0, |s| s.len())
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
