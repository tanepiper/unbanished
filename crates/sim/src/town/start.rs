//! A New Game's town: the start points, the start structures and the households.

use super::*;

impl StartingTown {
    /// [`StartingTown::create`] with `players` towns on the map: player 0's start is the
    /// original's search from the centre, each next one [`Map::find_start_away`] from the ones
    /// taken, and every player gets the profile's structures, a household and its own starting
    /// goods there. A separate towns game's New Game (step 7 of docs/MULTIPLAYER-PLAN.md); one
    /// player is the original's game, drawing the same random numbers in the same order.
    ///
    /// Original: `FUN_1400a87d0`.
    pub fn create_for(
        archive: &Archive,
        profile_ref: &str,
        map_size: usize,
        terrain: &HeightGrid,
        rng: &mut Random,
        players: u8,
    ) -> Option<StartingTown> {
        let players = usize::from(players.max(1));
        // A new game: no shared animation instances (`AnimationManager`).
        unbanished_game::behaviour::animation::shared::reset();
        let profile: MapGenerator = map_generator(&read(archive, profile_ref)?)
            .map_err(|e| log::warn!("{profile_ref}: {e}"))
            .ok()?;
        let map = Map::new(map_size, terrain);
        let radius = profile.start_radius;
        // One start per player: the original's for the first, a far one for each next
        // (inferred: four start radii apart, as close as the first search allows).
        let mut starts = Vec::with_capacity(players);
        for _ in 0..players {
            let start = match starts.first() {
                None => map.find_start(radius),
                Some(_) => map.find_start_away(radius, &starts, (radius * 4) as f32),
            };
            starts.push(start);
        }
        let start = starts[0];
        let mut templates = Vec::new();
        let structures: Vec<_> = profile
            .structures
            .iter()
            .map(|s| {
                (
                    s.clone(),
                    load_template(archive, &s.template, &mut templates),
                )
            })
            .collect();
        let list: Vec<usize> = profile
            .natural_resources
            .iter()
            .filter_map(|r| load_template(archive, r, &mut templates))
            .collect();
        let mut cells = Cells::new(map_size, map.types);
        // The map's setup files the shore cells as water sources (`FUN_1400a43f0`).
        let water = unbanished_game::disasters::fire::Water::new(&cells);
        let mut placed = Vec::new();
        let mut removed = Vec::new();
        let mut animation_events = Vec::new();
        let mut seeder = Seeder {
            templates: &templates,
            terrain,
            cells: &mut cells,
            rng,
            placed: &mut placed,
            removed: &mut removed,
            animation_events: &mut animation_events,
        };
        for (player, &(sx, sy)) in starts.iter().enumerate() {
            seeder.place_structures(&structures, sx, sy, player as u8);
        }
        let structure_count = seeder.placed.len();
        for (e, p) in seeder.placed.iter().enumerate() {
            assemble::stamp_structure(seeder.cells, &templates, p, e);
        }
        for &(sx, sy) in &starts {
            seeder.seed(&list, sx, sy, radius * radius);
        }
        let mut scheduler = EntityScheduler::default();
        for (id, p) in placed.iter().enumerate() {
            scheduler.register(id, p.bucket);
        }
        // `FUN_1400a5eb0`: the wildlife populations (deer) and their herds.
        let mut wildlife = unbanished_game::nature::wildlife::Wildlife::default();
        // The households' CharacterComponents activate as they are made, before any animal
        // (`FUN_14010ee90`): first in the characters' tree.
        let radius = read(
            archive,
            &normalise(&profile.citizen).replace(":resource", ":character"),
        )
        .and_then(|d| unbanished_formats::game::character_description(&d).ok())
        .unwrap_or_default()
        .radius;
        // `FUN_1400a62a0`: each player's starting households around that player's start.
        let mut citizens = Vec::new();
        let mut rules = None;
        for (player, &s) in starts.iter().enumerate() {
            let before = citizens.len();
            let (made, made_rules) = households(
                archive,
                &profile,
                &grid_start(map_size, terrain, s),
                rng,
                player as u8,
            );
            rules.get_or_insert(made_rules);
            citizens.extend(made);
            for (i, c) in citizens.iter().enumerate().skip(before) {
                wildlife.tree.insert(
                    unbanished_game::behaviour::character_tree::Who::Citizen(i),
                    c.position,
                    radius,
                );
            }
        }
        // `starts` is never empty, so the first household made the rules.
        let rules = rules.unwrap_or(CitizenRules {
            adult_age: 2.0,
            start_age: (2.2, 8.0),
            lifespan: (15.0, 3.0),
            education: 0,
        });
        let populations =
            wildlife_kinds(archive, &profile.populations, &mut templates, &mut wildlife);
        for t in populations {
            unbanished_game::nature::wildlife::Wild {
                wildlife: &mut wildlife,
                placed: &mut placed,
                templates: &templates,
                cells: &cells,
                terrain,
                rng,
                scheduler: &mut scheduler,
            }
            .start_population(t);
        }
        // Each player's starting goods go in that player's own storages.
        let mut materials = Vec::new();
        let mut storages = Vec::new();
        let mut storage_sites = Vec::new();
        for player in 0..players {
            let (mats, made, mut sites) =
                starting_goods(archive, &profile, &placed, &templates, player as u8);
            if player == 0 {
                materials = mats;
            }
            let offset = storages.len();
            for s in &mut sites {
                s.storage += offset;
            }
            storages.extend(made);
            storage_sites.extend(sites);
        }
        // `FUN_140097330`: the profile's raw materials are the first known.
        let known_materials: Vec<usize> = (0..materials.len()).collect();
        // `FUN_1400a87d0`'s end: random crops, orchard trees and livestock become known
        // (`FUN_140096ef0`, once each).
        // Each player's town knows its own random picks; a template both pick is one entry.
        let mut known = Vec::new();
        for _ in 0..players {
            for (list, picks) in [
                (&profile.crops, profile.crop_picks),
                (&profile.orchard_trees, profile.tree_picks),
            ] {
                for _ in 0..picks {
                    let r = rng.rand_below(list.len() as u32) as usize;
                    if let Some(t) = list
                        .get(r)
                        .and_then(|t| load_template(archive, t, &mut templates))
                    {
                        if !known.contains(&t) {
                            known.push(t);
                        }
                    }
                }
            }
        }
        // Every livestock kind can be kept; the picks bring their starting animals
        // (`FUN_1400a5fe0`) and become known.
        for t in &profile.livestock {
            livestock_kind(archive, t, &mut templates, &mut wildlife);
        }
        let [dx, dy, spread] = profile.livestock_floats;
        for &s in &starts {
            let at = grid_start(map_size, terrain, s);
            for _ in 0..profile.livestock_picks {
                let r = rng.rand_below(profile.livestock.len() as u32) as usize;
                if let Some(t) = profile
                    .livestock
                    .get(r)
                    .and_then(|t| livestock_kind(archive, t, &mut templates, &mut wildlife))
                {
                    unbanished_game::nature::wildlife::Wild {
                        wildlife: &mut wildlife,
                        placed: &mut placed,
                        templates: &templates,
                        cells: &cells,
                        terrain,
                        rng,
                        scheduler: &mut scheduler,
                    }
                    .start_livestock(
                        t,
                        profile.ints[0],
                        [at[0] + dx, at[1] + dy, at[2]],
                        spread,
                    );
                    if !known.contains(&t) {
                        known.push(t);
                    }
                }
            }
        }
        log::info!(
            "starts {starts:?}: {} natural resources placed",
            placed.len()
        );
        let mut town = Self::assemble(
            archive,
            &profile.citizen,
            TownParts {
                start,
                citizens,
                rules,
                materials,
                storages,
                storage_sites,
                templates,
                placed,
                cells,
                scheduler,
                removed,
                animation_events,
                wildlife,
                known,
                known_materials,
                water,
            },
        );
        town.build_structures(archive, structure_count);
        town.place_displays(terrain);
        town.starts = starts;
        Some(town)
    }
}
