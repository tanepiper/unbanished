//! Restoring the character tree, the water, the schedule and the map cells from a save.

use super::*;

/// The gameplay map's cells as `FUN_14001b570` wrote them (the 48-byte cell layout is in
/// `unbanished_game::nature::seeding::Cells`).
pub(super) fn restore_cells(
    archive: &Archive,
    saved: &SavedGame,
    n: usize,
    index: &BTreeMap<ObjId, usize>,
    templates: &mut Vec<unbanished_game::nature::seeding::Template>,
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

impl StartingTown {
    /// The town's water sources as saved (`+0xe0`, `+0x130`, `+0x140`): the wells by their
    /// WellComponents, filed in the slots they were saved in. A save before version 5 has no
    /// blocks; the points and blocks are then the scan's, with the wells filed again.
    /// The characters' tree (`Game +0x88`) as the load fills it: each CharacterComponent
    /// put in as it is read (`FUN_14010ef80` → `FUN_140018e00`), in the objects' order.
    pub(super) fn restore_character_tree(
        &mut self,
        saved: &SavedGame,
        index: &BTreeMap<ObjId, usize>,
        order: &BTreeMap<ObjId, usize>,
    ) {
        use unbanished_game::behaviour::character_tree::Who;
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
        fn who_alive(who: Who, citizens: &[unbanished_game::people::citizens::Citizen]) -> bool {
            match who {
                Who::Citizen(c) => citizens.get(c).is_some_and(|c| c.alive),
                _ => true,
            }
        }
    }

    pub(super) fn restore_water(&mut self, saved: &SavedGame, index: &BTreeMap<ObjId, usize>) {
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
    pub(super) fn restore_schedule(&mut self, saved: &SavedGame, ids: &mut super::super::SaveIds) {
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
