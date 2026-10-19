//! Saving a town (docs/SAVE-WRITING.md): the inverse of `load`, writing the port's state back
//! into the game state it was loaded from. Every field `load` reads, this writes, by the same
//! offsets; a value derived on load (a model's rotation from its yaw) is rewritten only when
//! the port's value differs, so an untouched object keeps its exact bits.

use banished_formats::savegame::{Fields, GameState, ObjId, Value};

use super::{normalise, StartingTown};

mod actions;
mod citizens;
mod construction;
mod create;
pub(crate) mod new_game;
mod producers;
mod wildlife;

/// The saved object of each entity and citizen, extended with what was made since the load.
pub(crate) struct Ids {
    pub entities: Vec<Option<ObjId>>,
    pub citizens: Vec<Option<ObjId>>,
}

impl Ids {
    pub fn entity(&self, p: usize) -> Option<ObjId> {
        self.entities.get(p).copied().flatten()
    }

    pub fn citizen(&self, c: usize) -> Option<ObjId> {
        self.citizens.get(c).copied().flatten()
    }
}

/// The external reference table (`ExternalFactory::SerializeReferenceTable`) by path: a path
/// the save does not have yet gets the next id.
pub(crate) struct Refs {
    ids: std::collections::BTreeMap<String, u32>,
}

impl Refs {
    pub(crate) fn new(g: &GameState) -> Refs {
        Refs {
            ids: g
                .references
                .iter()
                .map(|(&id, p)| (normalise(p), id))
                .collect(),
        }
    }

    pub(crate) fn id(&mut self, g: &mut GameState, path: &str) -> u32 {
        let key = normalise(path);
        if let Some(&id) = self.ids.get(&key) {
            return id;
        }
        let id = g.references.keys().max().map_or(1, |m| m + 1);
        g.references.insert(id, key.clone());
        self.ids.insert(key, id);
        id
    }
}

/// An entity's component of one type (its object id).
pub(crate) fn component(g: &GameState, entity: ObjId, type_name: &str) -> Option<ObjId> {
    let e = g.objects.get(&entity)?;
    match e.fields.at("entity.components") {
        Some(Value::List(items)) => items
            .iter()
            .filter_map(|c| c.ptr("component"))
            .find(|c| g.objects.get(c).is_some_and(|o| o.type_name == type_name)),
        _ => None,
    }
}

/// The fields of an entity's component of one type, for writing.
/// The capability bits that follow a component's state (update bit `0x04` while a status icon
/// shows or a particle slot is on), then each entity's, its components' together. Every saved
/// component and entity agrees with these.
fn finish_flags(g: &mut GameState) {
    for o in g.objects.values_mut() {
        let on = match o.type_name {
            "StatusIconComponent" => o.fields.i32("status.bits").map(|b| b != 0),
            "ParticleComponent" => match o.fields.at("particle.slots") {
                Some(Value::List(l)) => Some(
                    l.iter()
                        .any(|s| matches!(s.get("on"), Some(Value::Bool(true)))),
                ),
                _ => None,
            },
            _ => None,
        };
        if let (Some(on), Some(f)) = (on, o.fields.i32("base.flags")) {
            let f = if on { f | 0x04 } else { f & !0x04 };
            o.fields.set_i32("base.flags", f);
        }
    }
    let entities: Vec<ObjId> = g.entities.iter().flatten().copied().collect();
    for e in entities {
        if let Some(bits) = create::entity_flags(g, e) {
            if let Some(o) = g.objects.get_mut(&e) {
                o.fields.set_i32("entity.flags", bits as i32);
            }
        }
    }
}

pub(crate) fn fields_mut<'a>(
    g: &'a mut GameState,
    entity: ObjId,
    type_name: &str,
) -> Option<&'a mut Fields> {
    let c = component(g, entity, type_name)?;
    g.objects.get_mut(&c).map(|o| &mut o.fields)
}

fn f32s(f: &Fields, path: &str) -> Option<Vec<f32>> {
    match f.at(path) {
        Some(Value::F32s(v)) => Some(v.clone()),
        _ => None,
    }
}

impl StartingTown {
    /// Write the town into `g`, the game state it was loaded from: objects for what is new,
    /// what is gone dropped, then every field.
    pub fn export_into(&self, archive: &banished_formats::pkg::Archive, g: &mut GameState) {
        let foreign = self
            .save_ids
            .as_ref()
            .map_or_else(Vec::new, |s| s.foreign.clone());
        self.export_into_with(archive, g, &foreign);
    }

    /// [`export_into`](Self::export_into), with the scheduler entities the town does not model
    /// and the saved ComponentEntity of each (a new game's ambience regions).
    pub fn export_into_with(
        &self,
        archive: &banished_formats::pkg::Archive,
        g: &mut GameState,
        foreign: &[(usize, ObjId)],
    ) {
        // A new game has no saved objects: every entity is made.
        let new_game = crate::start::SaveIds::default();
        let saved = self.save_ids.as_ref().unwrap_or(&new_game);
        let mut refs = Refs::new(g);
        let mut ids = self.sync_entities(archive, g, &mut refs, saved);
        self.sync_citizens(archive, g, &mut refs, &mut ids);
        g.entity_time = self.scheduler.time;
        self.export_entities(g, &ids.entities);
        self.export_cells(g, &ids.entities);
        self.export_construction(g, &mut refs, &ids);
        self.export_citizens(archive, g, &mut refs, &ids);
        self.export_wildlife(g, &mut refs, &ids);
        self.export_producers(g, &mut refs, &ids);
        self.export_zones(g, &mut refs, &ids);
        if self.save_ids.is_none() {
            self.export_water(g, &ids);
        }
        self.export_schedule(g, &ids, foreign);
        finish_flags(g);
    }

    /// The entity manager's buckets (`FUN_1400c9970`): each bucket's entities in the
    /// scheduler's order, its cursor, and each entity's schedule (list index, bucket slot or
    /// −1, bucket, last visit time).
    fn export_schedule(&self, g: &mut GameState, ids: &Ids, foreign: &[(usize, ObjId)]) {
        use std::collections::HashMap;
        let mut obj_of: HashMap<usize, ObjId> = HashMap::new();
        for p in 0..self.placed.len() {
            if let (Some(e), true) = (ids.entity(p), self.scheduler.contains(p)) {
                obj_of.insert(p, e);
            }
        }
        for (c, h) in self.citizen_handles.iter().enumerate() {
            let entity = ids
                .citizen(c)
                .and_then(|c| g.objects.get(&c))
                .and_then(|o| o.fields.ptr("base.entity"));
            if let (Some(h), Some(e)) = (h, entity) {
                obj_of.insert(*h, e);
            }
        }
        for &(h, e) in foreign {
            obj_of.insert(h, e);
        }
        let mut slots: HashMap<ObjId, (usize, i32)> = HashMap::new();
        g.buckets.resize(5, Vec::new());
        for b in 0..5 {
            let list: Vec<Option<ObjId>> = self
                .scheduler
                .bucket(b)
                .iter()
                .filter_map(|h| obj_of.get(h).map(|&e| Some(e)))
                .collect();
            for (i, e) in list.iter().enumerate() {
                slots.insert(e.unwrap(), (b, i as i32));
            }
            g.buckets[b] = list;
            g.bucket_values[1][b] = self.scheduler.cursor(b) as i32;
        }
        let handle_of: HashMap<ObjId, usize> = obj_of.iter().map(|(&h, &e)| (e, h)).collect();
        let entities = g.entities.clone();
        for (k, e) in entities.iter().enumerate() {
            let Some(&h) = e.as_ref().and_then(|e| handle_of.get(e)) else {
                continue;
            };
            let (bucket, time) = self.scheduler.schedule(h);
            let (bucket, slot) = slots.get(&e.unwrap()).copied().unwrap_or((bucket, -1));
            if let Some(s) = g.schedules.get_mut(k) {
                *s = (k as i32, slot, bucket as i32, time);
            }
        }
    }

    /// The town's zones by template (`+0x160`, `FUN_1400186e0`): every placed entity with a
    /// ZoneComponent, registered when it activates (`FUN_1401356c0` → `FUN_14008e2f0`), built
    /// or not. Keyed by the description's address in the original, so a saved game keeps its
    /// keys' order (and its emptied keys) and new ones follow in placement order.
    fn export_zones(&self, g: &mut GameState, refs: &mut Refs, ids: &Ids) {
        use construction::{in_saved_order, like_saved, saved_order};
        let mut keys: Vec<u32> = Vec::new();
        let mut members: std::collections::BTreeMap<u32, Vec<Option<ObjId>>> = Default::default();
        for (p, placed) in self.placed.iter().enumerate() {
            let Some(id) = ids.entity(p).filter(|_| placed.alive) else {
                continue;
            };
            if component(g, id, "ZoneComponent").is_none() {
                continue;
            }
            let key = refs.id(g, &self.templates[placed.template].reference);
            if !keys.contains(&key) {
                keys.push(key);
            }
            members.entry(key).or_default().push(Some(id));
        }
        let saved = g.town.at("town.zones").cloned();
        let order = saved_order(saved.as_ref(), "description");
        for &k in &order {
            if !keys.contains(&k) {
                keys.push(k);
            }
        }
        let old = |k: u32| match &saved {
            Some(Value::List(l)) => l
                .iter()
                .find(|f| f.get("description") == Some(&Value::Ext(k)))
                .and_then(|f| f.get("entities").cloned()),
            _ => None,
        };
        let items: Vec<(u32, Fields)> = keys
            .iter()
            .map(|&k| {
                let list = Value::List(
                    members
                        .remove(&k)
                        .unwrap_or_default()
                        .into_iter()
                        .map(|e| Fields(vec![("entity", Value::Ptr(e))]))
                        .collect(),
                );
                let entities = like_saved(old(k).as_ref(), list);
                (
                    k,
                    Fields(vec![("description", Value::Ext(k)), ("entities", entities)]),
                )
            })
            .collect();
        g.town
            .set("town.zones", Value::List(in_saved_order(&order, items)));
    }

    /// A new game's water (`fire::Water`): the shore points (`+0x130`, `FUN_1400a43f0`), the
    /// wells' WellComponents (`+0xe0`) and the points and wells by 8 × 8 block (`+0x140`).
    /// A loaded game keeps its saved lists (the port rebuilds the points from its cells).
    fn export_water(&self, g: &mut GameState, ids: &Ids) {
        let w = &self.construction.water;
        let points = w
            .points
            .iter()
            .map(|&(a, b)| Fields(vec![("a", Value::I32(a)), ("b", Value::I32(b))]))
            .collect();
        let wells = w
            .wells
            .iter()
            .map(|e| {
                let well = e
                    .and_then(|e| ids.entity(e))
                    .and_then(|id| component(g, id, "WellComponent"));
                Fields(vec![("entity", Value::Ptr(well))])
            })
            .collect();
        let blocks = w
            .blocks
            .iter()
            .map(|b| {
                Fields(vec![
                    ("count", Value::I32(b.len() as i32)),
                    ("values", Value::U32s(b.iter().map(|&v| v as u32).collect())),
                ])
            })
            .collect();
        g.town.set("town.water_sources", Value::List(points));
        g.town.set("town.wells", Value::List(wells));
        g.town.set("town.values_140", Value::List(blocks));
    }

    /// Stage 1: the placed entities' map, model, natural resource, growth and spawning, and
    /// each entity's bucket (its schedule's third value).
    fn export_entities(&self, g: &mut GameState, entities: &[Option<ObjId>]) {
        // The schedules are in the entity manager's order.
        let order: std::collections::BTreeMap<ObjId, usize> = g
            .entities
            .iter()
            .enumerate()
            .filter_map(|(k, e)| Some(((*e)?, k)))
            .collect();
        for (p, placed) in self.placed.iter().enumerate() {
            let Some(id) = entities.get(p).copied().flatten() else {
                continue;
            };
            if let Some(&k) = order.get(&id) {
                if let Some(s) = g.schedules.get_mut(k) {
                    s.2 = placed.bucket as i32;
                }
            }
            let natural = component(g, id, "NaturalResourceComponent").is_some();
            let area = self.templates[placed.template].fenced_area;
            if let Some(m) = fields_mut(g, id, "MapComponent") {
                m.set_i32("map.x", placed.cell.0);
                m.set_i32("map.y", placed.cell.1);
                m.set_i32("map.w", placed.footprint.0);
                m.set_i32("map.h", placed.footprint.1);
                // `+0x30`: the placing tool's whole degrees, wrapped into 0..360
                // (`FUN_1400e02d0`, `FUN_140058480`); stamped natural resources keep 0.
                if !natural {
                    let mut degrees = placed.yaw as i32;
                    while degrees < 0 {
                        degrees += 360;
                    }
                    m.set_i32("map.rotation", degrees);
                }
            }
            // Wildlife and boats keep their body in their character; their model is written
            // with them.
            let wild =
                self.wildlife.animals.contains_key(&p) || self.wildlife.boats.contains_key(&p);
            let model = if wild {
                None
            } else {
                fields_mut(g, id, "ModelComponent")
            };
            if let Some(m) = model {
                if f32s(m, "model.position").as_deref() != Some(&placed.position[..]) {
                    m.set("model.position", Value::F32s(placed.position.to_vec()));
                }
                // A fenced area's model is never placed: yaw 0, the identity.
                let yaw = if area { 0.0 } else { placed.yaw };
                if area {
                    m.set("model.rotation", Value::F32s(vec![0.0, 0.0, 0.0, 1.0]));
                    m.set_f32("model.yaw", 0.0);
                } else {
                    // The model's rotation: `FromYawPitchOffset(yaw, 0)` (`(-0, -0, -sin, cos)` of
                    // the half angle), bit for bit (a template's default is the identity's +0).
                    // Natural resources too: every one in the vanilla saves holds this.
                    let q = banished_game::math::Transform::of_model(yaw, [0.0; 3]).q;
                    let bits = |v: &[f32]| v.iter().map(|x| x.to_bits()).collect::<Vec<_>>();
                    let stored = f32s(m, "model.rotation").map(|v| bits(&v));
                    if m.f32("model.yaw").map(f32::to_bits) != Some(yaw.to_bits())
                        || stored != Some(bits(&q))
                    {
                        m.set_f32("model.yaw", yaw);
                        m.set("model.rotation", Value::F32s(q.to_vec()));
                    }
                }
                m.set_i32("model.variant", placed.variant as i32);
                // `usize::MAX`: no mesh (−1).
                m.set_i32(
                    "model.mesh",
                    if placed.mesh == usize::MAX {
                        -1
                    } else {
                        placed.mesh as i32
                    },
                );
            }
            if let Some(n) = fields_mut(g, id, "NaturalResourceComponent") {
                if n.f32("natural.value_28") != Some(placed.growth) {
                    n.set_f32("natural.value_28", placed.growth);
                }
                let flags = n.i32("base.flags").unwrap_or(0) as u32;
                let mut f = flags & !0x20_0000;
                if placed.removal_requested {
                    f |= 0x20_0000;
                }
                n.set_i32("base.flags", f as i32);
            }
            if let (Some(gs), Some(gr)) =
                (&placed.growth_state, fields_mut(g, id, "GrowthComponent"))
            {
                gr.set_f32("growth.growth", gs.smoothed);
                gr.set_f32("growth.maximum", gs.maximum);
                gr.set_time("growth.timer", gs.timer);
            }
            if let (Some(sp), Some(f)) = (&placed.spawn_state, fields_mut(g, id, "SpawnComponent"))
            {
                if let Some(Value::List(items)) = f.at_mut("spawn.timers") {
                    for (item, &t) in items.iter_mut().zip(&sp.timers) {
                        item.set_time("timer", t);
                    }
                }
            }
        }
    }

    /// Stage 1: the gameplay map's cells (`FUN_14001b570`).
    fn export_cells(&self, g: &mut GameState, entities: &[Option<ObjId>]) {
        let id = |p: Option<usize>| p.and_then(|p| entities.get(p).copied().flatten());
        let c = &self.cells;
        for (i, cell) in g.map.cells.iter_mut().enumerate() {
            if c.objects[i].is_none() || id(c.objects[i]).is_some() {
                cell.entities[1] = id(c.objects[i]);
            }
            if c.occupants[i].is_none() || id(c.occupants[i]).is_some() {
                cell.entities[0] = id(c.occupants[i]);
            }
            cell.bytes[0] = c.flags[i];
            cell.bytes[1] = c.types[i].0;
            cell.bytes[2] = c.road_kind[i];
            cell.bytes[3] = c.road_built[i];
            cell.bytes[4] = c.growth_bytes[i];
            cell.bytes[5] = c.floors.counts[i];
            cell.flags.copy_from_slice(&c.layers[i][..9]);
            cell.value = u16::from_le_bytes([c.layers[i][10], c.layers[i][11]]);
        }
        g.map.trailer.copy_from_slice(&c.layer_totals[..9]);
    }
}

#[cfg(test)]
mod tests {
    use crate::start::load::tests_support::{archive, root};
    use banished_formats::savegame::{Fields, GameState, Value};

    /// The differences between two game states: objects' fields (type, path, ours, theirs),
    /// map cells and schedules.
    pub(crate) fn diff(a: &GameState, b: &GameState) -> Vec<String> {
        fn walk(prefix: &str, a: &Fields, b: &Fields, out: &mut Vec<String>) {
            for ((n, va), (_, vb)) in a.0.iter().zip(&b.0) {
                let path = format!("{prefix}.{n}");
                match (va, vb) {
                    (Value::Block(_, fa), Value::Block(_, fb)) => walk(&path, fa, fb, out),
                    (Value::List(la), Value::List(lb)) => {
                        if la.len() != lb.len() {
                            out.push(format!("{path}: {} items, saved {}", la.len(), lb.len()));
                        }
                        for (k, (fa, fb)) in la.iter().zip(lb).enumerate() {
                            walk(&format!("{path}[{k}]"), fa, fb, out);
                        }
                    }
                    _ if va != vb => out.push(format!("{path}: ours {va:?} saved {vb:?}")),
                    _ => {}
                }
            }
        }
        let mut out = Vec::new();
        if a.entity_time != b.entity_time {
            out.push(format!(
                "entity clock: ours {} saved {}",
                a.entity_time, b.entity_time
            ));
        }
        walk("town", &a.town, &b.town, &mut out);
        walk("managers", &a.managers, &b.managers, &mut out);
        for (id, oa) in &a.objects {
            match b.objects.get(id) {
                Some(ob) => walk(
                    &format!("{} {id}", oa.type_name),
                    &oa.fields,
                    &ob.fields,
                    &mut out,
                ),
                None => out.push(format!("{} {id}: not in the save", oa.type_name)),
            }
        }
        for (i, (ca, cb)) in a.map.cells.iter().zip(&b.map.cells).enumerate() {
            if ca != cb {
                out.push(format!("map cell {i}: ours {ca:?} saved {cb:?}"));
            }
        }
        for (k, (sa, sb)) in a.schedules.iter().zip(&b.schedules).enumerate() {
            if sa != sb {
                out.push(format!("schedule {k}: ours {sa:?} saved {sb:?}"));
            }
        }
        out
    }

    /// A save's state as the original holds it after loading: one more floor under each active
    /// FloorComponent (its load slot adds them again, `FUN_140124a10`).
    fn floors_added(saved: &crate::saves::saved::SavedGame) -> GameState {
        let mut expected = saved.state.clone();
        let n = expected.map.size;
        for (_, e) in saved.entities() {
            let active = saved
                .component(e, "FloorComponent")
                .is_some_and(|f| matches!(f.fields.at("floor.active"), Some(Value::Bool(true))));
            let Some(map) = saved.component(e, "MapComponent").filter(|_| active) else {
                continue;
            };
            let m = |k: &str| map.fields.i32(&format!("map.{k}")).unwrap_or(0);
            for y in m("y")..m("y") + m("h") {
                for x in m("x")..m("x") + m("w") {
                    expected.map.cells[(y * n + x) as usize].bytes[5] += 1;
                }
            }
        }
        expected
    }

    /// Loading a vanilla save and writing it straight back gives the same bytes: every field
    /// the load reads is written back by the same mapping.
    #[test]
    fn vanilla_saves_export_unchanged() {
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
            let saved = crate::saves::saved::SavedGame::parse(&bytes).unwrap();
            let town = crate::start::StartingTown::from_save(&archive, &saved).unwrap();
            let mut g = saved.state.clone();
            town.export_into(&archive, &mut g);
            let ours = g.encode().unwrap();
            // What the original writes right after a load: the saved state with one more floor
            // under each active FloorComponent (its load slot adds them again, `FUN_140124a10`).
            let expected = floors_added(&saved);
            let original = expected.encode().unwrap();
            if ours != original {
                for line in diff(&g, &expected).iter().take(12) {
                    eprintln!("{name}: {line}");
                }
            }
            if let Some(at) = ours.iter().zip(&original).position(|(a, b)| a != b) {
                panic!(
                    "{name}: written state differs at {at:#x} (lengths {} / {})",
                    ours.len(),
                    original.len()
                );
            }
            assert_eq!(ours.len(), original.len(), "{name}");
            eprintln!("{name}: exported unchanged");
        }
    }

    /// A town run for a minute after loading, saved, and loaded again writes the same state:
    /// everything the simulation changed is written in a form the load reads back.
    #[test]
    fn a_simulated_town_reloads_as_saved() {
        let Some(archive) = archive() else {
            eprintln!("game data missing; skipping");
            return;
        };
        let Ok(bytes) = std::fs::read(root().join("save-game").join("Save83.sav")) else {
            return;
        };
        let saved = crate::saves::saved::SavedGame::parse(&bytes).unwrap();
        let world = crate::view::world::World::from_save(&archive, &saved).unwrap();
        let grid = banished_game::terrain::HeightGrid {
            width: world.terrain.width(),
            heights: &world.terrain.heights,
        };
        let mut town = crate::start::StartingTown::from_save(&archive, &saved).unwrap();
        let mut rng = banished_game::random::Random::new(83);
        let dt = 1.0 / 30.0;
        for _ in 0..30 * 60 {
            town.apply_commands(&grid, &mut rng);
            town.iterate(&grid, &mut rng, dt, 50.0);
            let warnings = banished_game::events::StockWarnings::default();
            let stock = std::array::from_fn(|i| (warnings.count[i], warnings.limits[i]));
            town.update_citizens(&grid, &mut rng, dt, 50.0, stock);
            town.construction
                .jobs
                .step(banished_game::scheduler::ticks(dt));
        }
        let mut first = saved.state.clone();
        town.export_into(&archive, &mut first);
        // The minute changed the town: positions, needs, stacks, works.
        let changed = diff(&first, &floors_added(&saved));
        eprintln!(
            "{} fields changed by simulating; {} objects ({} saved)",
            changed.len(),
            first.objects.len(),
            saved.state.objects.len()
        );
        assert!(changed.len() > 100, "the simulation changed little");
        let written = first.encode().unwrap();
        let reloaded = crate::saves::saved::SavedGame {
            state: GameState::decode(&written).unwrap(),
            metadata: saved.metadata.clone(),
        };
        let again = crate::start::StartingTown::from_save(&archive, &reloaded).unwrap();
        assert_eq!(
            again.citizens.len(),
            town.citizens.iter().filter(|c| c.alive).count()
        );
        assert_eq!(again.wildlife.animals.len(), town.wildlife.animals.len());
        let mut second = reloaded.state.clone();
        again.export_into(&archive, &mut second);
        let lines = diff(&second, &floors_added(&reloaded));
        for line in lines.iter().take(20) {
            eprintln!("{line}");
        }
        assert!(
            lines.is_empty(),
            "{} differences after reloading",
            lines.len()
        );
    }

    /// A new game (seed 1234, Medium, run for ten seconds) written into an empty state loads
    /// back and writes the same state again: every entity is made by the writers and reads
    /// back as it was.
    #[test]
    fn a_new_game_saves_and_reloads() {
        let Some(archive) = archive() else {
            eprintln!("game data missing; skipping");
            return;
        };
        let params = crate::view::world::WorldParams {
            seed: 1234,
            start_condition_ref: r"Game\StartCondition\Medium.rsc:resource".into(),
            ..Default::default()
        };
        let world = crate::view::world::World::generate(&archive, &params).unwrap();
        let grid = banished_game::terrain::HeightGrid {
            width: world.terrain.width(),
            heights: &world.terrain.heights,
        };
        let mut rng = banished_game::random::Random::new(params.seed);
        let mut town = crate::start::StartingTown::create(
            &archive,
            &params.start_condition_ref,
            world.terrain.size / 2,
            &grid,
            &mut rng,
        )
        .unwrap();
        let dt = 1.0 / 30.0;
        for _ in 0..30 * 10 {
            town.apply_commands(&grid, &mut rng);
            town.iterate(&grid, &mut rng, dt, 50.0);
            let warnings = banished_game::events::StockWarnings::default();
            let stock = std::array::from_fn(|i| (warnings.count[i], warnings.limits[i]));
            town.update_citizens(&grid, &mut rng, dt, 50.0, stock);
            town.construction
                .jobs
                .step(banished_game::scheduler::ticks(dt));
        }
        let size = world.terrain.size as i32;
        let map = banished_formats::menu_scene::MapComponent {
            version: 6,
            dims: [size, size / 2, 32, 33, size + 1, 33 * 33],
            heights: world.terrain.heights.clone(),
            terrain_material: Some(world.profile.materials[0].clone()),
            surface: world.terrain.surface.clone(),
            water_vertices: world.water.vertices.clone(),
            water_indices: world.water.indices.clone(),
            river_material: Some(world.profile.materials[1].clone()),
            lake_material: Some(world.profile.materials[2].clone()),
            river_ranges: world.water.rivers.clone(),
            lake_ranges: world.water.lakes.clone(),
            river_paths: world.river_paths.clone(),
            decals: Vec::new(),
            extra_decals: Vec::new(),
            ints: Vec::new(),
            decal_materials: Vec::new(),
        };
        let mut first = super::new_game::state(super::new_game::NewGame {
            params: &params,
            flags: 0,
            map,
            effects: 2,
            history: 0,
            stock: 0,
            capacity: 0,
        });
        town.export_into(&archive, &mut first);
        let written = first.encode().unwrap();
        let reloaded = crate::saves::saved::SavedGame {
            state: GameState::decode(&written).unwrap(),
            metadata: banished_formats::save::SaveMetadata {
                name: String::new(),
                width: 0,
                height: 0,
                image: Vec::new(),
                value: 2,
                mods: Vec::new(),
                wide: false,
            },
        };
        let again = crate::start::StartingTown::from_save(&archive, &reloaded).unwrap();
        let alive = |t: &crate::start::StartingTown| t.placed.iter().filter(|p| p.alive).count();
        assert_eq!(alive(&again), alive(&town));
        assert_eq!(again.citizens.len(), town.citizens.len());
        assert_eq!(again.wildlife.animals.len(), town.wildlife.animals.len());
        let mut second = reloaded.state.clone();
        again.export_into(&archive, &mut second);
        let lines = diff(&second, &floors_added(&reloaded));
        for line in lines.iter().take(20) {
            eprintln!("{line}");
        }
        assert!(
            lines.is_empty(),
            "{} differences after reloading",
            lines.len()
        );
    }
}
