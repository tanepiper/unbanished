//! FloorComponent (vtable `0x140169810`'s description, FloorDescription): a bridge part's deck.
//! Activated (`FUN_140124810` → `FUN_140124860`) it counts one more on every cell of its
//! MapComponent (cell `+0x2d`) and adds its picking mesh to the map's floor scene; removed
//! (`FUN_140124770` → `FUN_1400a2070`) it takes both back. `FUN_1400a48b0` (the ground under a
//! point) stands on the floor where a cell has one and the floor is above the terrain.
//!
//! Deviation: only the citizens' walking height (`character::World`) asks the floors; the
//! other ground queries (`town::cell_ground` and friends: piles, work points) stay on the
//! terrain, so goods dropped on a bridge lie on the river bed.

use std::collections::BTreeMap;

use crate::terrain::HeightGrid;

/// `FUN_1400a48b0`'s floor ray: from 50 above, straight down, at most 100 long.
const RAY_TOP: f32 = 50.0;
const RAY_LENGTH: f32 = 100.0;

/// The map's floors: per cell how many floors cover it (`+0x2d`) and every floor's triangles
/// in map space (the map's picking scene, map `+0x08`).
#[derive(Debug, Clone, Default)]
pub struct Floors {
    pub counts: Vec<u8>,
    pub meshes: BTreeMap<usize, Floor>,
}

#[derive(Debug, Clone, Default)]
pub struct Floor {
    /// The MapComponent's rectangle (`x, y, w, h`) the floor counts on.
    pub rect: (i32, i32, i32, i32),
    pub triangles: Vec<[[f32; 3]; 3]>,
}

impl Floor {
    /// A floor mesh (model space) turned and placed as the entity's model, over its
    /// MapComponent's cells (`FUN_140124860`: the model transform and map rectangle).
    pub fn placed(mesh: &[[[f32; 3]; 3]], p: &crate::seeding::Placed) -> Floor {
        let (s, c) = p.yaw.to_radians().sin_cos();
        let at = p.position;
        let place = |v: [f32; 3]| {
            [
                c * v[0] - s * v[1] + at[0],
                s * v[0] + c * v[1] + at[1],
                v[2] + at[2],
            ]
        };
        Floor {
            rect: (p.cell.0, p.cell.1, p.footprint.0, p.footprint.1),
            triangles: mesh.iter().map(|t| t.map(place)).collect(),
        }
    }
}

impl Floors {
    pub fn new(cells: usize) -> Floors {
        Floors {
            counts: vec![0; cells],
            meshes: BTreeMap::new(),
        }
    }

    /// `FUN_140124860`: a floor that was added goes first; every cell of the rectangle counts
    /// one more and the mesh joins the scene.
    pub fn add(&mut self, size: usize, entity: usize, floor: Floor) {
        self.remove(size, entity);
        self.count(size, floor.rect, 1);
        self.meshes.insert(entity, floor);
    }

    /// `FUN_140124770` → `FUN_1400a2070`: the rectangle counts one less and the mesh leaves.
    pub fn remove(&mut self, size: usize, entity: usize) {
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

    /// `FUN_1400a48b0(point, skip floors = false)`: the terrain under `(x, y)`; on a cell with a
    /// floor, the nearest floor hit by a ray from 50 above, when it is above the terrain.
    pub fn ground(&self, size: usize, terrain: &HeightGrid, x: f32, y: f32) -> f32 {
        let mut normal = [0.0; 3];
        let ground = terrain.ground(x, y, &mut normal);
        let n = size as i32;
        let half = (n >> 1) as f32;
        let cx = ((x + half) as i32).clamp(0, n - 1);
        let cy = ((y + half) as i32).clamp(0, n - 1);
        if self
            .counts
            .get((cy * n + cx) as usize)
            .copied()
            .unwrap_or(0) as i8
            <= 0
        {
            return ground;
        }
        let origin = [x, y, RAY_TOP];
        let direction = [0.0, 0.0, -1.0];
        let mut nearest = RAY_LENGTH;
        let mut hit = None;
        for floor in self.meshes.values() {
            for &t in &floor.triangles {
                if let Some(d) = crate::picking::triangle(origin, direction, t, nearest) {
                    nearest = d;
                    hit = Some(d);
                }
            }
        }
        match hit {
            Some(d) if ground < RAY_TOP - d => RAY_TOP - d,
            _ => ground,
        }
    }
}
