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
use banished_game::nature::seeding::Placed;
use banished_game::random::Random;
use banished_game::terrain::HeightGrid;
use banished_game::world::placement::{
    can_place, footprint, footprint_cell, PlacementRules, Rect, ROTATION_STEP,
};

use crate::gfx::models::placement_matrix;
use crate::start::StartingTown;

fn read(archive: &Archive, reference: &str) -> Option<Vec<u8>> {
    archive.read_ref(&reference.replace('\\', "/")).ok()
}

pub struct PlacementTool {
    /// `Template\X.rsc:resource`.
    pub reference: String,
    template: usize,
    rules: PlacementRules,
    /// CreatePlacedDescription `+0x40`: the entrance's direction in degrees from the
    /// building's rotation, −1 for none (the overlay's arrows).
    entrance: i32,
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
    /// `+0x40`: the template's RadiusDescription (`{file}:radius`): its radius and material.
    radius: Option<(i32, String)>,
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
            entrance: placed.value,
            model,
            variant,
            rect: None,
            position: [0.0; 3],
            valid: false,
            toolbar,
            build,
            bounds,
            radius: read(archive, &format!("{file}:radius"))
                .and_then(|d| banished_formats::game::radius_description(&d).ok())
                .and_then(|(r, m)| Some((r, m?))),
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

    /// `FUN_1401033b0` → `FUN_140055d90`: with a RadiusDescription, its ring about the
    /// footprint's centre cell (`x0 + (x1 − x0) / 2`, likewise y): cell, radius, material.
    pub fn radius_decal(&self) -> Option<((i32, i32), i32, &str)> {
        let (r, material) = self.radius.as_ref()?;
        let rect = self.rect?;
        let cell = (
            ((rect.x1 - rect.x0) >> 1) + rect.x0,
            ((rect.y1 - rect.y0) >> 1) + rect.y0,
        );
        Some((cell, *r, material.as_str()))
    }

    /// `FUN_140103190` → `FUN_140106850`: the footprint's overlay. Each cell is green where it
    /// admits the building and red where not (yellow for the tutorial's target, `restrict`);
    /// a `.` cell (the ground the entrance opens onto) shows the arrow (frame 2) turned the
    /// entrance's way, or frame 1 for a building without one.
    pub fn overlay(
        &self,
        town: &StartingTown,
        terrain: &HeightGrid,
        rect: Rect,
        degrees: i32,
        restrict: bool,
    ) -> Vec<crate::gfx::selection_overlay::OverlayCell> {
        let mut out = Vec::new();
        for y in rect.y0..=rect.y1 {
            for x in rect.x0..=rect.x1 {
                let (admits, untouched) = footprint_cell(
                    &town.cells,
                    terrain,
                    &self.rules,
                    rect,
                    degrees,
                    x,
                    y,
                    |_| false,
                );
                let argb = match (restrict, admits) {
                    (true, _) => 0xffff_ff00,
                    (false, true) => 0xff00_ff00,
                    (false, false) => 0xffff_0000,
                };
                let frame = match (untouched, self.entrance >= 0) {
                    (false, _) => 0,
                    (true, true) => 2,
                    (true, false) => 1,
                };
                out.push(crate::gfx::selection_overlay::OverlayCell {
                    x,
                    y,
                    frame,
                    argb,
                    rotation: degrees + self.entrance,
                });
            }
        }
        out
    }

    /// Footprint size in cells.
    /// The cells the ghost covers.
    pub fn rect(&self) -> Option<Rect> {
        self.rect
    }

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

    /// The building a click would create (`FUN_140102b20`): the tool's footprint and ghost if
    /// they are valid, turned by `rotation`; none otherwise.
    pub fn placement(&self, rotation: i32) -> Option<Placement> {
        let rect = self.rect.filter(|_| self.valid)?;
        Some(Placement {
            template: self.template,
            rect,
            position: self.position,
            degrees: (rotation * ROTATION_STEP as i32).rem_euclid(360),
            variant: self.variant,
            rules: self.rules.clone(),
        })
    }

    /// `FUN_140102b20`: create the building if the footprint is valid ([`place`]), then the
    /// ghost shows the next variant. Returns the new entity. (The game goes through
    /// `PlayerCommand::Place`.)
    #[cfg(test)]
    pub fn place(
        &mut self,
        archive: &Archive,
        town: &mut StartingTown,
        rotation: i32,
    ) -> Option<usize> {
        let id = place(archive, town, &self.placement(rotation)?);
        self.placed();
        Some(id)
    }

    /// After a placement: the ghost shows the next variant.
    pub fn placed(&mut self) {
        self.next_variant();
    }
}

/// A building to create: what a placement click hands over (`PlayerCommand::Place`).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Placement {
    /// The template.
    pub template: usize,
    /// The footprint's cells.
    pub rect: Rect,
    /// The model's position.
    pub position: [f32; 3],
    /// The rotation in degrees (0, 90, 180 or 270).
    pub degrees: i32,
    /// The model variant.
    pub variant: usize,
    /// The template's placement rules (the zone it reserves).
    pub rules: PlacementRules,
}

/// `FUN_140102b20`: create the building. It has no mesh until its BuildComponent starts
/// building (`FUN_14010cfb0` shows mesh 0, the first stage). Its ZoneComponent reserves the
/// footprint now (`FUN_1400a2830`); the map is stamped when the building is finished. Returns
/// the new entity.
pub fn place(archive: &Archive, town: &mut StartingTown, p: &Placement) -> usize {
    let rect = p.rect;
    let id = town.placed.len();
    town.placed.push(Placed {
        template: p.template,
        bucket: town.templates[p.template].bucket,
        cell: (rect.x0, rect.y0),
        footprint: (rect.width(), rect.height()),
        position: p.position,
        yaw: p.degrees as f32,
        variant: p.variant,
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
    banished_game::world::placement::zone(&mut town.cells, &p.rules, rect, p.degrees, id);
    town.add_site(archive, id);
    id
}

#[cfg(test)]
mod construction_tests;
