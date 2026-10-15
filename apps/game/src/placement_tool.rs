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
                    let clear = (rect.y0..=rect.y1).all(|y| {
                        (rect.x0..=rect.x1)
                            .all(|x| town.cells.objects[(y * n + x) as usize].is_none())
                    });
                    if clear {
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
        assert!(town
            .storages
            .iter()
            .all(|s| s.items.iter().all(|e| e.reserved == 0)));
    }
}
