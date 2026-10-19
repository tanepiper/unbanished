//! Quarries and mines: the pit deepens as it is worked. See docs/QUARRY.md.
//!
//! - Entity event 4 (the build's end): the pit is dug to the description's shallow floor.
//! - Its message, which each produced batch sends from the ConsumeProduce sequence (the
//!   description's component type): the pit is dug to `lerp(shallow, deep, produced / lifetime)`,
//!   then every cell of the map rectangle whose terrain is steep at any of four points (normal `z <
//!   0.85`) becomes the quarry's (type 5, object the quarry).
//!
//! A dig: every vertex of the pit's picking mesh, placed by the model's transform,
//! sets the height under it to the vertex's height, no lower than the floor. The DecalComponent is
//! re-added around each dig (the port's decals follow the terrain).
//!
//! Original: `QuarryComponent`: `FUN_14010cad0`, `FUN_140055a50`, `FUN_1400554e0`, `FUN_1400c4440`;
//! the dig is `FUN_140055320`.
// vtable 0x1401605b0; slot +0x60; slot +0x78; +0x38

use std::sync::Arc;

use crate::nature::seeding::{Cells, Placed};
use crate::terrain::{CellType, HeightGrid};

/// A resolved `QuarryDescription`.
#[derive(Debug, Clone, PartialEq)]
pub struct QuarryTemplate {
    // +0x28, +0x2c
    /// The floor when built and at the end of production.
    pub shallow: f32,
    pub deep: f32,
    // +0x20
    /// The pit's picking mesh vertices (model space).
    pub vertices: Arc<Vec<[f32; 3]>>,
}

/// `DAT_140178930`.
const HEIGHT_SCALE: f32 = 256.0;
/// The cell type a steep quarry cell becomes.
pub const QUARRY_CELL: CellType = CellType(5);

/// The pit under the quarry `p`, no lower than `floor`. `size` is the map's cells per side.
///
/// Original: `FUN_140055320`.
pub fn dig(heights: &mut [i16], width: usize, q: &QuarryTemplate, p: &Placed, floor: f32) -> bool {
    let half = ((width - 1) >> 1) as f32;
    let model = crate::math::Transform::of_model(p.yaw, p.position);
    let mut changed = false;
    for &v in q.vertices.iter() {
        // The entity transform (`+0x148`) times the vertex.
        let [x, y, z] = model.apply(v);
        let z = (z + 0.0).max(floor);
        let (gx, gy) = ((x + 0.25 + half) as i32, (y + 0.25 + half) as i32);
        if gx < 0 || gy < 0 || width as i32 <= gx || width as i32 <= gy {
            continue;
        }
        let h = (z * HEIGHT_SCALE) as i32 as i16;
        let cell = &mut heights[gy as usize * width + gx as usize];
        changed |= *cell != h;
        *cell = h;
    }
    changed
}

/// The message's second half: the quarry's steep cells become its own.
///
/// Original: `FUN_1400554e0`.
pub fn claim_steep_cells(cells: &mut Cells, terrain: &HeightGrid, entity: usize, p: &Placed) {
    let n = cells.size as i32;
    let half = (n >> 1) as f32 - 0.5;
    let (x0, y0) = p.cell;
    let (w, h) = p.footprint;
    for y in y0..y0 + h {
        for x in x0..x0 + w {
            if x < 0 || y < 0 || n <= x || n <= y {
                continue;
            }
            let (cx, cy) = (x as f32 - half, y as f32 - half);
            for (dx, dy) in [(-0.25, -0.25), (-0.25, 0.25), (0.25, -0.25), (0.25, 0.25)] {
                // The cell centre's ground (`FUN_1400a48b0` keeps x and y) a quarter on, the
                // terrain cell under it cast from 100 above, at most 200 (`FUN_1400afef0`);
                // no hit leaves the normal zero, which claims the cell.
                let normal = terrain.normal_below(cx + dx, cy + dy, 100.0, 200.0);
                if normal[2] < 0.85 {
                    let i = (y * n + x) as usize;
                    cells.set_type(i, QUARRY_CELL);
                    cells.objects[i] = Some(entity);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn placed() -> Placed {
        Placed {
            template: 0,
            bucket: 0,
            cell: (2, 2),
            footprint: (4, 4),
            position: [0.0, 0.0, 0.0],
            yaw: 0.0,
            variant: 0,
            mesh: 0,
            growth: 0.0,
            growth_state: None,
            spawn_state: None,
            alive: true,
            removal_requested: false,
            animation: None,
            scale: [1.0; 4],
        }
    }

    /// The pit follows the mesh down to the floor; a deeper floor lets it follow further,
    /// and the cells over its sides become the quarry's.
    #[test]
    fn the_pit_deepens_to_its_floor_and_claims_steep_cells() {
        let q = QuarryTemplate {
            shallow: -1.0,
            deep: -3.0,
            vertices: Arc::new(vec![[0.0, 0.0, -5.0], [1.0, 0.0, -0.5], [2.0, 0.0, 0.0]]),
        };
        let width = 17;
        let mut heights = vec![0i16; width * width];
        let p = placed();
        assert!(dig(&mut heights, width, &q, &p, q.shallow));
        let at = |h: &[i16], x: usize| h[8 * width + 8 + x];
        assert_eq!(
            (at(&heights, 0), at(&heights, 1), at(&heights, 2)),
            (-256, -128, 0)
        );
        assert!(dig(&mut heights, width, &q, &p, q.deep));
        assert_eq!(at(&heights, 0), -768);
        let mut cells = Cells::new(8, vec![CellType::GROUND; 64]);
        let grid = HeightGrid {
            width,
            heights: &heights,
        };
        claim_steep_cells(&mut cells, &grid, 7, &p);
        let n = 8;
        let claimed: Vec<usize> = (0..n * n)
            .filter(|&i| cells.types[i] == QUARRY_CELL)
            .collect();
        assert!(!claimed.is_empty());
        assert!(claimed.iter().all(|&i| cells.objects[i] == Some(7)));
    }
}
