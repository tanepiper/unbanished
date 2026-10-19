//! Floors: the deck of a bridge part. When it is activated, it counts one more floor on every cell
//! under it and adds its picking mesh to the map's floor scene; when it is removed, it takes both
//! back. The ground under a point is on the floor where a cell has one and the floor is above the
//! terrain.
//!
//! The ground queries ask the floors as the original's do ([`crate::town::floor_ground`]): the
//! characters' walking height, piles, storage drop points, work points, needs, boats' routes,
//! seeding, fences, bridges and the cell checks. Only the tornado and the natural cell types skip
//! them, as here.
//!
//! Original: `FloorComponent`: `FUN_140124810`, `FUN_140124860`, `FUN_140124770`, `FUN_1400a2070`;
//! the ground under a point `FUN_1400a48b0`; the callers that skip floors `FUN_140058630`,
//! `FUN_1400a3790`.
// vtable 0x140169810
// +0x2d

use std::collections::BTreeMap;

use crate::terrain::HeightGrid;

/// `FUN_1400a48b0`'s floor ray: from 50 above, straight down, at most 100 long.
const RAY_TOP: f32 = 50.0;
const RAY_LENGTH: f32 = 100.0;

// +0x2d; +0x08
/// The map's floors: per cell how many floors cover it and every floor's mesh with its transform
/// (the map's picking scene, map).
#[derive(Debug, Clone, Default)]
pub struct Floors {
    /// How many floors cover each cell.
    pub counts: Vec<u8>,
    /// Every floor's mesh with its transform, by entity.
    pub meshes: BTreeMap<usize, Floor>,
    /// Each floor's world-space footprint (`x0, y0, x1, y1`, widened by [`BOUNDS_MARGIN`]),
    /// set by [`Floors::add`]. The downward ray can hit a floor only inside it, so the ground
    /// query skips the others without changing its result. A floor without bounds is tested.
    bounds: BTreeMap<usize, [f32; 4]>,
}

/// Slack around a floor's footprint, far above the float error of turning its triangles into
/// world space.
const BOUNDS_MARGIN: f32 = 0.05;

/// A bridge's floor: its triangles and the cells it covers.
#[derive(Debug, Clone, Default)]
pub struct Floor {
    /// The MapComponent's rectangle (`x, y, w, h`) the floor counts on.
    pub rect: (i32, i32, i32, i32),
    // +0x148
    /// The entity transform AddMesh keeps for the mesh's root (every shipped floor mesh is one root
    /// submesh).
    pub transform: crate::math::Transform,
    /// Model space: `Picking::Scene::Intersect` takes the ray into the mesh's space.
    pub triangles: Vec<[[f32; 3]; 3]>,
}

impl Floor {
    /// A floor mesh placed as the entity's model (its transform), over the entity's map cells.
    ///
    /// Original: `FUN_140124860`.
    pub fn placed(mesh: &[[[f32; 3]; 3]], p: &crate::nature::seeding::Placed) -> Floor {
        Floor {
            rect: (p.cell.0, p.cell.1, p.footprint.0, p.footprint.1),
            transform: crate::math::Transform::of_model(p.yaw, p.position),
            triangles: mesh.to_vec(),
        }
    }
}

impl Floors {
    /// No floors, for a map of `cells` cells.
    pub fn new(cells: usize) -> Floors {
        Floors {
            counts: vec![0; cells],
            meshes: BTreeMap::new(),
            bounds: BTreeMap::new(),
        }
    }

    /// A floor that was added goes first; every cell of the rectangle counts one more and the mesh
    /// joins the scene.
    ///
    /// Original: `FUN_140124860`.
    pub fn add(&mut self, size: usize, entity: usize, floor: Floor) {
        self.remove(size, entity);
        self.count(size, floor.rect, 1);
        let mut b = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
        for v in floor.triangles.iter().flatten() {
            let w = floor.transform.apply(*v);
            b = [
                b[0].min(w[0]),
                b[1].min(w[1]),
                b[2].max(w[0]),
                b[3].max(w[1]),
            ];
        }
        let m = BOUNDS_MARGIN;
        self.bounds
            .insert(entity, [b[0] - m, b[1] - m, b[2] + m, b[3] + m]);
        self.meshes.insert(entity, floor);
    }

    /// The rectangle counts one less and the mesh leaves.
    ///
    /// Original: `FUN_140124770`, `FUN_1400a2070`.
    pub fn remove(&mut self, size: usize, entity: usize) {
        self.bounds.remove(&entity);
        if let Some(floor) = self.meshes.remove(&entity) {
            self.count(size, floor.rect, -1);
        }
    }

    fn count(&mut self, size: usize, rect: (i32, i32, i32, i32), by: i8) {
        let n = size as i32;
        let (x0, y0, w, h) = rect;
        for y in y0..y0 + h {
            for x in x0..x0 + w {
                if let Some(c) = self.counts.get_mut((y * n + x) as usize) {
                    *c = c.wrapping_add_signed(by);
                }
            }
        }
    }

    /// The ground under `(x, y)`: the terrain, or, on a cell with a floor, the nearest floor that
    /// a ray from 50 above hits, when it is above the terrain.
    ///
    /// Original: `FUN_1400a48b0(point, skip floors = false)`.
    pub fn ground(&self, size: usize, terrain: &HeightGrid, x: f32, y: f32) -> f32 {
        self.ground_normal(size, terrain, x, y, &mut [0.0; 3])
    }

    /// As [`Floors::ground`], with the normal there (`param_5`): the terrain's, or on a floor
    /// the hit triangle's turned back into the map (`Intersect`'s last accepted hit).
    pub fn ground_normal(
        &self,
        size: usize,
        terrain: &HeightGrid,
        x: f32,
        y: f32,
        normal: &mut [f32; 3],
    ) -> f32 {
        let mut terrain_normal = [0.0; 3];
        let ground = terrain.ground(x, y, &mut terrain_normal);
        let n = size as i32;
        let half = (n >> 1) as f32;
        let cx = ((x + half) as i32).clamp(0, n - 1);
        let cy = ((y + half) as i32).clamp(0, n - 1);
        if self
            .counts
            .get((cy * n + cx) as usize)
            .copied()
            .unwrap_or(0) as i8
            > 0
        {
            let origin = [x, y, RAY_TOP];
            let direction = [0.0, 0.0, -1.0];
            let mut nearest = RAY_LENGTH;
            let mut hit = None;
            for (id, floor) in &self.meshes {
                if let Some(b) = self.bounds.get(id) {
                    if x < b[0] || y < b[1] || b[2] < x || b[3] < y {
                        continue;
                    }
                }
                // `Picking::Scene::Intersect`: the ray in the mesh's space
                // (`Transform::Inverse`); the distance holds, the rotation keeping the
                // direction's length.
                let inverse = floor.transform.inverse();
                let (o, dir) = (inverse.apply(origin), inverse.rotate(direction));
                for &t in &floor.triangles {
                    if let Some(d) = crate::picking::triangle(o, dir, t, nearest) {
                        nearest = d;
                        hit = Some((d, floor.transform.rotate(crate::picking::face_normal(t))));
                    }
                }
            }
            if let Some((d, n)) = hit {
                let z = crate::picking::point(origin, direction, d)[2];
                if ground < z {
                    *normal = n;
                    return z;
                }
            }
        }
        *normal = terrain_normal;
        ground
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A deck 3 above flat terrain over one cell: that cell's ground is the deck, its
    /// neighbour's the terrain, and a floor below the terrain is ignored.
    #[test]
    fn the_ground_stands_on_a_floor_above_the_terrain() {
        let size = 8;
        let heights = vec![0i16; (size + 1) * (size + 1)];
        let terrain = HeightGrid {
            width: size + 1,
            heights: &heights,
        };
        let mut floors = Floors::new(size * size);
        // Cell (4, 4) spans −0..1 around the map centre: x, y in [0, 1].
        let deck = |z: f32| Floor {
            rect: (4, 4, 1, 1),
            transform: crate::math::Transform::of_model(0.0, [0.0; 3]),
            triangles: vec![
                [[0.0, 0.0, z], [1.0, 0.0, z], [1.0, 1.0, z]],
                [[0.0, 0.0, z], [1.0, 1.0, z], [0.0, 1.0, z]],
            ],
        };
        floors.add(size, 0, deck(3.0));
        let on = crate::town::floor_ground(&floors, size, &terrain, 4 * 8 + 4);
        let off = crate::town::floor_ground(&floors, size, &terrain, 4 * 8 + 3);
        assert!((on[2] - 3.0).abs() < 1e-4, "{on:?}");
        assert!(off[2].abs() < 1e-4, "{off:?}");
        floors.add(size, 0, deck(-3.0));
        let below = crate::town::floor_ground(&floors, size, &terrain, 4 * 8 + 4);
        assert!(below[2].abs() < 1e-4, "{below:?}");
    }
}
