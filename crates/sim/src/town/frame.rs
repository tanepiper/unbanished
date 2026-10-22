//! The town's frame update: running the simulation, applying commands, rebuilds and storage changes.

use super::*;

impl StartingTown {
    /// Pile templates' raw materials in the town's list (`FUN_140097330`, which the original
    /// runs when a pile is set up; resolved here when its template loads).
    pub(super) fn resolve_piles(&mut self, archive: &Archive) {
        for i in 0..self.templates.len() {
            let t = &self.templates[i];
            if t.pile_material.is_some() || t.pile_yield == (0, 0) {
                continue;
            }
            let reference = t.reference.clone();
            self.templates[i].pile_material =
                material_index(archive, &mut self.materials, &reference);
        }
    }

    /// Run placed entities on their template's scheduled bucket.
    ///
    /// Original: `FUN_1400c9560`.
    pub fn iterate(
        &mut self,
        terrain: &HeightGrid,
        rng: &mut Random,
        dt: f32,
        temperature: f32,
    ) -> bool {
        // `AnimationManager::Update` (`FUN_14009b850`): the shared animation instances, before
        // any character.
        unbanished_game::behaviour::animation::shared::update(unbanished_game::scheduler::ticks(
            dt,
        ));
        self.agents.sounds.learn(&self.templates);
        // The wild animals' CharacterComponent (the bit-0x04 phase).
        self.agents.move_animals(
            &mut self.wildlife,
            &self.cells,
            terrain,
            rng,
            &self.materials,
            dt,
        );
        let wildlife = &mut self.wildlife;
        let fruits = &mut self.construction.fruits;
        let tombstones = &mut self.construction.tombstones;
        let calendar = self.construction.calendar;
        let changed = Seeder {
            templates: &self.templates,
            terrain,
            cells: &mut self.cells,
            rng,
            placed: &mut self.placed,
            removed: &mut self.removed,
            animation_events: &mut self.animation_events,
        }
        .iterate_with(
            &mut self.scheduler,
            dt,
            temperature,
            false,
            &mut |seeder, scheduler, id, elapsed| {
                // TombstoneComponent (`FUN_140058130`).
                if tombstones.contains_key(&id) {
                    return unbanished_game::people::cemetery::tombstone_visit(
                        tombstones,
                        seeder.cells,
                        seeder.placed,
                        id,
                        elapsed,
                    );
                }
                // FruitComponent (`FUN_14004a740`) after the tree's growth.
                let p = &seeder.placed[id];
                if let Some(f) = seeder.templates[p.template].fruit {
                    let fruit = fruits.entry(id).or_default();
                    unbanished_game::production::orchard::ripen(
                        fruit,
                        &f,
                        p.growth,
                        elapsed,
                        calendar,
                        temperature,
                    );
                    return false;
                }
                let mut wild = unbanished_game::nature::wildlife::Wild {
                    wildlife: &mut *wildlife,
                    placed: seeder.placed,
                    templates: seeder.templates,
                    cells: seeder.cells,
                    terrain: seeder.terrain,
                    rng: seeder.rng,
                    scheduler,
                };
                wild.scheduled(id, elapsed);
                false
            },
        );
        // The entity manager deletes the queued animals.
        let buried = unbanished_game::nature::wildlife::Wild {
            wildlife: &mut self.wildlife,
            placed: &mut self.placed,
            templates: &self.templates,
            cells: &self.cells,
            terrain,
            rng,
            scheduler: &mut self.scheduler,
        }
        .bury();
        self.visits = self.scheduler.take_visits();
        changed | buried
    }

    /// Player commands and newly placed sites (before an iteration).
    pub fn apply_commands(&mut self, terrain: &HeightGrid, rng: &mut Random) {
        let groups: Vec<_> = self
            .citizens
            .iter()
            .map(|c| {
                self.citizen_anims
                    .get(usize::from(!c.male))
                    .and_then(Option::as_deref)
                    .filter(|_| c.alive)
            })
            .collect();
        let mut env = unbanished_game::town::Env {
            wildlife: &mut self.wildlife,
            cells: &mut self.cells,
            placed: &mut self.placed,
            templates: &self.templates,
            terrain,
            materials: &self.materials,
            storages: &mut self.storages,
            sites: &self.storage_sites,
            rng,
            construction: &mut self.construction,
            stock: [(0, 0); 21],
            temperature: 0.0,
            visits: &[],
        };
        let rules = self.rules;
        let names = self.names.as_ref();
        // Citizens made from nomads take a name, appearance and lifespan as newborns do.
        let mut newborn = |rng: &mut Random, model: &Citizen, male: bool| match names {
            Some((m, f)) => {
                unbanished_game::people::citizens::newborn(rng, &rules, m, f, model, male)
            }
            None => Citizen {
                male,
                alive: true,
                ..model.clone()
            },
        };
        self.agents
            .apply_commands(&mut self.citizens, &mut env, &groups, &mut newborn);
        self.apply_storage_changes();
        // Remove Roads' markers on built road cells (`FUN_140108ab0`).
        for (cell, marker) in std::mem::take(&mut self.construction.road_markers) {
            let n = self.cells.size as i32;
            self.add_cached_marker(marker, (cell % n, cell / n), terrain);
        }
    }

    /// The ground under each StorageDisplay's cells (`FUN_140057090`'s ground points), for
    /// the entities that have none yet.
    pub(super) fn place_displays(&mut self, terrain: &HeightGrid) {
        for (e, p) in self.placed.iter().enumerate() {
            if !p.alive
                || !self.templates[p.template].storage_display
                || self.display_cells.contains_key(&e)
            {
                continue;
            }
            let n = self.cells.size as i32;
            let (w, h) = p.footprint;
            let cells = (0..w * h)
                .map(|i| {
                    let cell = (p.cell.1 + i / w) * n + p.cell.0 + i % w;
                    unbanished_game::town::floor_ground(
                        &self.cells.floors,
                        self.cells.size,
                        terrain,
                        cell,
                    )
                })
                .collect();
            self.display_cells.insert(e, cells);
        }
    }

    /// What DestroyComponents put in torn-down buildings' places (`FUN_1401215f0`): the
    /// template over the same map rectangle, turned and placed as the old model, its zone
    /// reserving the footprint; a site is activated before the next iteration, anything else
    /// stamps the map now.
    pub(super) fn apply_rebuilds(&mut self) {
        for r in std::mem::take(&mut self.construction.rebuilds) {
            let id = self.placed.len();
            let t = &self.templates[r.template];
            let (bucket, reference) = (t.bucket, t.reference.clone());
            let footprint = t
                .map
                .as_ref()
                .map(|m| m.footprint.clone())
                .unwrap_or_default();
            let cell_type = t.map.as_ref().map_or(0, |m| m.cell_type as u8);
            let stamps = t.map.as_ref().is_some_and(|m| !m.no_stamp);
            let mesh = t.model.as_ref().map(|m| m.mesh.max(0) as usize);
            self.placed.push(Placed {
                template: r.template,
                bucket,
                cell: r.cell,
                footprint: r.footprint,
                position: r.position,
                yaw: r.yaw,
                variant: r.variant,
                mesh: usize::MAX,
                growth: 0.0,
                growth_state: None,
                spawn_state: None,
                alive: true,
                removal_requested: false,
                animation: None,
                scale: [1.0; 4],
            });
            self.register_entity(id);
            let rules = unbanished_game::world::placement::PlacementRules {
                mask: 0,
                masks: Vec::new(),
                mask_chars: String::new(),
                footprint,
                width: 0,
                height: 0,
                cell_type,
            };
            let rect = unbanished_game::world::placement::Rect {
                x0: r.cell.0,
                y0: r.cell.1,
                x1: r.cell.0 + r.footprint.0 - 1,
                y1: r.cell.1 + r.footprint.1 - 1,
            };
            let degrees = r.yaw as i32;
            unbanished_game::world::placement::zone(&mut self.cells, &rules, rect, degrees, id);
            if let Some(st) = self
                .construction
                .templates
                .iter()
                .position(|t| t.reference == reference)
            {
                self.add_site_from(st, id);
            } else {
                // A plain object: its model's mesh, its MapComponent stamped.
                if let Some(mesh) = mesh {
                    self.placed[id].mesh = mesh;
                }
                if stamps {
                    unbanished_game::world::placement::stamp(
                        &mut self.cells,
                        &rules,
                        rect,
                        degrees,
                        id,
                    );
                }
            }
            self.construction.models_dirty = true;
        }
    }

    /// StorageLocationComponent's state 2 and 3 enters (`FUN_140132700` / `FUN_1401326c0`):
    /// a site's storage joins the town's (appended) or leaves it.
    pub(super) fn apply_storage_changes(&mut self) {
        for (e, join) in std::mem::take(&mut self.construction.storage_changes) {
            if !join {
                self.storage_sites.retain(|s| s.entity != e);
                continue;
            }
            let Some(site) = self.construction.sites.get(&e) else {
                continue;
            };
            if self.storage_sites.iter().any(|s| s.entity == e) {
                continue;
            }
            let t = &self.templates[self.placed[e].template];
            self.storage_sites.push(StorageSite {
                entity: e,
                storage: site.storage,
                location_flags: t.storage_location_flags,
                // A MarketLocationComponent's market (`+0x10`), searched within its radius.
                radius: if t.market_location {
                    t.market_radius
                } else {
                    None
                },
            });
        }
    }

    // Game +0xf0 +0x00/+0x10
    /// The registered town storages (`Game`), the ones the stock counts.
    pub fn registered_storages(&self) -> Vec<Storage> {
        self.storage_sites
            .iter()
            .map(|s| self.storages[s.storage].clone())
            .collect()
    }

    /// Adult counts per profession and each profession's workplace places.
    ///
    /// Original: `FUN_14007ba70`.
    pub fn profession_counts(&self) -> (Vec<i32>, Vec<i32>) {
        let c = &self.construction;
        let mut counts = vec![0; c.professions.list.len().max(1)];
        for (citizen, state) in self.citizens.iter().zip(&self.agents.states) {
            if citizen.alive && citizen.is_adult(&self.rules) && citizen.school.is_none() {
                if let Some(n) = counts.get_mut(state.profession) {
                    *n += 1;
                }
            }
        }
        let capacity = (0..counts.len()).map(|p| c.workforce.capacity(p)).collect();
        (counts, capacity)
    }
}
