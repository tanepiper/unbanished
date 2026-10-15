//! The building placement tool (`PlacementTool`, vtable `0x140168130`), started by toolbar kind 1
//! (`FUN_1400be770` → `FUN_140108030`). See docs/BUILD-TOOLS.md.
//!
//! - start (`+0x20`, `FUN_140103a50`): read the template's createplaced, map, model and decal
//!   descriptions and build a ghost mesh instance: `Rand(variant count)` from the game stream,
//!   the variant's last (finished) mesh;
//! - input (`+0x30`, `FUN_140103770`): StructureLeft/Right turn the shared RotationSetting,
//!   StructureType cycles the variant (`FUN_140102e30`), MouseLeft places (`FUN_140102b20`);
//! - hover (`+0x38`, `FUN_1401033b0`): footprint under the cursor, ghost transform, validity.

use banished_formats::game::{
    build_description, create_placed_description, map_description, model_description,
    toolbar_description, BuildDescription, ModelDescription, ToolbarDescription,
};
use banished_formats::pkg::Archive;
use banished_formats::types::graphics_mesh;
use banished_game::placement::{can_place, footprint, PlacementRules, Rect, ROTATION_STEP};
use banished_game::random::Random;
use banished_game::seeding::Placed;
use banished_game::terrain::HeightGrid;

use crate::models::placement_matrix;
use crate::start::StartingTown;

fn read(archive: &Archive, reference: &str) -> Option<Vec<u8>> {
    archive.read_ref(&reference.replace('\\', "/")).ok()
}

pub struct PlacementTool {
    /// `Template\X.rsc:resource`.
    pub reference: String,
    template: usize,
    rules: PlacementRules,
    model: ModelDescription,
    /// `+0x64`: ghost variant.
    variant: usize,
    /// Current footprint, ghost position and validity.
    rect: Option<Rect>,
    position: [f32; 3],
    valid: bool,
    pub toolbar: Option<ToolbarDescription>,
    pub build: Option<BuildDescription>,
    /// Bounds of each variant's ghost mesh (for the CreateBar position).
    bounds: Vec<Option<([f32; 3], [f32; 3])>>,
}

impl PlacementTool {
    /// `FUN_140103a50`.
    pub fn start(
        archive: &Archive,
        town: &mut StartingTown,
        reference: &str,
        rng: &mut Random,
    ) -> Option<PlacementTool> {
        let file = reference.split(':').next()?.replace('\\', "/");
        let placed = create_placed_description(&read(archive, &format!("{file}:createplaced"))?)
            .map_err(|e| log::warn!("{reference}: {e}"))
            .ok()?;
        let map = map_description(&read(archive, &format!("{file}:map"))?).ok()?;
        let model = model_description(&read(archive, &format!("{file}:model"))?).ok()?;
        let template = town.load_template(archive, reference)?;
        let variant = rng.rand_below(model.variants.len() as u32) as usize;
        let toolbar =
            read(archive, &format!("{file}:toolbar")).and_then(|d| toolbar_description(&d).ok());
        let build =
            read(archive, &format!("{file}:build")).and_then(|d| build_description(&d).ok());
        let bounds = model
            .variants
            .iter()
            .map(|v| {
                let mesh = v.meshes.last()?.as_deref()?;
                let file = mesh.split(':').next()?.replace('\\', "/");
                let m = graphics_mesh(&archive.read_ref(&format!("{file}:resource")).ok()?).ok()?;
                Some((m.bounds_min, m.bounds_max))
            })
            .collect();
        Some(PlacementTool {
            reference: reference.to_string(),
            template,
            rules: PlacementRules {
                mask: placed.flags,
                masks: placed.values.clone(),
                mask_chars: placed.name.clone(),
                footprint: map.footprint.clone(),
                width: placed.width,
                height: placed.height,
                cell_type: map.cell_type as u8,
            },
            model,
            variant,
            rect: None,
            position: [0.0; 3],
            valid: false,
            toolbar,
            build,
            bounds,
        })
    }

    /// The finished mesh of the current variant (`variants[v].meshes[count − 1]`).
    fn ghost_mesh(&self) -> Option<String> {
        self.model
            .variants
            .get(self.variant)?
            .meshes
            .last()?
            .clone()
    }

    /// `FUN_140102e30`.
    pub fn next_variant(&mut self) {
        self.variant += 1;
        if self.variant >= self.model.variants.len() {
            self.variant = 0;
        }
    }

    /// `FUN_1401033b0` / `FUN_140102ed0`: follow the cursor cell.
    pub fn hover(
        &mut self,
        town: &StartingTown,
        terrain: &HeightGrid,
        cell: Option<(i32, i32)>,
        rotation: i32,
    ) {
        let n = town.cells.size as i32;
        self.rect =
            cell.and_then(|c| footprint(n, c, self.rules.width, self.rules.height, rotation));
        let Some(rect) = self.rect else {
            self.valid = false;
            return;
        };
        // Midpoint of the ground at the two corner cells (cell positions without the half-cell
        // offset); the original then forces z to 0.
        let corner = |x: i32, y: i32| {
            let c = (n >> 1) as f32 - 0.5;
            let (px, py) = (x as f32 - c, y as f32 - c);
            let mut normal = [0.0; 3];
            [px, py, terrain.ground(px, py, &mut normal)]
        };
        let (a, b) = (corner(rect.x0, rect.y0), corner(rect.x1, rect.y1));
        self.position = [(b[0] + a[0]) * 0.5, (b[1] + a[1]) * 0.5, 0.0];
        let owner_ok = |_occupant: usize| false;
        self.valid = can_place(
            &town.cells,
            terrain,
            &self.rules,
            rect,
            rotation * ROTATION_STEP as i32,
            owner_ok,
        );
    }

    /// For scripts and tests: the first valid footprint without objects on a square spiral
    /// from `near` (rings 6 to 40), left hovered.
    pub fn find_clear_spot(
        &mut self,
        town: &StartingTown,
        terrain: &HeightGrid,
        near: (i32, i32),
    ) -> Option<(i32, i32)> {
        self.find_spot(town, terrain, near, false)
    }

    /// A valid spot whose footprint is clear, or (`resources`) holds natural resources.
    pub fn find_spot(
        &mut self,
        town: &StartingTown,
        terrain: &HeightGrid,
        near: (i32, i32),
        resources: bool,
    ) -> Option<(i32, i32)> {
        let n = town.cells.size as i32;
        for r in 6i32..40 {
            for dy in -r..=r {
                for dx in -r..=r {
                    if dx.abs() != r && dy.abs() != r {
                        continue;
                    }
                    let at = (near.0 + dx, near.1 + dy);
                    self.hover(town, terrain, Some(at), 0);
                    let Some(rect) = self.rect.filter(|_| self.valid) else {
                        continue;
                    };
                    let objects = (rect.y0..=rect.y1)
                        .flat_map(|y| (rect.x0..=rect.x1).map(move |x| (y * n + x) as usize))
                        .filter(|&i| town.cells.objects[i].is_some())
                        .count();
                    if (objects == 0) != resources {
                        return Some(at);
                    }
                }
            }
        }
        None
    }

    pub fn valid(&self) -> bool {
        self.valid
    }

    /// Footprint size in cells.
    pub fn size(&self) -> Option<(i32, i32)> {
        self.rect.map(|r| (r.width(), r.height()))
    }

    /// The ghost's corners in world space (`FUN_140104080`).
    pub fn ghost_corners(&self, rotation: i32) -> Option<Vec<glam::Vec3>> {
        let (_, matrix) = self.ghost(rotation)?;
        let (lo, hi) = self.bounds.get(self.variant).copied().flatten()?;
        let m = glam::Mat4::from_cols_array(&matrix);
        Some(
            (0..8)
                .map(|i| {
                    let c = glam::Vec3::new(
                        if i & 1 == 0 { lo[0] } else { hi[0] },
                        if i & 2 == 0 { lo[1] } else { hi[1] },
                        if i & 4 == 0 { lo[2] } else { hi[2] },
                    );
                    m.transform_point3(c)
                })
                .collect(),
        )
    }

    /// Ghost model instance for the renderer.
    pub fn ghost(&self, rotation: i32) -> Option<(String, [f32; 16])> {
        self.rect?;
        let yaw = rotation as f32 * ROTATION_STEP;
        Some((
            self.ghost_mesh()?,
            placement_matrix(self.position, [1.0; 3], yaw),
        ))
    }

    /// `FUN_140102b20`: create the building if the footprint is valid. It has no mesh until
    /// its BuildComponent starts building (`FUN_14010cfb0` shows mesh 0, the first stage). Its
    /// ZoneComponent reserves the footprint now (`FUN_1400a2830`); the map is stamped when the
    /// building is finished. Returns the new entity.
    pub fn place(
        &mut self,
        archive: &Archive,
        town: &mut StartingTown,
        rotation: i32,
    ) -> Option<usize> {
        let rect = self.rect.filter(|_| self.valid)?;
        let degrees = (rotation * ROTATION_STEP as i32).rem_euclid(360);
        let id = town.placed.len();
        town.placed.push(Placed {
            template: self.template,
            bucket: town.templates[self.template].bucket,
            cell: (rect.x0, rect.y0),
            footprint: (rect.width(), rect.height()),
            position: self.position,
            yaw: degrees as f32,
            variant: self.variant,
            mesh: usize::MAX,
            growth: 0.0,
            growth_state: None,
            spawn_state: None,
            alive: true,
            removal_requested: false,
            animation: None,
            scale: [1.0; 4],
        });
        // `FUN_1400c9d70`: register with the entity manager's buckets.
        town.register_entity(id);
        banished_game::placement::zone(&mut town.cells, &self.rules, rect, degrees, id);
        town.add_site(archive, id);
        self.next_variant();
        Some(id)
    }
}

#[cfg(test)]
mod construction_tests {
    use super::*;
    use banished_formats::pkg::Package;
    use banished_game::stock::stock_table;

    /// A wood house placed on clear ground near the start is supplied by laborers and built
    /// by two builders: the town's logs and stone drop by 16 and 8, the final mesh shows and
    /// the map is stamped.
    #[test]
    fn wood_house_gets_built() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../game-data/original/WinData");
        let mut archive = Archive::default();
        for name in ["data1.pkg", "patch1.pkg", "patch0.pkg", "data0.pkg"] {
            let Ok(bytes) = std::fs::read(dir.join(name)) else {
                eprintln!("game data missing; skipping construction integration");
                return;
            };
            archive.add(Package::parse(bytes).unwrap());
        }
        let world = crate::world::World::generate(
            &archive,
            &crate::world::WorldParams {
                seed: 1234,
                ..Default::default()
            },
        )
        .unwrap();
        let terrain = HeightGrid {
            width: world.terrain.width(),
            heights: &world.terrain.heights,
        };
        let mut rng = Random::new(1234);
        let mut town = StartingTown::create(
            &archive,
            "Game/StartCondition/Medium.rsc:resource",
            world.terrain.size,
            &terrain,
            &mut rng,
        )
        .unwrap();
        let mut tool = PlacementTool::start(
            &archive,
            &mut town,
            r"Template\WoodHouse.rsc:resource",
            &mut rng,
        )
        .unwrap();
        let n = town.cells.size as i32;
        let spot = tool.find_clear_spot(&town, &terrain, town.start);
        let spot = spot.expect("no clear spot for a house");
        tool.hover(&town, &terrain, Some(spot), 0);
        let before = stock_table(&town.registered_storages(), &town.materials);
        let id = tool.place(&archive, &mut town, 0).unwrap();
        let builder = town.construction.professions.builder;
        town.construction
            .commands
            .push(banished_game::professions::Command::SetCount {
                profession: builder,
                count: 2,
            });
        let mut warnings = banished_game::events::StockWarnings::default();
        let dt = 1.0 / 30.0;
        let mut done_at = None;
        for step in 0..30 * 60 * 8 {
            town.apply_commands(&terrain, &mut rng);
            town.iterate(&terrain, &mut rng, dt, 50.0);
            let stock = std::array::from_fn(|i| (warnings.count[i], warnings.limits[i]));
            town.update_citizens(&terrain, &mut rng, dt, 50.0, stock);
            warnings.recompute(&town.registered_storages(), &town.materials);
            town.construction
                .jobs
                .step(banished_game::scheduler::ticks(dt));
            if step == 1 {
                let (counts, capacity) = town.profession_counts();
                assert_eq!(counts[builder], 2);
                let points = town.templates[town.placed[id].template].build_points.len();
                assert!(points > 0);
                assert_eq!(capacity[builder], points as i32);
            }
            if town.construction.sites[&id].complete {
                done_at = Some(step);
                break;
            }
        }
        let site = &town.construction.sites[&id];
        eprintln!(
            "state {} progress {} done at {:?}",
            site.machine.state, site.progress, done_at
        );
        assert!(done_at.is_some(), "the house was not built");
        let after = stock_table(&town.registered_storages(), &town.materials);
        assert_eq!(before[0] - after[0], 16, "logs");
        assert_eq!(before[1] - after[1], 8, "stone");
        let p = &town.placed[id];
        assert_eq!(p.mesh, 2);
        let stamped = (p.cell.1..p.cell.1 + p.footprint.1)
            .flat_map(|y| (p.cell.0..p.cell.0 + p.footprint.0).map(move |x| (y * n + x) as usize))
            .filter(|&i| town.cells.objects[i] == Some(id))
            .count();
        assert!(stamped > 0, "footprint not stamped");
        assert!(town.construction.jobs.is_empty());
        // Builders keep their profession without a workplace once the site is done.
        assert_eq!(town.profession_counts().0[builder], 2);
        assert!(town.agents.states.iter().all(|s| s.workplace.is_none()));
        assert_reservations_held(&town);
    }

    /// Every reserved stored unit belongs to an action still queued (a family carrying goods
    /// home once the house is finished): construction left none behind.
    fn assert_reservations_held(town: &StartingTown) {
        let mut held = std::collections::BTreeMap::new();
        for ch in &town.agents.characters {
            for (storage, material, n) in ch.stack.pending_reservations() {
                *held.entry((storage, material)).or_insert(0) += n;
            }
        }
        for (si, s) in town.storages.iter().enumerate() {
            for e in &s.items {
                let h = held.get(&(si, e.material)).copied().unwrap_or(0);
                assert_eq!(
                    e.reserved, h,
                    "storage {si} material {} reserved",
                    e.material
                );
            }
        }
    }

    /// A wood house placed over natural resources: laborers fell them (the site clears), the
    /// piles they leave are hauled to storage, then the house is supplied and built.
    #[test]
    fn wood_house_on_resources_gets_cleared_and_built() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../game-data/original/WinData");
        let mut archive = Archive::default();
        for name in ["data1.pkg", "patch1.pkg", "patch0.pkg", "data0.pkg"] {
            let Ok(bytes) = std::fs::read(dir.join(name)) else {
                return;
            };
            archive.add(Package::parse(bytes).unwrap());
        }
        let world = crate::world::World::generate(
            &archive,
            &crate::world::WorldParams {
                seed: 1234,
                ..Default::default()
            },
        )
        .unwrap();
        let terrain = HeightGrid {
            width: world.terrain.width(),
            heights: &world.terrain.heights,
        };
        let mut rng = Random::new(1234);
        let mut town = StartingTown::create(
            &archive,
            "Game/StartCondition/Medium.rsc:resource",
            world.terrain.size,
            &terrain,
            &mut rng,
        )
        .unwrap();
        let mut tool = PlacementTool::start(
            &archive,
            &mut town,
            r"Template\WoodHouse.rsc:resource",
            &mut rng,
        )
        .unwrap();
        let spot = tool
            .find_spot(&town, &terrain, town.start, true)
            .expect("no spot with resources");
        tool.hover(&town, &terrain, Some(spot), 0);
        let rect = tool.rect.unwrap();
        let n = town.cells.size as i32;
        let resources: Vec<usize> = (rect.y0..=rect.y1)
            .flat_map(|y| (rect.x0..=rect.x1).map(move |x| (y * n + x) as usize))
            .filter_map(|i| town.cells.objects[i])
            .collect();
        assert!(!resources.is_empty());
        let id = tool.place(&archive, &mut town, 0).unwrap();
        let builder = town.construction.professions.builder;
        town.construction
            .commands
            .push(banished_game::professions::Command::SetCount {
                profession: builder,
                count: 2,
            });
        let mut warnings = banished_game::events::StockWarnings::default();
        let dt = 1.0 / 30.0;
        let (mut cleared_at, mut done_at, mut most_piles) = (None, None, 0);
        for step in 0..30 * 60 * 12 {
            town.apply_commands(&terrain, &mut rng);
            town.iterate(&terrain, &mut rng, dt, 50.0);
            let stock = std::array::from_fn(|i| (warnings.count[i], warnings.limits[i]));
            town.update_citizens(&terrain, &mut rng, dt, 50.0, stock);
            warnings.recompute(&town.registered_storages(), &town.materials);
            town.construction
                .jobs
                .step(banished_game::scheduler::ticks(dt));
            most_piles = most_piles.max(town.construction.piles.len());
            if cleared_at.is_none() && resources.iter().all(|&r| !town.placed[r].alive) {
                cleared_at = Some(step);
            }
            if town.construction.sites[&id].complete {
                done_at = Some(step);
                break;
            }
        }
        eprintln!(
            "resources {} cleared at {:?}, most piles {}, built at {:?}, piles left {}",
            resources.len(),
            cleared_at,
            most_piles,
            done_at,
            town.construction.piles.len()
        );
        assert!(cleared_at.is_some(), "the footprint was not cleared");
        assert!(most_piles > 0, "no piles were left");
        assert!(done_at.is_some(), "the house was not built");
        assert!(town.construction.claimed_resources.is_empty());
    }

    /// A wood cutter near the start is built, two woodcutters take the ConsumeProduce work:
    /// they fetch logs into the yard, work cycles and leave firewood piles that laborers haul to
    /// storage.
    #[test]
    fn cutter_yard_makes_firewood() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../game-data/original/WinData");
        let mut archive = Archive::default();
        for name in ["data1.pkg", "patch1.pkg", "patch0.pkg", "data0.pkg"] {
            let Ok(bytes) = std::fs::read(dir.join(name)) else {
                return;
            };
            archive.add(Package::parse(bytes).unwrap());
        }
        let world = crate::world::World::generate(
            &archive,
            &crate::world::WorldParams {
                seed: 1234,
                ..Default::default()
            },
        )
        .unwrap();
        let terrain = HeightGrid {
            width: world.terrain.width(),
            heights: &world.terrain.heights,
        };
        let mut rng = Random::new(1234);
        let mut town = StartingTown::create(
            &archive,
            "Game/StartCondition/Medium.rsc:resource",
            world.terrain.size,
            &terrain,
            &mut rng,
        )
        .unwrap();
        let mut tool = PlacementTool::start(
            &archive,
            &mut town,
            r"Template\CutterYard.rsc:resource",
            &mut rng,
        )
        .unwrap();
        let spot = tool.find_clear_spot(&town, &terrain, town.start);
        tool.hover(&town, &terrain, spot, 0);
        let id = tool.place(&archive, &mut town, 0).unwrap();
        let p = &town.construction.professions;
        let builder = p.builder;
        let cutter = p
            .list
            .iter()
            .position(|p| p.reference.to_ascii_lowercase().ends_with(":cutter"))
            .expect("no cutter profession");
        town.construction
            .commands
            .push(banished_game::professions::Command::SetCount {
                profession: builder,
                count: 2,
            });
        let firewood = town
            .materials
            .iter()
            .position(|m| m.reference.contains("Firewood"));
        let mut warnings = banished_game::events::StockWarnings::default();
        // The Medium start holds the default firewood limit (200) already: raise it, as the
        // yard's limit control would.
        warnings.limits[3] = 1000;
        let dt = 1.0 / 30.0;
        let (mut built_at, mut first_pile, mut stored_at) = (None, None, None);
        let mut logs_before = None;
        let mut seen = std::collections::BTreeSet::new();
        for step in 0..30 * 60 * 12 {
            town.apply_commands(&terrain, &mut rng);
            town.iterate(&terrain, &mut rng, dt, 50.0);
            let stock = std::array::from_fn(|i| (warnings.count[i], warnings.limits[i]));
            town.update_citizens(&terrain, &mut rng, dt, 50.0, stock);
            warnings.recompute(&town.registered_storages(), &town.materials);
            town.construction
                .jobs
                .step(banished_game::scheduler::ticks(dt));
            if built_at.is_none() && town.construction.sites[&id].complete {
                built_at = Some(step);
                let stocked = stock_table(&town.registered_storages(), &town.materials);
                logs_before = Some(stocked[0]);
                town.construction
                    .commands
                    .push(banished_game::professions::Command::SetCount {
                        profession: cutter,
                        count: 2,
                    });
            }
            let piles: Vec<usize> = town
                .construction
                .piles
                .iter()
                .filter(|(_, p)| Some(p.material) == firewood)
                .map(|(&e, _)| e)
                .collect();
            if first_pile.is_none() && !piles.is_empty() {
                first_pile = Some(step);
            }
            seen.extend(piles.iter().copied());
            if seen.iter().any(|e| !piles.contains(e)) {
                stored_at = Some(step);
                break;
            }
        }
        let site = &town.construction.sites[&id];
        eprintln!(
            "built {built_at:?}, first firewood pile {first_pile:?}, stored {stored_at:?}, \
             state {} produced {:?}, yard {:?}",
            site.machine.state,
            site.produce.as_ref().map(|p| p.produced),
            town.storages[site.storage].items
        );
        assert!(built_at.is_some(), "the yard was not built");
        assert!(first_pile.is_some(), "no firewood was made");
        assert!(stored_at.is_some(), "no firewood pile was hauled away");
        let after = stock_table(&town.registered_storages(), &town.materials);
        assert!(after[0] < logs_before.unwrap(), "no logs were used");
    }

    /// A gatherer's shelter near the start gets two gatherers: they harvest grown wild food
    /// within the radius, and the piles they leave are picked up (by them first, through the
    /// AutoPickup) and stored.
    #[test]
    fn gatherers_harvest_wild_food() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../game-data/original/WinData");
        let mut archive = Archive::default();
        for name in ["data1.pkg", "patch1.pkg", "patch0.pkg", "data0.pkg"] {
            let Ok(bytes) = std::fs::read(dir.join(name)) else {
                return;
            };
            archive.add(Package::parse(bytes).unwrap());
        }
        let world = crate::world::World::generate(
            &archive,
            &crate::world::WorldParams {
                seed: 1234,
                ..Default::default()
            },
        )
        .unwrap();
        let terrain = HeightGrid {
            width: world.terrain.width(),
            heights: &world.terrain.heights,
        };
        let mut rng = Random::new(1234);
        let mut town = StartingTown::create(
            &archive,
            "Game/StartCondition/Medium.rsc:resource",
            world.terrain.size,
            &terrain,
            &mut rng,
        )
        .unwrap();
        let mut tool = PlacementTool::start(
            &archive,
            &mut town,
            r"Template\GathererShelter.rsc:resource",
            &mut rng,
        )
        .unwrap();
        let spot = tool.find_clear_spot(&town, &terrain, town.start);
        tool.hover(&town, &terrain, spot, 0);
        let id = tool.place(&archive, &mut town, 0).unwrap();
        let st = town.construction.sites[&id].template;
        let kinds = town.construction.templates[st]
            .herbalist
            .clone()
            .expect("no HerbalistDescription")
            .resources;
        let p = &town.construction.professions;
        let builder = p.builder;
        let gatherer = p
            .list
            .iter()
            .position(|p| p.reference.to_ascii_lowercase().ends_with(":gatherer"))
            .expect("no gatherer profession");
        town.construction
            .commands
            .push(banished_game::professions::Command::SetCount {
                profession: builder,
                count: 2,
            });
        let mut warnings = banished_game::events::StockWarnings::default();
        warnings.limits[5] = 100_000;
        let wild = |town: &StartingTown| {
            town.placed
                .iter()
                .filter(|p| p.alive && !p.removal_requested && kinds.contains(&p.template))
                .count()
        };
        let wild_before = wild(&town);
        let dt = 1.0 / 30.0;
        let (mut built_at, mut harvested_at, mut stored_at) = (None, None, None);
        let mut seen = std::collections::BTreeSet::new();
        let mut auto = 0;
        for step in 0..30 * 60 * 12 {
            town.apply_commands(&terrain, &mut rng);
            town.iterate(&terrain, &mut rng, dt, 50.0);
            let stock = std::array::from_fn(|i| (warnings.count[i], warnings.limits[i]));
            town.update_citizens(&terrain, &mut rng, dt, 50.0, stock);
            warnings.recompute(&town.registered_storages(), &town.materials);
            town.construction
                .jobs
                .step(banished_game::scheduler::ticks(dt));
            if built_at.is_none() && town.construction.sites[&id].complete {
                built_at = Some(step);
                town.construction
                    .commands
                    .push(banished_game::professions::Command::SetCount {
                        profession: gatherer,
                        count: 2,
                    });
            }
            let site = &town.construction.sites[&id];
            if let Some(a) = &site.auto_pickup {
                auto = auto.max(a.works.len());
            }
            let piles: Vec<usize> = town
                .construction
                .piles
                .iter()
                .filter(|(_, p)| p.auto_pickup == Some(id))
                .map(|(&e, _)| e)
                .collect();
            if harvested_at.is_none() && !piles.is_empty() {
                harvested_at = Some(step);
            }
            seen.extend(piles.iter().copied());
            if seen.iter().any(|e| !piles.contains(e)) {
                stored_at = Some(step);
                break;
            }
        }
        eprintln!(
            "built {built_at:?}, harvested {harvested_at:?}, stored {stored_at:?}, wild {} -> {}, \
             auto-pickup list up to {auto}",
            wild_before,
            wild(&town)
        );
        assert!(built_at.is_some(), "the shelter was not built");
        assert!(harvested_at.is_some(), "nothing was gathered");
        assert!(auto > 0, "the AutoPickup recorded no pile");
        assert!(stored_at.is_some(), "no gathered pile was picked up");
        assert!(wild(&town) < wild_before);
        assert_reservations_held(&town);
    }

    /// A forester's lodge near the start gets two foresters: they plant saplings on free
    /// ground in the lodge's region and cut grown trees, whose logs are picked up.
    #[test]
    fn foresters_plant_and_cut() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../game-data/original/WinData");
        let mut archive = Archive::default();
        for name in ["data1.pkg", "patch1.pkg", "patch0.pkg", "data0.pkg"] {
            let Ok(bytes) = std::fs::read(dir.join(name)) else {
                return;
            };
            archive.add(Package::parse(bytes).unwrap());
        }
        let world = crate::world::World::generate(
            &archive,
            &crate::world::WorldParams {
                seed: 1234,
                ..Default::default()
            },
        )
        .unwrap();
        let terrain = HeightGrid {
            width: world.terrain.width(),
            heights: &world.terrain.heights,
        };
        let mut rng = Random::new(1234);
        let mut town = StartingTown::create(
            &archive,
            "Game/StartCondition/Medium.rsc:resource",
            world.terrain.size,
            &terrain,
            &mut rng,
        )
        .unwrap();
        let mut tool = PlacementTool::start(
            &archive,
            &mut town,
            r"Template\ForesterLodge.rsc:resource",
            &mut rng,
        )
        .unwrap();
        let spot = tool.find_clear_spot(&town, &terrain, town.start);
        tool.hover(&town, &terrain, spot, 0);
        let id = tool.place(&archive, &mut town, 0).unwrap();
        let st = town.construction.sites[&id].template;
        let tree = town.construction.templates[st]
            .forester
            .clone()
            .expect("no ForesterDescription")
            .tree;
        let p = &town.construction.professions;
        let builder = p.builder;
        let forester = p
            .list
            .iter()
            .position(|p| p.reference.to_ascii_lowercase().ends_with(":forester"))
            .expect("no forester profession");
        town.construction
            .commands
            .push(banished_game::professions::Command::SetCount {
                profession: builder,
                count: 2,
            });
        let mut warnings = banished_game::events::StockWarnings::default();
        warnings.limits[0] = 100_000;
        let saplings = |town: &StartingTown| {
            town.placed
                .iter()
                .filter(|p| p.alive && p.template == tree && p.growth < 0.05)
                .count()
        };
        let dt = 1.0 / 30.0;
        let (mut built_at, mut planted_at, mut cut_at, mut stored_at) = (None, None, None, None);
        let mut base = 0;
        let mut seen = std::collections::BTreeSet::new();
        for step in 0..30 * 60 * 15 {
            town.apply_commands(&terrain, &mut rng);
            town.iterate(&terrain, &mut rng, dt, 50.0);
            let stock = std::array::from_fn(|i| (warnings.count[i], warnings.limits[i]));
            town.update_citizens(&terrain, &mut rng, dt, 50.0, stock);
            warnings.recompute(&town.registered_storages(), &town.materials);
            town.construction
                .jobs
                .step(banished_game::scheduler::ticks(dt));
            if built_at.is_none() && town.construction.sites[&id].complete {
                built_at = Some(step);
                base = saplings(&town);
                town.construction
                    .commands
                    .push(banished_game::professions::Command::SetCount {
                        profession: forester,
                        count: 2,
                    });
            }
            if built_at.is_some() && planted_at.is_none() && saplings(&town) > base {
                planted_at = Some(step);
            }
            let piles: Vec<usize> = town
                .construction
                .piles
                .iter()
                .filter(|(_, p)| p.auto_pickup == Some(id))
                .map(|(&e, _)| e)
                .collect();
            if cut_at.is_none() && !piles.is_empty() {
                cut_at = Some(step);
            }
            seen.extend(piles.iter().copied());
            if stored_at.is_none() && seen.iter().any(|e| !piles.contains(e)) {
                stored_at = Some(step);
            }
            if planted_at.is_some() && stored_at.is_some() {
                break;
            }
        }
        eprintln!(
            "built {built_at:?}, planted {planted_at:?}, cut {cut_at:?}, stored {stored_at:?}, \
             toggles {:?}",
            town.construction.sites[&id].forester
        );
        assert!(built_at.is_some(), "the lodge was not built");
        assert!(planted_at.is_some(), "no sapling was planted");
        assert!(cut_at.is_some(), "no tree was cut");
        assert!(stored_at.is_some(), "no log pile was picked up");
        assert_reservations_held(&town);
    }

    /// A well near the start: once built, its HappinessComponent registers with the town and
    /// marks layer 0 within its radius; the citizens' views see it as a loitering spot.
    #[test]
    fn a_well_registers_its_happiness() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../game-data/original/WinData");
        let mut archive = Archive::default();
        for name in ["data1.pkg", "patch1.pkg", "patch0.pkg", "data0.pkg"] {
            let Ok(bytes) = std::fs::read(dir.join(name)) else {
                return;
            };
            archive.add(Package::parse(bytes).unwrap());
        }
        let world = crate::world::World::generate(
            &archive,
            &crate::world::WorldParams {
                seed: 1234,
                ..Default::default()
            },
        )
        .unwrap();
        let terrain = HeightGrid {
            width: world.terrain.width(),
            heights: &world.terrain.heights,
        };
        let mut rng = Random::new(1234);
        let mut town = StartingTown::create(
            &archive,
            "Game/StartCondition/Medium.rsc:resource",
            world.terrain.size,
            &terrain,
            &mut rng,
        )
        .unwrap();
        let mut tool =
            PlacementTool::start(&archive, &mut town, r"Template\Well.rsc:resource", &mut rng)
                .unwrap();
        let spot = tool.find_clear_spot(&town, &terrain, town.start);
        tool.hover(&town, &terrain, spot, 0);
        let id = tool.place(&archive, &mut town, 0).unwrap();
        let builder = town.construction.professions.builder;
        town.construction
            .commands
            .push(banished_game::professions::Command::SetCount {
                profession: builder,
                count: 2,
            });
        let mut warnings = banished_game::events::StockWarnings::default();
        let dt = 1.0 / 30.0;
        let mut registered_at = None;
        for step in 0..30 * 60 * 10 {
            town.apply_commands(&terrain, &mut rng);
            town.iterate(&terrain, &mut rng, dt, 50.0);
            let stock = std::array::from_fn(|i| (warnings.count[i], warnings.limits[i]));
            town.update_citizens(&terrain, &mut rng, dt, 50.0, stock);
            warnings.recompute(&town.registered_storages(), &town.materials);
            town.construction
                .jobs
                .step(banished_game::scheduler::ticks(dt));
            if town.construction.happiness.contains(&id) {
                registered_at = Some(step);
                break;
            }
        }
        eprintln!("registered at {registered_at:?}");
        assert!(registered_at.is_some(), "the well never registered");
        let st = town.construction.sites[&id].template;
        let radius = town.construction.templates[st].radius.unwrap();
        let p = &town.placed[id];
        let n = town.cells.size as i32;
        let centre = (p.cell.1 + (p.footprint.1 >> 1)) * n + p.cell.0 + (p.footprint.0 >> 1);
        assert_eq!(town.cells.layers[centre as usize][0], 1);
        assert_eq!(town.cells.layer_totals[0], 1);
        let far = centre + radius + 1;
        assert_eq!(town.cells.layers[far as usize][0], 0);
        let views =
            banished_game::agents::entity_views(&town.placed, &town.templates, &town.construction);
        assert_eq!(views[id].happiness, Some(2));
    }

    /// A fisherman's dock on the stream near the start: once built it counts on map layer 8
    /// with its share of water, and two fishermen land fish on its create points.
    #[test]
    fn fishermen_land_fish() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../game-data/original/WinData");
        let mut archive = Archive::default();
        for name in ["data1.pkg", "patch1.pkg", "patch0.pkg", "data0.pkg"] {
            let Ok(bytes) = std::fs::read(dir.join(name)) else {
                return;
            };
            archive.add(Package::parse(bytes).unwrap());
        }
        let world = crate::world::World::generate(
            &archive,
            &crate::world::WorldParams {
                seed: 1234,
                ..Default::default()
            },
        )
        .unwrap();
        let terrain = HeightGrid {
            width: world.terrain.width(),
            heights: &world.terrain.heights,
        };
        let mut rng = Random::new(1234);
        let mut town = StartingTown::create(
            &archive,
            "Game/StartCondition/Medium.rsc:resource",
            world.terrain.size,
            &terrain,
            &mut rng,
        )
        .unwrap();
        let mut tool = PlacementTool::start(
            &archive,
            &mut town,
            r"Template\FishermansDock.rsc:resource",
            &mut rng,
        )
        .unwrap();
        let spot = tool.find_clear_spot(&town, &terrain, town.start);
        assert!(spot.is_some(), "no shore near the start");
        tool.hover(&town, &terrain, spot, 0);
        let id = tool.place(&archive, &mut town, 0).unwrap();
        let p = &town.construction.professions;
        let builder = p.builder;
        let fisherman = p
            .list
            .iter()
            .position(|p| p.reference.to_ascii_lowercase().ends_with(":fisherman"))
            .expect("no fisherman profession");
        town.construction
            .commands
            .push(banished_game::professions::Command::SetCount {
                profession: builder,
                count: 2,
            });
        let fish = town
            .materials
            .iter()
            .position(|m| m.reference.contains("Fish"))
            .expect("no fish material");
        let mut warnings = banished_game::events::StockWarnings::default();
        warnings.limits[5] = 100_000;
        let dt = 1.0 / 30.0;
        let (mut built_at, mut landed_at) = (None, None);
        for step in 0..30 * 60 * 15 {
            town.apply_commands(&terrain, &mut rng);
            town.iterate(&terrain, &mut rng, dt, 50.0);
            let stock = std::array::from_fn(|i| (warnings.count[i], warnings.limits[i]));
            town.update_citizens(&terrain, &mut rng, dt, 50.0, stock);
            warnings.recompute(&town.registered_storages(), &town.materials);
            town.construction
                .jobs
                .step(banished_game::scheduler::ticks(dt));
            if built_at.is_none() && town.construction.sites[&id].complete {
                built_at = Some(step);
                town.construction
                    .commands
                    .push(banished_game::professions::Command::SetCount {
                        profession: fisherman,
                        count: 2,
                    });
            }
            if town.construction.piles.values().any(|p| p.material == fish) {
                landed_at = Some(step);
                break;
            }
        }
        let f = town.construction.sites[&id].fishing.unwrap();
        eprintln!("built {built_at:?}, landed {landed_at:?}, dock {f:?}");
        assert!(built_at.is_some(), "the dock was not built");
        assert!(f.registered && f.water > 0.0, "{f:?}");
        assert_eq!(town.cells.layer_totals[banished_game::fishing::LAYER], 1);
        assert!(landed_at.is_some(), "no fish was landed");
        assert_reservations_held(&town);
    }

    /// The Medium start's deer population: herds of 5–10 deer where there is habitat; they
    /// wander, and the herds keep their numbers.
    #[test]
    fn deer_herds_start_and_wander() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../game-data/original/WinData");
        let mut archive = Archive::default();
        for name in ["data1.pkg", "patch1.pkg", "patch0.pkg", "data0.pkg"] {
            let Ok(bytes) = std::fs::read(dir.join(name)) else {
                return;
            };
            archive.add(Package::parse(bytes).unwrap());
        }
        let world = crate::world::World::generate(
            &archive,
            &crate::world::WorldParams {
                seed: 1234,
                ..Default::default()
            },
        )
        .unwrap();
        let terrain = HeightGrid {
            width: world.terrain.width(),
            heights: &world.terrain.heights,
        };
        let mut rng = Random::new(1234);
        let mut town = StartingTown::create(
            &archive,
            "Game/StartCondition/Medium.rsc:resource",
            world.terrain.size,
            &terrain,
            &mut rng,
        )
        .unwrap();
        let w = &town.wildlife;
        let herds = w.herds.iter().filter(|h| !h.animals.is_empty()).count();
        eprintln!(
            "{} populations, {} herds ({} with deer), {} deer",
            w.populations.len(),
            w.herds.len(),
            herds,
            w.animals.len()
        );
        assert_eq!(w.populations.len(), 1);
        assert!(herds > 0 && w.animals.len() >= 3 * herds);
        // Herds start with `Rand(max − min) + min` deer; those already past their lifespan
        // die at once (`FUN_14010b8f0` with no time at activation).
        for h in &w.herds {
            assert!(h.animals.len() <= 9, "{}", h.animals.len());
        }
        let start: Vec<[f32; 3]> = w.animals.values().map(|a| a.character.position).collect();
        let dt = 1.0 / 30.0;
        let mut warnings = banished_game::events::StockWarnings::default();
        for _ in 0..30 * 60 {
            town.apply_commands(&terrain, &mut rng);
            town.iterate(&terrain, &mut rng, dt, 50.0);
            let stock = std::array::from_fn(|i| (warnings.count[i], warnings.limits[i]));
            town.update_citizens(&terrain, &mut rng, dt, 50.0, stock);
            warnings.recompute(&town.registered_storages(), &town.materials);
        }
        let moved = town
            .wildlife
            .animals
            .values()
            .zip(&start)
            .filter(|(a, p)| {
                let q = a.character.position;
                (q[0] - p[0]).hypot(q[1] - p[1]) > 1.0
            })
            .count();
        eprintln!("{moved} of {} deer moved", town.wildlife.animals.len());
        assert!(moved > 0, "no deer moved");
    }

    /// A hunter's lodge near the start gets two hunters: once the lodge has gathered its
    /// hunting time, a hunter who comes near deer takes one and its venison is left there.
    #[test]
    fn hunters_take_deer() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../game-data/original/WinData");
        let mut archive = Archive::default();
        for name in ["data1.pkg", "patch1.pkg", "patch0.pkg", "data0.pkg"] {
            let Ok(bytes) = std::fs::read(dir.join(name)) else {
                return;
            };
            archive.add(Package::parse(bytes).unwrap());
        }
        let world = crate::world::World::generate(
            &archive,
            &crate::world::WorldParams {
                seed: 1234,
                ..Default::default()
            },
        )
        .unwrap();
        let terrain = HeightGrid {
            width: world.terrain.width(),
            heights: &world.terrain.heights,
        };
        let mut rng = Random::new(1234);
        let mut town = StartingTown::create(
            &archive,
            "Game/StartCondition/Medium.rsc:resource",
            world.terrain.size,
            &terrain,
            &mut rng,
        )
        .unwrap();
        let mut tool = PlacementTool::start(
            &archive,
            &mut town,
            r"Template\HunterLodge.rsc:resource",
            &mut rng,
        )
        .unwrap();
        let spot = tool.find_clear_spot(&town, &terrain, town.start);
        tool.hover(&town, &terrain, spot, 0);
        let id = tool.place(&archive, &mut town, 0).unwrap();
        let p = &town.construction.professions;
        let builder = p.builder;
        let hunter = p
            .list
            .iter()
            .position(|p| p.reference.to_ascii_lowercase().ends_with(":hunter"))
            .expect("no hunter profession");
        town.construction
            .commands
            .push(banished_game::professions::Command::SetCount {
                profession: builder,
                count: 2,
            });
        let venison = town
            .materials
            .iter()
            .position(|m| m.reference.contains("Venison"))
            .expect("no venison material");
        let mut warnings = banished_game::events::StockWarnings::default();
        warnings.limits[5] = 100_000;
        let dt = 1.0 / 30.0;
        let (mut built_at, mut ready_at, mut taken_at) = (None, None, None);
        for step in 0..30 * 60 * 60 {
            town.apply_commands(&terrain, &mut rng);
            town.iterate(&terrain, &mut rng, dt, 50.0);
            let stock = std::array::from_fn(|i| (warnings.count[i], warnings.limits[i]));
            town.update_citizens(&terrain, &mut rng, dt, 50.0, stock);
            warnings.recompute(&town.registered_storages(), &town.materials);
            town.construction
                .jobs
                .step(banished_game::scheduler::ticks(dt));
            if built_at.is_none() && town.construction.sites[&id].complete {
                built_at = Some(step);
                town.construction
                    .commands
                    .push(banished_game::professions::Command::SetCount {
                        profession: hunter,
                        count: 2,
                    });
            }
            // The hunting time grows while staffed (it drops to 0 when a deer is taken).
            let time = town.construction.sites[&id].hunt_time.unwrap_or(0);
            if ready_at.is_none() && time > 0 {
                ready_at = Some(step);
            }
            if town
                .construction
                .piles
                .values()
                .any(|p| p.material == venison)
            {
                taken_at = Some(step);
                break;
            }
        }
        let lodge = town.placed[id].position;
        let near = town
            .wildlife
            .animals
            .values()
            .map(|a| {
                let p = a.character.position;
                (p[0] - lodge[0]).hypot(p[1] - lodge[1])
            })
            .fold(f32::MAX, f32::min);
        eprintln!(
            "built {built_at:?}, ready {ready_at:?}, venison {taken_at:?}, {} deer, nearest {near}",
            town.wildlife.animals.len()
        );
        assert!(built_at.is_some(), "the lodge was not built");
        assert!(ready_at.is_some(), "no hunting time was gathered");
        assert!(taken_at.is_some(), "no deer was taken");
        assert_reservations_held(&town);
    }

    /// A crop field near the start, given one of the town's known crops in spring: two
    /// farmers plant every cell, tend the crop while it grows, harvest it and the crop is
    /// stored.
    #[test]
    fn farmers_plant_tend_and_harvest_a_field() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../game-data/original/WinData");
        let mut archive = Archive::default();
        for name in ["data1.pkg", "patch1.pkg", "patch0.pkg", "data0.pkg"] {
            let Ok(bytes) = std::fs::read(dir.join(name)) else {
                return;
            };
            archive.add(Package::parse(bytes).unwrap());
        }
        let world = crate::world::World::generate(
            &archive,
            &crate::world::WorldParams {
                seed: 1234,
                ..Default::default()
            },
        )
        .unwrap();
        let terrain = HeightGrid {
            width: world.terrain.width(),
            heights: &world.terrain.heights,
        };
        let mut rng = Random::new(1234);
        let mut town = StartingTown::create(
            &archive,
            "Game/StartCondition/Medium.rsc:resource",
            world.terrain.size,
            &terrain,
            &mut rng,
        )
        .unwrap();
        let crop = town
            .construction
            .known
            .iter()
            .copied()
            .find(|&t| {
                town.templates[t].growth.is_some() && !town.templates[t].reference.contains("Tree")
            })
            .expect("no known crop");
        // The nearest 6 × 6 square of empty ground to the start (rings outwards).
        let n = town.cells.size as i32;
        let empty = |town: &StartingTown, x: i32, y: i32| {
            (y..y + 6).all(|cy| {
                (x..x + 6).all(|cx| {
                    let i = (cy * n + cx) as usize;
                    cx < n
                        && cy < n
                        && town.cells.types[i] == banished_game::terrain::CellType::GROUND
                        && town.cells.objects[i].is_none()
                        && town.cells.occupants[i].is_none()
                })
            })
        };
        let (sx, sy) = town.start;
        let rect = (8..60)
            .flat_map(|r| (-r..=r).flat_map(move |d| [(d, -r), (d, r), (-r, d), (r, d)]))
            .map(|(dx, dy)| (sx + dx, sy + dy))
            .find(|&(x, y)| x > 0 && y > 0 && empty(&town, x, y))
            .expect("no empty ground");
        let id = town
            .place_area(
                &archive,
                r"Template\CropField.rsc:resource",
                (rect.0, rect.1, 6, 6),
                &terrain,
            )
            .expect("the field was not placed");
        town.construction
            .commands
            .push(banished_game::professions::Command::SetCrop {
                entity: id,
                crop: Some(crop),
            });
        let farmer = town
            .construction
            .professions
            .list
            .iter()
            .position(|p| p.reference.to_ascii_lowercase().ends_with(":farmer"))
            .expect("no farmer profession");
        town.construction
            .commands
            .push(banished_game::professions::Command::SetCount {
                profession: farmer,
                count: 2,
            });
        let material = town.templates[crop]
            .pile
            .and_then(|p| town.templates[p].pile_material)
            .expect("the crop leaves no raw material");
        let mut warnings = banished_game::events::StockWarnings::default();
        warnings.limits[5] = 100_000;
        let dt = 1.0 / 30.0;
        let mut seen = std::collections::BTreeSet::new();
        let (mut planted_at, mut growing_at, mut harvest_at, mut stored_at) =
            (None, None, None, None);
        let mut calendar = banished_game::calendar::Calendar::default();
        for step in 0..30 * 60 * 60 {
            town.construction.calendar = calendar.seconds();
            town.apply_commands(&terrain, &mut rng);
            town.iterate(&terrain, &mut rng, dt, 60.0);
            let stock = std::array::from_fn(|i| (warnings.count[i], warnings.limits[i]));
            town.update_citizens(&terrain, &mut rng, dt, 60.0, stock);
            warnings.recompute(&town.registered_storages(), &town.materials);
            town.construction
                .jobs
                .step(banished_game::scheduler::ticks(dt));
            calendar.advance(banished_game::scheduler::ticks(dt));
            let site = &town.construction.sites[&id];
            let state = site.machine.state;
            if planted_at.is_none() && state == 9 {
                planted_at = Some(step);
            }
            if growing_at.is_none() && state == 0xa {
                growing_at = Some(step);
            }
            if harvest_at.is_none() && state == 0xb {
                harvest_at = Some(step);
            }
            let piles: Vec<usize> = town
                .construction
                .piles
                .iter()
                .filter(|(_, p)| p.material == material)
                .map(|(&e, _)| e)
                .collect();
            seen.extend(piles.iter().copied());
            if stored_at.is_none() && seen.iter().any(|e| !piles.contains(e)) {
                stored_at = Some(step);
                break;
            }
        }
        let f = town.construction.sites[&id].crop_field.clone().unwrap();
        eprintln!(
            "planting {planted_at:?}, growing {growing_at:?}, harvest {harvest_at:?}, stored \
             {stored_at:?}, {} cells, states {:?}",
            f.cells.len(),
            f.cells.iter().map(|c| c.state).collect::<Vec<_>>()
        );
        assert!(planted_at.is_some(), "the field never planted");
        assert!(growing_at.is_some(), "the crop never grew");
        assert!(harvest_at.is_some(), "the crop was never harvested");
        assert!(stored_at.is_some(), "no crop was stored");
        assert_reservations_held(&town);
    }

    /// An orchard near the start given the town's known tree: farmers plant every
    /// slot and tend; with the trees grown and their fruit ripe in autumn they pick it and
    /// the fruit is stored.
    #[test]
    fn arborists_plant_tend_and_pick_an_orchard() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../game-data/original/WinData");
        let mut archive = Archive::default();
        for name in ["data1.pkg", "patch1.pkg", "patch0.pkg", "data0.pkg"] {
            let Ok(bytes) = std::fs::read(dir.join(name)) else {
                return;
            };
            archive.add(Package::parse(bytes).unwrap());
        }
        let world = crate::world::World::generate(
            &archive,
            &crate::world::WorldParams {
                seed: 1234,
                ..Default::default()
            },
        )
        .unwrap();
        let terrain = HeightGrid {
            width: world.terrain.width(),
            heights: &world.terrain.heights,
        };
        let mut rng = Random::new(1234);
        let mut town = StartingTown::create(
            &archive,
            "Game/StartCondition/Medium.rsc:resource",
            world.terrain.size,
            &terrain,
            &mut rng,
        )
        .unwrap();
        let tree = town
            .construction
            .known
            .iter()
            .copied()
            .find(|&t| town.templates[t].fruit.is_some())
            .expect("no known orchard tree");
        let n = town.cells.size as i32;
        let empty = |town: &StartingTown, x: i32, y: i32| {
            (y..y + 7).all(|cy| {
                (x..x + 7).all(|cx| {
                    let i = (cy * n + cx) as usize;
                    cx < n
                        && cy < n
                        && town.cells.types[i] == banished_game::terrain::CellType::GROUND
                        && town.cells.objects[i].is_none()
                        && town.cells.occupants[i].is_none()
                })
            })
        };
        let (sx, sy) = town.start;
        let rect = (8..60)
            .flat_map(|r| (-r..=r).flat_map(move |d| [(d, -r), (d, r), (-r, d), (r, d)]))
            .map(|(dx, dy)| (sx + dx, sy + dy))
            .find(|&(x, y)| x > 0 && y > 0 && empty(&town, x, y))
            .expect("no empty ground");
        let id = town
            .place_area(
                &archive,
                r"Template\Orchard.rsc:resource",
                (rect.0, rect.1, 7, 7),
                &terrain,
            )
            .expect("the orchard was not placed");
        town.construction
            .commands
            .push(banished_game::professions::Command::SetCrop {
                entity: id,
                crop: Some(tree),
            });
        let arborist = town
            .construction
            .professions
            .list
            .iter()
            .position(|p| p.reference.to_ascii_lowercase().ends_with(":farmer"))
            .expect("no farmer profession");
        town.construction
            .commands
            .push(banished_game::professions::Command::SetCount {
                profession: arborist,
                count: 2,
            });
        let fruit = town.templates[tree]
            .fruit
            .and_then(|f| f.pile)
            .and_then(|p| town.templates[p].pile_material)
            .expect("no fruit material");
        let mut warnings = banished_game::events::StockWarnings::default();
        warnings.limits[5] = 100_000;
        let dt = 1.0 / 30.0;
        let mut calendar = banished_game::calendar::Calendar::default();
        let (mut tending_at, mut picking_at, mut stored_at) = (None, None, None);
        let mut seen = std::collections::BTreeSet::new();
        for step in 0..30 * 60 * 30 {
            town.construction.calendar = calendar.seconds();
            town.construction.year = calendar.year;
            town.apply_commands(&terrain, &mut rng);
            town.iterate(&terrain, &mut rng, dt, 60.0);
            let stock = std::array::from_fn(|i| (warnings.count[i], warnings.limits[i]));
            town.update_citizens(&terrain, &mut rng, dt, 60.0, stock);
            warnings.recompute(&town.registered_storages(), &town.materials);
            town.construction
                .jobs
                .step(banished_game::scheduler::ticks(dt));
            calendar.advance(banished_game::scheduler::ticks(dt));
            let state = town.construction.sites[&id].machine.state;
            if tending_at.is_none() && state == 0xa {
                tending_at = Some(step);
                // Fast-forward: the trees are grown, their fruit ripe, and it is autumn.
                let trees: Vec<usize> = town.construction.sites[&id]
                    .orchard
                    .as_ref()
                    .unwrap()
                    .trees
                    .iter()
                    .filter_map(|t| t.entity)
                    .collect();
                for t in trees {
                    town.placed[t].growth = 1.0;
                    town.construction.fruits.entry(t).or_default().ripeness = 1.0;
                }
                calendar.elapsed = banished_game::scheduler::ticks(8.0 * 300.0);
            }
            if picking_at.is_none() && state == 0xb {
                picking_at = Some(step);
            }
            let piles: Vec<usize> = town
                .construction
                .piles
                .iter()
                .filter(|(_, p)| p.material == fruit)
                .map(|(&e, _)| e)
                .collect();
            seen.extend(piles.iter().copied());
            if stored_at.is_none() && seen.iter().any(|e| !piles.contains(e)) {
                stored_at = Some(step);
                break;
            }
        }
        let o = town.construction.sites[&id].orchard.clone().unwrap();
        eprintln!(
            "tending {tending_at:?}, picking {picking_at:?}, stored {stored_at:?}, {} trees, \
             states {:?}",
            o.trees.len(),
            o.trees.iter().map(|t| t.state).collect::<Vec<_>>()
        );
        assert!(tending_at.is_some(), "the orchard was never planted");
        assert!(picking_at.is_some(), "the fruit was never picked");
        assert!(stored_at.is_some(), "no fruit was stored");
        assert_reservations_held(&town);
    }

    /// An easy start's livestock wait for a pasture; a fenced pasture is built with its shelter
    /// and fences, takes them once its kind is chosen, its herdsmen tend it (a visit lets an
    /// animal breed) and young are born; with a lower limit the herdsmen cull the oldest
    /// grown animal, which leaves its raw materials.
    #[test]
    fn herdsmen_keep_a_pasture() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../game-data/original/WinData");
        let mut archive = Archive::default();
        for name in ["data1.pkg", "patch1.pkg", "patch0.pkg", "data0.pkg"] {
            let Ok(bytes) = std::fs::read(dir.join(name)) else {
                return;
            };
            archive.add(Package::parse(bytes).unwrap());
        }
        let world = crate::world::World::generate(
            &archive,
            &crate::world::WorldParams {
                seed: 1234,
                ..Default::default()
            },
        )
        .unwrap();
        let terrain = HeightGrid {
            width: world.terrain.width(),
            heights: &world.terrain.heights,
        };
        let mut rng = Random::new(1234);
        let mut town = StartingTown::create(
            &archive,
            "Game/StartCondition/Easy.rsc:resource",
            world.terrain.size,
            &terrain,
            &mut rng,
        )
        .unwrap();
        let waiting = town.wildlife.unassigned.clone();
        assert_eq!(waiting.len(), 8, "the starting livestock");
        let kind = town.wildlife.animals[&waiting[0]].template;
        assert!(town.construction.known.contains(&kind));
        let n = town.cells.size as i32;
        let size = 10;
        let empty = |town: &StartingTown, x: i32, y: i32| {
            (y..y + size).all(|cy| {
                (x..x + size).all(|cx| {
                    let i = (cy * n + cx) as usize;
                    cx < n
                        && cy < n
                        && town.cells.types[i] == banished_game::terrain::CellType::GROUND
                        && town.cells.objects[i].is_none()
                        && town.cells.occupants[i].is_none()
                })
            })
        };
        let (sx, sy) = town.start;
        let rect = (8..60)
            .flat_map(|r| (-r..=r).flat_map(move |d| [(d, -r), (d, r), (-r, d), (r, d)]))
            .map(|(dx, dy)| (sx + dx, sy + dy))
            .find(|&(x, y)| x > 0 && y > 0 && empty(&town, x, y))
            .expect("no empty ground");
        let id = town
            .place_fenced(
                &archive,
                r"Template\Pasture.rsc:resource",
                rect,
                (rect.0 + size - 1, rect.1 + size - 1),
                0,
                &terrain,
            )
            .expect("the pasture was not placed");
        let parts = town.construction.sites[&id].parts.clone();
        let named = |t: &str| {
            parts
                .iter()
                .filter(|&&p| {
                    town.templates[town.placed[p].template]
                        .reference
                        .contains(t)
                })
                .count()
        };
        eprintln!(
            "parts: {} shelter {} corners {} gates {} long {}",
            parts.len(),
            named("AnimalShelter"),
            named("WoodFenceCorner"),
            named("WoodFenceGate"),
            named("WoodFenceStraightLong")
        );
        assert_eq!(named("AnimalShelter"), 1);
        assert_eq!(named("WoodFenceCorner"), 3, "the shelter takes one corner");
        assert_eq!(named("WoodFenceGate"), 4, "a gate on each side");
        let builder = town.construction.professions.builder;
        let herdsman = town
            .construction
            .professions
            .list
            .iter()
            .position(|p| p.reference.to_ascii_lowercase().ends_with(":herdsman"))
            .expect("no herdsman profession");
        // Herdsmen hired before the pasture can take them find it once it does
        // (`FUN_140115490`).
        for (profession, count) in [(builder, 3), (herdsman, 2)] {
            town.construction
                .commands
                .push(banished_game::professions::Command::SetCount { profession, count });
        }
        let mut warnings = banished_game::events::StockWarnings::default();
        warnings.limits = [100_000; 21];
        let dt = 1.0 / 30.0;
        let (mut built_at, mut penned_at, mut bred_at, mut born_at, mut culled_at) =
            (None, None, None, None, None);
        let mut chose = false;
        let initial: std::collections::BTreeSet<usize> =
            town.wildlife.animals.keys().copied().collect();
        let beef = town.wildlife.animal_kinds[&kind]
            .materials
            .first()
            .and_then(|&m| town.templates[m].pile_material);
        let mut culled = 0;
        for step in 0..30 * 60 * 40 {
            town.apply_commands(&terrain, &mut rng);
            town.iterate(&terrain, &mut rng, dt, 60.0);
            let stock = std::array::from_fn(|i| (warnings.count[i], warnings.limits[i]));
            town.update_citizens(&terrain, &mut rng, dt, 60.0, stock);
            warnings.recompute(&town.registered_storages(), &town.materials);
            town.construction
                .jobs
                .step(banished_game::scheduler::ticks(dt));
            let state = town.construction.sites[&id].machine.state;
            if built_at.is_none() && state == 2 {
                built_at = Some(step);
            }
            if built_at.is_some() && !chose {
                chose = true;
                town.construction
                    .commands
                    .push(banished_game::professions::Command::SetCrop {
                        entity: id,
                        crop: Some(kind),
                    });
            }
            let pasture = &town.wildlife.pastures[&id];
            let cap = town.wildlife.capacity(id);
            if penned_at.is_none() && 0 < cap && pasture.livestock.len() as i32 == cap.min(8) {
                penned_at = Some(step);
            }
            if penned_at.is_some() && bred_at.is_none() {
                let breeder = pasture.livestock.iter().copied().find(|a| {
                    let x = &town.wildlife.animals[a];
                    x.flags & banished_game::wildlife::BREEDER != 0
                });
                if let Some(a) = breeder {
                    bred_at = Some(step);
                    // Fast-forward: a grown male breeder with young almost due.
                    let males: Vec<usize> = pasture
                        .livestock
                        .iter()
                        .copied()
                        .filter(|a| town.wildlife.animals[a].male)
                        .collect();
                    let m = males.first().copied().unwrap_or(a);
                    let x = town.wildlife.animals.get_mut(&m).unwrap();
                    x.male = true;
                    x.flags |= banished_game::wildlife::BREEDER;
                    x.age = 2.0;
                    x.breed = 1;
                }
            }
            let young = town
                .wildlife
                .animals
                .iter()
                .any(|(a, x)| x.livestock.is_some() && !initial.contains(a));
            if bred_at.is_some() && born_at.is_none() && young {
                born_at = Some(step);
                // The herd is to shrink to two: the herdsmen cull.
                for a in town.wildlife.pastures[&id].livestock.clone() {
                    town.wildlife.animals.get_mut(&a).unwrap().age = 3.0;
                }
                town.construction
                    .commands
                    .push(banished_game::professions::Command::SetLimit {
                        entity: id,
                        limit: 2,
                    });
            }
            if born_at.is_some() {
                let left = town.wildlife.pastures[&id].livestock.len();
                if left <= 2 && culled_at.is_none() {
                    culled_at = Some(step);
                }
                culled = town
                    .construction
                    .piles
                    .values()
                    .filter(|p| Some(p.material) == beef)
                    .count();
                if culled_at.is_some() && 0 < culled {
                    break;
                }
            }
        }
        let p = &town.wildlife.pastures[&id];
        eprintln!(
            "built {built_at:?}, penned {penned_at:?}, bred {bred_at:?}, born {born_at:?}, \
             culled {culled_at:?}; {} animals, limit {}, {} meat piles",
            p.livestock.len(),
            p.limit,
            culled
        );
        assert!(built_at.is_some(), "the pasture was not built");
        assert!(
            penned_at.is_some(),
            "the livestock never joined the pasture"
        );
        assert!(bred_at.is_some(), "no herdsman's visit let an animal breed");
        assert!(born_at.is_some(), "no young were born");
        assert!(culled_at.is_some(), "the herd was not culled");
        assert!(0 < culled || beef.is_none(), "a culled animal left nothing");
        for &a in &town.wildlife.pastures[&id].livestock {
            let x = &town.wildlife.animals[&a];
            assert_eq!(x.flags & banished_game::wildlife::WILD, 0);
            assert_eq!(x.area, town.wildlife.pastures[&id].area);
        }
    }

    /// A seed-1234 Medium town with its terrain and stream, after `setup`.
    fn with_town(test: impl FnOnce(&Archive, &HeightGrid, &mut StartingTown, &mut Random)) {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../game-data/original/WinData");
        let mut archive = Archive::default();
        for name in ["data1.pkg", "patch1.pkg", "patch0.pkg", "data0.pkg"] {
            let Ok(bytes) = std::fs::read(dir.join(name)) else {
                eprintln!("game data missing; skipping life integration");
                return;
            };
            archive.add(Package::parse(bytes).unwrap());
        }
        let world = crate::world::World::generate(
            &archive,
            &crate::world::WorldParams {
                seed: 1234,
                ..Default::default()
            },
        )
        .unwrap();
        let terrain = HeightGrid {
            width: world.terrain.width(),
            heights: &world.terrain.heights,
        };
        let mut rng = Random::new(1234);
        let mut town = StartingTown::create(
            &archive,
            "Game/StartCondition/Medium.rsc:resource",
            world.terrain.size,
            &terrain,
            &mut rng,
        )
        .unwrap();
        test(&archive, &terrain, &mut town, &mut rng);
    }

    /// One simulation iteration as the gameplay loop runs it, at `temperature`.
    fn step(town: &mut StartingTown, terrain: &HeightGrid, rng: &mut Random, temperature: f32) {
        let dt = 1.0 / 30.0;
        town.apply_commands(terrain, rng);
        town.iterate(terrain, rng, dt, temperature);
        let warnings = banished_game::events::StockWarnings::default();
        let stock = std::array::from_fn(|i| (warnings.count[i], warnings.limits[i]));
        town.update_citizens(terrain, rng, dt, temperature, stock);
        town.construction
            .jobs
            .step(banished_game::scheduler::ticks(dt));
    }

    /// A finished wood house gets a family, who eat from the town's storage, carry food home
    /// and have a child there.
    #[test]
    fn a_family_moves_in_stocks_the_house_and_has_a_child() {
        with_town(|archive, terrain, town, rng| {
            let mut tool =
                PlacementTool::start(archive, town, r"Template\WoodHouse.rsc:resource", rng)
                    .unwrap();
            let spot = tool.find_clear_spot(town, terrain, town.start).unwrap();
            tool.hover(town, terrain, Some(spot), 0);
            let id = tool.place(archive, town, 0).unwrap();
            let builder = town.construction.professions.builder;
            town.construction
                .commands
                .push(banished_game::professions::Command::SetCount {
                    profession: builder,
                    count: 2,
                });
            let mut built = false;
            for _ in 0..30 * 60 * 8 {
                step(town, terrain, rng, 50.0);
                if town.construction.sites[&id].complete {
                    built = true;
                    break;
                }
            }
            assert!(built, "the house was not built");
            // Hungry enough to eat (`D[0xc8]`), not yet urgently.
            let hungry = town.agents.life.f(0xc8) + 1.0;
            for s in &mut town.agents.states {
                s.hunger = hungry;
            }
            let mut moved_in = None;
            for k in 0..30 * 60 * 3 {
                step(town, terrain, rng, 50.0);
                if moved_in.is_none() && town.construction.homes[&id].occupants() > 0 {
                    moved_in = Some(k);
                }
                let home = &town.construction.homes[&id];
                if moved_in.is_some()
                    && banished_game::life::stored(&town.storages[home.storage], &town.materials, 5)
                        > 0
                    && town.agents.states.iter().any(|s| !s.foods.is_empty())
                {
                    break;
                }
            }
            let home = town.construction.homes[&id].clone();
            eprintln!("moved in after {moved_in:?}: {:?}", home.families);
            assert!(moved_in.is_some(), "nobody moved in");
            let family = &home.families[0];
            for &c in family {
                assert_eq!(town.agents.states[c].home, Some(id));
            }
            let food =
                banished_game::life::stored(&town.storages[home.storage], &town.materials, 5);
            eprintln!("food at home: {food}");
            assert!(food > 0, "nobody carried food home");
            assert!(
                town.agents.states.iter().any(|s| !s.foods.is_empty()),
                "nobody ate"
            );
            assert!(town.citizens.iter().all(|c| c.alive), "someone died");
            // A married woman of the family: her pregnancy ends at once.
            let mother = family
                .iter()
                .copied()
                .find(|&c| {
                    let ci = &town.citizens[c];
                    !ci.male && ci.spouse.is_some() && ci.age >= town.rules.adult_age
                })
                .expect("no married woman in the house");
            let before = town.citizens.len();
            town.agents.states[mother].reproduction = 0.001;
            town.agents.states[mother].health = 10.0;
            for _ in 0..10 {
                step(town, terrain, rng, 50.0);
            }
            assert_eq!(town.citizens.len(), before + 1, "no child was born");
            let child = before;
            assert_eq!(town.citizens[child].parent, Some(mother));
            assert_eq!(town.agents.states[child].home, Some(id));
            assert!(town.construction.homes[&id].families[0].contains(&child));
            assert!(town.agents.registry.contains(&child));
            assert!(
                town.agents.states[mother].reproduction < 0.0,
                "no wait after the birth"
            );
        });
    }

    /// Without food, hungry citizens starve once idle: the entity goes, the family and the
    /// registry let go of it, and the log hears of it.
    #[test]
    fn citizens_starve_without_food() {
        with_town(|_, terrain, town, rng| {
            for s in &mut town.storages {
                let materials = &town.materials;
                s.items
                    .retain(|e| materials[e.material].flags >> 5 & 1 == 0);
            }
            let max = town.agents.life.f(0xd0);
            for s in &mut town.agents.states {
                s.hunger = max - 0.01;
            }
            let n = town.citizens.len();
            let mut events = Vec::new();
            for _ in 0..30 * 30 {
                step(town, terrain, rng, 50.0);
                events.extend(std::mem::take(&mut town.agents.events));
                if town.citizens.iter().all(|c| !c.alive) {
                    break;
                }
            }
            let dead = town.citizens.iter().filter(|c| !c.alive).count();
            eprintln!("{dead} of {n} starved");
            assert!(dead > 0, "nobody starved");
            assert_eq!(town.agents.registry.len(), n - dead);
            assert!(events
                .iter()
                .any(|e| e.kind == banished_game::life::reason::STARVED));
            for (i, c) in town.citizens.iter().enumerate().filter(|(_, c)| !c.alive) {
                assert!(c.spouse.is_none());
                assert!(!town.agents.characters[i].stack.is_active());
            }
        });
    }

    /// A staffed school takes a child coming of age, who sits lessons there (message 4).
    #[test]
    fn a_teacher_staffs_the_school_and_a_student_learns() {
        with_town(|archive, terrain, town, rng| {
            let mut tool =
                PlacementTool::start(archive, town, r"Template\SchoolHouse.rsc:resource", rng)
                    .unwrap();
            let spot = tool.find_clear_spot(town, terrain, town.start).unwrap();
            tool.hover(town, terrain, Some(spot), 0);
            let id = tool.place(archive, town, 0).unwrap();
            let builder = town.construction.professions.builder;
            town.construction
                .commands
                .push(banished_game::professions::Command::SetCount {
                    profession: builder,
                    count: 3,
                });
            let mut built = false;
            for _ in 0..30 * 60 * 12 {
                step(town, terrain, rng, 50.0);
                if town.construction.sites[&id].complete {
                    built = true;
                    break;
                }
            }
            assert!(built, "the school was not built");
            assert!(town.construction.attendance_list.contains(&id));
            let teacher = town.agents.life.services[0].expect("no teacher profession");
            town.construction
                .commands
                .push(banished_game::professions::Command::AddWorker { entity: id });
            for _ in 0..60 {
                step(town, terrain, rng, 50.0);
            }
            let staff = town.construction.workforce.workplaces[&id].workers.clone();
            assert_eq!(staff.len(), 1, "no teacher hired");
            assert_eq!(town.agents.states[staff[0]].profession, teacher);
            // A child about to come of age.
            let adult = town.rules.adult_age;
            let child = (0..town.citizens.len())
                .find(|&c| town.citizens[c].age < adult)
                .expect("no child");
            town.citizens[child].age = adult - 0.0001;
            let mut learned = false;
            for _ in 0..30 * 60 * 4 {
                step(town, terrain, rng, 50.0);
                if town.citizens[child].education > 0 {
                    learned = true;
                    break;
                }
            }
            eprintln!(
                "school {:?}, attendees {:?}, education {}",
                town.agents.states[child].school,
                town.construction.attendance[&id].attendees,
                town.citizens[child].education
            );
            assert_eq!(town.agents.states[child].school, Some(id));
            assert!(learned, "no lesson");
        });
    }

    /// Chickens in a pasture lay: every pile's worth gathered becomes an egg pile on a free
    /// shelter point, which is then hauled to storage.
    #[test]
    fn a_pasture_gathers_eggs() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../game-data/original/WinData");
        let mut archive = Archive::default();
        for name in ["data1.pkg", "patch1.pkg", "patch0.pkg", "data0.pkg"] {
            let Ok(bytes) = std::fs::read(dir.join(name)) else {
                return;
            };
            archive.add(Package::parse(bytes).unwrap());
        }
        let world = crate::world::World::generate(
            &archive,
            &crate::world::WorldParams {
                seed: 99,
                ..Default::default()
            },
        )
        .unwrap();
        let terrain = HeightGrid {
            width: world.terrain.width(),
            heights: &world.terrain.heights,
        };
        let mut rng = Random::new(99);
        let mut town = StartingTown::create(
            &archive,
            "Game/StartCondition/Medium.rsc:resource",
            world.terrain.size,
            &terrain,
            &mut rng,
        )
        .unwrap();
        let chicken = town
            .wildlife
            .livestock_kinds
            .iter()
            .find(|(_, k)| k.product.is_some() && k.interval == 1.0)
            .map(|(&t, _)| t)
            .expect("no chickens");
        let egg = town.wildlife.livestock_kinds[&chicken]
            .product
            .and_then(|p| town.templates[p].pile_material)
            .expect("no egg material");
        let n = town.cells.size as i32;
        let size = 8;
        let empty = |town: &StartingTown, x: i32, y: i32| {
            (y..y + size).all(|cy| {
                (x..x + size).all(|cx| {
                    let i = (cy * n + cx) as usize;
                    cx < n
                        && cy < n
                        && town.cells.types[i] == banished_game::terrain::CellType::GROUND
                        && town.cells.objects[i].is_none()
                        && town.cells.occupants[i].is_none()
                })
            })
        };
        let (sx, sy) = town.start;
        let rect = (8..60)
            .flat_map(|r| (-r..=r).flat_map(move |d| [(d, -r), (d, r), (-r, d), (r, d)]))
            .map(|(dx, dy)| (sx + dx, sy + dy))
            .find(|&(x, y)| x > 0 && y > 0 && empty(&town, x, y))
            .expect("no empty ground");
        let id = town
            .place_fenced(
                &archive,
                r"Template\Pasture.rsc:resource",
                rect,
                (rect.0 + size - 1, rect.1 + size - 1),
                2,
                &terrain,
            )
            .expect("the pasture was not placed");
        let builder = town.construction.professions.builder;
        town.construction
            .commands
            .push(banished_game::professions::Command::SetCount {
                profession: builder,
                count: 3,
            });
        let mut warnings = banished_game::events::StockWarnings::default();
        warnings.limits = [100_000; 21];
        let dt = 1.0 / 30.0;
        let (mut built_at, mut laid_at, mut stored_at) = (None, None, None);
        let mut seen = std::collections::BTreeSet::new();
        for step in 0..30 * 60 * 20 {
            town.apply_commands(&terrain, &mut rng);
            town.iterate(&terrain, &mut rng, dt, 60.0);
            let stock = std::array::from_fn(|i| (warnings.count[i], warnings.limits[i]));
            town.update_citizens(&terrain, &mut rng, dt, 60.0, stock);
            warnings.recompute(&town.registered_storages(), &town.materials);
            town.construction
                .jobs
                .step(banished_game::scheduler::ticks(dt));
            let state = town.construction.sites[&id].machine.state;
            if built_at.is_none() && state == 2 {
                built_at = Some(step);
                town.construction
                    .commands
                    .push(banished_game::professions::Command::SetCrop {
                        entity: id,
                        crop: Some(chicken),
                    });
                let herdsman = town
                    .construction
                    .professions
                    .list
                    .iter()
                    .position(|p| p.reference.to_ascii_lowercase().ends_with(":herdsman"))
                    .expect("no herdsman profession");
                town.construction
                    .commands
                    .push(banished_game::professions::Command::SetCount {
                        profession: herdsman,
                        count: 1,
                    });
            }
            let pasture = &town.wildlife.pastures[&id];
            if pasture.kind == Some(chicken) && pasture.livestock.is_empty() {
                // Four hens, bought in (`FUN_14012db40` without a parent).
                town.with_wild(&terrain, &mut rng, |w| {
                    for _ in 0..4 {
                        w.give_birth(id, None, true);
                    }
                });
            }
            if built_at.is_some() && laid_at.is_none() {
                // Fast-forward: the hens have laid three piles' worth.
                if town.wildlife.pastures[&id].kind == Some(chicken)
                    && town.wildlife.pastures[&id].produced == 0
                {
                    let amount = town.materials[egg].pile_amount;
                    town.wildlife.pastures.get_mut(&id).unwrap().produced = amount * 3 + 1;
                }
            }
            let piles: Vec<usize> = town
                .construction
                .piles
                .iter()
                .filter(|(_, p)| p.material == egg)
                .map(|(&e, _)| e)
                .collect();
            if laid_at.is_none() && !piles.is_empty() {
                laid_at = Some(step);
            }
            seen.extend(piles.iter().copied());
            if stored_at.is_none() && seen.iter().any(|e| !piles.contains(e)) {
                stored_at = Some(step);
                break;
            }
        }
        let p = &town.wildlife.pastures[&id];
        eprintln!(
            "built {built_at:?}, laid {laid_at:?}, stored {stored_at:?}; {} hens, {} piles seen",
            p.livestock.len(),
            seen.len()
        );
        assert!(built_at.is_some(), "the pasture was not built");
        assert!(4 <= p.livestock.len());
        assert!(laid_at.is_some(), "no eggs were laid");
        assert!(stored_at.is_some(), "no eggs were stored");
    }

    /// Blight on a field withers its crops and passes to the field next to it, which grows
    /// the same crop (and says so in the event log).
    #[test]
    fn blight_withers_a_field_and_spreads() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../game-data/original/WinData");
        let mut archive = Archive::default();
        for name in ["data1.pkg", "patch1.pkg", "patch0.pkg", "data0.pkg"] {
            let Ok(bytes) = std::fs::read(dir.join(name)) else {
                return;
            };
            archive.add(Package::parse(bytes).unwrap());
        }
        let world = crate::world::World::generate(
            &archive,
            &crate::world::WorldParams {
                seed: 1234,
                ..Default::default()
            },
        )
        .unwrap();
        let terrain = HeightGrid {
            width: world.terrain.width(),
            heights: &world.terrain.heights,
        };
        let mut rng = Random::new(1234);
        let mut town = StartingTown::create(
            &archive,
            "Game/StartCondition/Medium.rsc:resource",
            world.terrain.size,
            &terrain,
            &mut rng,
        )
        .unwrap();
        let crop = town
            .construction
            .known
            .iter()
            .copied()
            .find(|&t| {
                town.templates[t].growth.is_some() && !town.templates[t].reference.contains("Tree")
            })
            .expect("no known crop");
        let n = town.cells.size as i32;
        let empty = |town: &StartingTown, x: i32, y: i32| {
            (y..y + 10).all(|cy| {
                (x..x + 20).all(|cx| {
                    let i = (cy * n + cx) as usize;
                    cx < n
                        && cy < n
                        && town.cells.types[i] == banished_game::terrain::CellType::GROUND
                        && town.cells.occupants[i].is_none()
                        && town.cells.objects[i].is_none()
                })
            })
        };
        let (sx, sy) = town.start;
        let rect = (8..150)
            .flat_map(|r| (-r..=r).flat_map(move |d| [(d, -r), (d, r), (-r, d), (r, d)]))
            .map(|(dx, dy)| (sx + dx, sy + dy))
            .find(|&(x, y)| x > 0 && y > 0 && empty(&town, x, y))
            .expect("no empty ground");
        let mut fields = Vec::new();
        // Big enough that a spread draw (±20 cells) falls on the other field often.
        for dx in [0, 10] {
            let id = town
                .place_area(
                    &archive,
                    r"Template\CropField.rsc:resource",
                    (rect.0 + dx, rect.1, 10, 10),
                    &terrain,
                )
                .expect("the field was not placed");
            town.construction
                .commands
                .push(banished_game::professions::Command::SetCrop {
                    entity: id,
                    crop: Some(crop),
                });
            fields.push(id);
        }
        let farmer = town
            .construction
            .professions
            .list
            .iter()
            .position(|p| p.reference.to_ascii_lowercase().ends_with(":farmer"))
            .expect("no farmer profession");
        town.construction
            .commands
            .push(banished_game::professions::Command::SetCount {
                profession: farmer,
                count: 8,
            });
        let mut warnings = banished_game::events::StockWarnings::default();
        warnings.limits[5] = 100_000;
        let dt = 1.0 / 30.0;
        let mut calendar = banished_game::calendar::Calendar::default();
        let crops = |town: &StartingTown, e: usize| {
            let f = town.construction.sites[&e].crop_field.as_ref().unwrap();
            f.cells
                .iter()
                .filter_map(|c| c.entity)
                .filter(|&x| town.placed[x].alive && !town.placed[x].removal_requested)
                .count()
        };
        let (mut sick_at, mut withered, mut spread_at) = (None, false, None);
        let mut before = 0;
        let mut logged = false;
        for step in 0..30 * 60 * 60 {
            town.construction.calendar = calendar.seconds();
            town.apply_commands(&terrain, &mut rng);
            town.iterate(&terrain, &mut rng, dt, 60.0);
            let stock = std::array::from_fn(|i| (warnings.count[i], warnings.limits[i]));
            town.update_citizens(&terrain, &mut rng, dt, 60.0, stock);
            warnings.recompute(&town.registered_storages(), &town.materials);
            town.construction
                .jobs
                .step(banished_game::scheduler::ticks(dt));
            calendar.advance(banished_game::scheduler::ticks(dt));
            // Both fields have crops coming up (planting goes on).
            let growing = 20 <= crops(&town, fields[0]) && 5 <= crops(&town, fields[1]);
            if sick_at.is_none() && growing {
                sick_at = Some(step);
                before = crops(&town, fields[0]);
                let b = town.construction.sites.get_mut(&fields[0]).unwrap();
                let b = b.disease.as_mut().unwrap();
                b.left = banished_game::scheduler::ticks(8.0 * 300.0);
                // Fast-forward: 200 spread intervals at once (a draw lands on the other
                // field about one time in sixteen).
                b.spread = banished_game::scheduler::ticks(200.0 * 0.125 * 300.0);
            }
            if sick_at.is_some() && crops(&town, fields[0]) < before {
                withered = true;
            }
            for (kind, _, e) in std::mem::take(&mut town.construction.site_events) {
                logged |= kind == 0xc && e == fields[1];
            }
            let other = town.construction.sites[&fields[1]]
                .disease
                .as_ref()
                .unwrap();
            if sick_at.is_some() && spread_at.is_none() && 0 < other.left {
                spread_at = Some(step);
            }
            if withered && spread_at.is_some() {
                break;
            }
        }
        eprintln!(
            "sick {sick_at:?} with {before} crops, now {}, spread {spread_at:?}",
            crops(&town, fields[0])
        );
        assert!(sick_at.is_some(), "the fields never grew");
        assert!(withered, "no crop withered");
        assert!(spread_at.is_some(), "the blight did not spread");
        assert!(logged, "the event log was not told");
    }

    /// A storage yard placed over trees: laborers fell them, the logs left on the yard go
    /// straight into its storage, and once clear the yard joins the town's storages.
    #[test]
    fn a_storage_yard_absorbs_its_logs_and_joins_the_town() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../game-data/original/WinData");
        let mut archive = Archive::default();
        for name in ["data1.pkg", "patch1.pkg", "patch0.pkg", "data0.pkg"] {
            let Ok(bytes) = std::fs::read(dir.join(name)) else {
                return;
            };
            archive.add(Package::parse(bytes).unwrap());
        }
        let world = crate::world::World::generate(
            &archive,
            &crate::world::WorldParams {
                seed: 1234,
                ..Default::default()
            },
        )
        .unwrap();
        let terrain = HeightGrid {
            width: world.terrain.width(),
            heights: &world.terrain.heights,
        };
        let mut rng = Random::new(1234);
        let mut town = StartingTown::create(
            &archive,
            "Game/StartCondition/Medium.rsc:resource",
            world.terrain.size,
            &terrain,
            &mut rng,
        )
        .unwrap();
        let n = town.cells.size as i32;
        // A 5 × 5 patch near the start with a few trees and no buildings.
        let trees = |town: &StartingTown, x: i32, y: i32| {
            let mut count = 0;
            for cy in y..y + 5 {
                for cx in x..x + 5 {
                    if n <= cx || n <= cy {
                        return None;
                    }
                    let i = (cy * n + cx) as usize;
                    if town.cells.occupants[i].is_some() {
                        return None;
                    }
                    match town.cells.objects[i] {
                        None if town.cells.types[i] == banished_game::terrain::CellType::GROUND => {
                        }
                        Some(o)
                            if town.templates[town.placed[o].template]
                                .reference
                                .contains("Tree") =>
                        {
                            count += 1
                        }
                        _ => return None,
                    }
                }
            }
            Some(count)
        };
        let (sx, sy) = town.start;
        let rect = (6..80)
            .flat_map(|r| (-r..=r).flat_map(move |d| [(d, -r), (d, r), (-r, d), (r, d)]))
            .map(|(dx, dy)| (sx + dx, sy + dy))
            .find(|&(x, y)| x > 0 && y > 0 && trees(&town, x, y).is_some_and(|t| 3 <= t))
            .expect("no patch of trees");
        let before = town.storage_sites.len();
        let id = town
            .place_area(
                &archive,
                r"Template\StorageYard.rsc:resource",
                (rect.0, rect.1, 5, 5),
                &terrain,
            )
            .expect("the yard was not placed");
        let storage = town.construction.sites[&id].storage;
        let mut warnings = banished_game::events::StockWarnings::default();
        let dt = 1.0 / 30.0;
        let (mut absorbed_at, mut joined_at) = (None, None);
        for step in 0..30 * 60 * 15 {
            town.apply_commands(&terrain, &mut rng);
            town.iterate(&terrain, &mut rng, dt, 60.0);
            let stock = std::array::from_fn(|i| (warnings.count[i], warnings.limits[i]));
            town.update_citizens(&terrain, &mut rng, dt, 60.0, stock);
            warnings.recompute(&town.registered_storages(), &town.materials);
            town.construction
                .jobs
                .step(banished_game::scheduler::ticks(dt));
            if absorbed_at.is_none() && !town.storages[storage].items.is_empty() {
                absorbed_at = Some(step);
            }
            if joined_at.is_none() && town.storage_sites.iter().any(|s| s.entity == id) {
                joined_at = Some(step);
            }
            if absorbed_at.is_some() && joined_at.is_some() {
                break;
            }
        }
        eprintln!(
            "absorbed {absorbed_at:?}, joined {joined_at:?}, state {}, items {:?}",
            town.construction.sites[&id].machine.state, town.storages[storage].items
        );
        assert!(absorbed_at.is_some(), "no log went into the yard");
        assert!(
            joined_at.is_some(),
            "the yard never joined the town's storages"
        );
        assert_eq!(town.storage_sites.len(), before + 1);
    }

    /// A fenced cemetery is built and lays out its graves; a citizen who dies of old age is
    /// buried there (a tombstone on a grave) and the spouse grieves less (bit `0x40`).
    #[test]
    fn the_dead_are_buried_in_the_cemetery() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../game-data/original/WinData");
        let mut archive = Archive::default();
        for name in ["data1.pkg", "patch1.pkg", "patch0.pkg", "data0.pkg"] {
            let Ok(bytes) = std::fs::read(dir.join(name)) else {
                return;
            };
            archive.add(Package::parse(bytes).unwrap());
        }
        let world = crate::world::World::generate(
            &archive,
            &crate::world::WorldParams {
                seed: 1234,
                ..Default::default()
            },
        )
        .unwrap();
        let terrain = HeightGrid {
            width: world.terrain.width(),
            heights: &world.terrain.heights,
        };
        let mut rng = Random::new(1234);
        let mut town = StartingTown::create(
            &archive,
            "Game/StartCondition/Easy.rsc:resource",
            world.terrain.size,
            &terrain,
            &mut rng,
        )
        .unwrap();
        let n = town.cells.size as i32;
        let size = 8;
        let empty = |town: &StartingTown, x: i32, y: i32| {
            (y..y + size).all(|cy| {
                (x..x + size).all(|cx| {
                    let i = (cy * n + cx) as usize;
                    cx < n
                        && cy < n
                        && town.cells.types[i] == banished_game::terrain::CellType::GROUND
                        && town.cells.objects[i].is_none()
                        && town.cells.occupants[i].is_none()
                })
            })
        };
        let (sx, sy) = town.start;
        let rect = (8..80)
            .flat_map(|r| (-r..=r).flat_map(move |d| [(d, -r), (d, r), (-r, d), (r, d)]))
            .map(|(dx, dy)| (sx + dx, sy + dy))
            .find(|&(x, y)| x > 0 && y > 0 && empty(&town, x, y))
            .expect("no empty ground");
        let id = town
            .place_fenced(
                &archive,
                r"Template\Cemetery.rsc:resource",
                rect,
                (rect.0 + size - 1, rect.1 + size - 1),
                0,
                &terrain,
            )
            .expect("the cemetery was not placed");
        let builder = town.construction.professions.builder;
        town.construction
            .commands
            .push(banished_game::professions::Command::SetCount {
                profession: builder,
                count: 4,
            });
        let mut warnings = banished_game::events::StockWarnings::default();
        warnings.limits = [100_000; 21];
        let dt = 1.0 / 30.0;
        let (mut built_at, mut buried_at) = (None, None);
        let mut widow = None;
        for step in 0..30 * 60 * 30 {
            town.apply_commands(&terrain, &mut rng);
            town.iterate(&terrain, &mut rng, dt, 60.0);
            let stock = std::array::from_fn(|i| (warnings.count[i], warnings.limits[i]));
            town.update_citizens(&terrain, &mut rng, dt, 60.0, stock);
            warnings.recompute(&town.registered_storages(), &town.materials);
            town.construction
                .jobs
                .step(banished_game::scheduler::ticks(dt));
            if built_at.is_none() && town.construction.cemeteries.contains(&id) {
                built_at = Some(step);
                // Old age comes for a married citizen.
                let c = (0..town.citizens.len())
                    .find(|&c| town.citizens[c].alive && town.citizens[c].spouse.is_some())
                    .expect("no married citizen");
                widow = town.citizens[c].spouse;
                let age = town.citizens[c].age;
                town.citizens[c].lifespan = age;
            }
            if built_at.is_some() && !town.construction.tombstones.is_empty() {
                buried_at = Some(step);
                break;
            }
        }
        let graves = town.construction.sites[&id].graves.clone();
        eprintln!(
            "built {built_at:?}, buried {buried_at:?}, {} graves, tombstones {:?}",
            graves.len(),
            town.construction.tombstones
        );
        assert!(built_at.is_some(), "the cemetery was not built");
        assert!(!graves.is_empty(), "no graves laid out");
        assert!(buried_at.is_some(), "nobody was buried");
        let (&stone, _) = town.construction.tombstones.iter().next().unwrap();
        let p = &town.placed[stone];
        let cell = p.cell.1 * n + p.cell.0;
        assert!(graves.contains(&cell), "the tombstone is not on a grave");
        assert_eq!(town.cells.objects[cell as usize], Some(stone));
        let w = widow.unwrap();
        assert_ne!(
            town.agents.states[w].flags & 0x40,
            0,
            "the spouse knows of the burial"
        );
    }

    /// With accidents made near certain for builders, a builder's work step fails with an
    /// injury and the builder dies of it (code 7, the profession's death message).
    #[test]
    fn a_builder_dies_in_an_accident() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../game-data/original/WinData");
        let mut archive = Archive::default();
        for name in ["data1.pkg", "patch1.pkg", "patch0.pkg", "data0.pkg"] {
            let Ok(bytes) = std::fs::read(dir.join(name)) else {
                return;
            };
            archive.add(Package::parse(bytes).unwrap());
        }
        let world = crate::world::World::generate(
            &archive,
            &crate::world::WorldParams {
                seed: 1234,
                ..Default::default()
            },
        )
        .unwrap();
        let terrain = HeightGrid {
            width: world.terrain.width(),
            heights: &world.terrain.heights,
        };
        let mut rng = Random::new(1234);
        let mut town = StartingTown::create(
            &archive,
            "Game/StartCondition/Medium.rsc:resource",
            world.terrain.size,
            &terrain,
            &mut rng,
        )
        .unwrap();
        let builder = town.construction.professions.builder;
        let death = town.construction.professions.list[builder].death.clone();
        assert!(!death.is_empty(), "the builder has no death message");
        assert!(0.0 < town.construction.professions.list[builder].accident.1);
        town.construction.professions.list[builder].accident = (1e-6, 1.0);
        let mut tool = PlacementTool::start(
            &archive,
            &mut town,
            r"Template\WoodHouse.rsc:resource",
            &mut rng,
        )
        .unwrap();
        let spot = tool
            .find_clear_spot(&town, &terrain, town.start)
            .expect("no spot");
        tool.hover(&town, &terrain, Some(spot), 0);
        tool.place(&archive, &mut town, 0).unwrap();
        town.construction
            .commands
            .push(banished_game::professions::Command::SetCount {
                profession: builder,
                count: 2,
            });
        let mut warnings = banished_game::events::StockWarnings::default();
        let dt = 1.0 / 30.0;
        let mut events = Vec::new();
        for _ in 0..30 * 60 * 8 {
            town.apply_commands(&terrain, &mut rng);
            town.iterate(&terrain, &mut rng, dt, 50.0);
            let stock = std::array::from_fn(|i| (warnings.count[i], warnings.limits[i]));
            town.update_citizens(&terrain, &mut rng, dt, 50.0, stock);
            warnings.recompute(&town.registered_storages(), &town.materials);
            town.construction
                .jobs
                .step(banished_game::scheduler::ticks(dt));
            events.extend(std::mem::take(&mut town.agents.events));
            if events
                .iter()
                .any(|e| e.kind == banished_game::life::reason::ACCIDENT)
            {
                break;
            }
        }
        let e = events
            .iter()
            .find(|e| e.kind == banished_game::life::reason::ACCIDENT)
            .expect("no accident");
        assert_eq!(
            e.extra.as_ref().map(|(_, k)| k.as_str()),
            Some(death.as_str())
        );
        assert_eq!(
            town.agents.states[e.citizen].accident,
            Some(builder),
            "the accident's profession"
        );
    }

    /// A finished house nobody has moved into, holding food, gets a ClearStorage work and
    /// laborers carry the food to the town's storage.
    #[test]
    fn an_empty_house_is_cleared() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../game-data/original/WinData");
        let mut archive = Archive::default();
        for name in ["data1.pkg", "patch1.pkg", "patch0.pkg", "data0.pkg"] {
            let Ok(bytes) = std::fs::read(dir.join(name)) else {
                return;
            };
            archive.add(Package::parse(bytes).unwrap());
        }
        let world = crate::world::World::generate(
            &archive,
            &crate::world::WorldParams {
                seed: 1234,
                ..Default::default()
            },
        )
        .unwrap();
        let terrain = HeightGrid {
            width: world.terrain.width(),
            heights: &world.terrain.heights,
        };
        let mut rng = Random::new(1234);
        let mut town = StartingTown::create(
            &archive,
            "Game/StartCondition/Medium.rsc:resource",
            world.terrain.size,
            &terrain,
            &mut rng,
        )
        .unwrap();
        let mut tool = PlacementTool::start(
            &archive,
            &mut town,
            r"Template\WoodHouse.rsc:resource",
            &mut rng,
        )
        .unwrap();
        let spot = tool
            .find_clear_spot(&town, &terrain, town.start)
            .expect("no spot");
        tool.hover(&town, &terrain, Some(spot), 0);
        let id = tool.place(&archive, &mut town, 0).unwrap();
        let builder = town.construction.professions.builder;
        town.construction
            .commands
            .push(banished_game::professions::Command::SetCount {
                profession: builder,
                count: 2,
            });
        // Food the house can hold.
        let food = (0..town.materials.len())
            .find(|&m| banished_game::stock::stock_slot(town.materials[m].flags) == Some(5))
            .expect("no food material");
        let mut warnings = banished_game::events::StockWarnings::default();
        let dt = 1.0 / 30.0;
        let (mut stocked, mut registered, mut cleared) = (false, false, false);
        for _ in 0..30 * 60 * 15 {
            town.apply_commands(&terrain, &mut rng);
            town.iterate(&terrain, &mut rng, dt, 50.0);
            let stock = std::array::from_fn(|i| (warnings.count[i], warnings.limits[i]));
            town.update_citizens(&terrain, &mut rng, dt, 50.0, stock);
            warnings.recompute(&town.registered_storages(), &town.materials);
            town.construction
                .jobs
                .step(banished_game::scheduler::ticks(dt));
            let site = &town.construction.sites[&id];
            let storage = site.storage;
            if !stocked && site.complete && town.construction.homes.contains_key(&id) {
                stocked = true;
                // Nobody may live there while the test watches: the house leaves the
                // residences and anyone who moved in leaves it.
                town.construction.workforce.residences.retain(|&r| r != id);
                let home = town.construction.homes.get_mut(&id).unwrap();
                for c in home.families.iter_mut().flat_map(std::mem::take) {
                    town.agents.states[c].home = None;
                    town.agents.states[c].home_slot = -1;
                }
                town.storages[storage].add(food, 5);
            }
            if stocked && !registered {
                let w = town.construction.sites[&id].work_id;
                registered = matches!(
                    town.construction.jobs.works.get(&w).map(|w| &w.kind),
                    Some(banished_game::jobs::WorkKind::ClearStorage(_))
                );
            }
            if stocked && town.storages[storage].items.iter().all(|i| i.count == 0) {
                cleared = true;
                break;
            }
        }
        assert!(stocked, "the house was not finished empty");
        assert!(registered, "no ClearStorage work");
        assert!(cleared, "the food stayed in the house");
    }

    /// A market is built and its vendors stock it from the town's storages by category; it
    /// joins the town's markets with its radius.
    #[test]
    fn vendors_stock_a_market() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../game-data/original/WinData");
        let mut archive = Archive::default();
        for name in ["data1.pkg", "patch1.pkg", "patch0.pkg", "data0.pkg"] {
            let Ok(bytes) = std::fs::read(dir.join(name)) else {
                return;
            };
            archive.add(Package::parse(bytes).unwrap());
        }
        let world = crate::world::World::generate(
            &archive,
            &crate::world::WorldParams {
                seed: 1234,
                ..Default::default()
            },
        )
        .unwrap();
        let terrain = HeightGrid {
            width: world.terrain.width(),
            heights: &world.terrain.heights,
        };
        let mut rng = Random::new(1234);
        let mut town = StartingTown::create(
            &archive,
            "Game/StartCondition/Easy.rsc:resource",
            world.terrain.size,
            &terrain,
            &mut rng,
        )
        .unwrap();
        let mut tool = PlacementTool::start(
            &archive,
            &mut town,
            r"Template\Market.rsc:resource",
            &mut rng,
        )
        .unwrap();
        let spot = tool
            .find_clear_spot(&town, &terrain, town.start)
            .expect("no spot");
        tool.hover(&town, &terrain, Some(spot), 0);
        let id = tool.place(&archive, &mut town, 0).expect("not placed");
        let builder = town.construction.professions.builder;
        let vendor = town
            .construction
            .professions
            .list
            .iter()
            .position(|p| p.reference.to_ascii_lowercase().ends_with(":vendor"))
            .expect("no vendor profession");
        for (profession, count) in [(builder, 4), (vendor, 2)] {
            town.construction
                .commands
                .push(banished_game::professions::Command::SetCount { profession, count });
        }
        let mut warnings = banished_game::events::StockWarnings::default();
        let dt = 1.0 / 30.0;
        let (mut built_at, mut stocked_at) = (None, None);
        for step in 0..30 * 60 * 20 {
            town.apply_commands(&terrain, &mut rng);
            town.iterate(&terrain, &mut rng, dt, 50.0);
            let stock = std::array::from_fn(|i| (warnings.count[i], warnings.limits[i]));
            town.update_citizens(&terrain, &mut rng, dt, 50.0, stock);
            warnings.recompute(&town.registered_storages(), &town.materials);
            town.construction
                .jobs
                .step(banished_game::scheduler::ticks(dt));
            let site = &town.construction.sites[&id];
            if built_at.is_none() && site.complete {
                built_at = Some(step);
            }
            let storage = site.storage;
            if built_at.is_some() && town.storages[storage].items.iter().any(|i| i.count > 0) {
                stocked_at = Some(step);
                break;
            }
        }
        let market = town.storage_sites.iter().find(|s| s.entity == id);
        eprintln!(
            "built {built_at:?}, stocked {stocked_at:?}, market {:?}, items {:?}",
            market.map(|m| m.radius),
            town.storages[town.construction.sites[&id].storage].items
        );
        assert!(built_at.is_some(), "the market was not built");
        assert!(
            market.is_some_and(|m| m.radius.is_some()),
            "not a town market"
        );
        assert!(stocked_at.is_some(), "no goods reached the market");
    }
}
