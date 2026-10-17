//! TerraformComponent (vtable `0x1401609a0`, TerraformDescription): a tunnel reshapes the
//! terrain when it is built. Entity event 4 (the build is done; a MultiObject passes it to its
//! parts) runs slot `+0x60` (`FUN_140057c70`): the ceiling mesh's vertices raise the terrain
//! where it is below them, keeping what was there (`FUN_1400574b0(1)`); the floor mesh's lower
//! it where it is above them (`FUN_1400574b0(0)`); then rough cells (10) under the
//! MapComponent become high ground (6) (`FUN_140057a80`). Removed (slot `+0x38`,
//! `FUN_140057cb0`), the raised heights are put back and those cells become rough again; the
//! dug floor stays. See docs/TUNNELS.md.
//!
//! Deviation: the heights belong to the terrain outside the simulation, so the town queues the
//! dig and the game applies it after the iteration (`StartingTown::apply_terraforms`), not
//! during it.

use std::sync::Arc;

use crate::seeding::Cells;
use crate::terrain::{CellType, HeightGrid};

/// The description's meshes as the first submesh's vertices (`PickingMesh::Vertices(0)`).
#[derive(Debug, Clone, Default)]
pub struct TerraformTemplate {
    pub floor: Option<Arc<Vec<[f32; 3]>>>,
    pub floor_mask: u32,
    pub ceiling: Option<Arc<Vec<[f32; 3]>>>,
    pub ceiling_mask: u32,
}

/// The component's `+0x28` raised points (where the ground was) and `+0x38` cells turned high.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TerraformState {
    pub raised: Vec<[f32; 3]>,
    pub cells: Vec<i32>,
}

/// A request for the terrain owner: dig (event 4) or undo (removal) an entity's terraform.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Terraform {
    Dig(usize),
    Undo(usize),
    /// A quarry's dig to `floor` (`crate::quarry`), claiming its steep cells with `claim`.
    Quarry {
        entity: usize,
        floor: f32,
        claim: bool,
    },
}

/// `FUN_140178930`: heights are stored × 256.
const HEIGHT_SCALE: f32 = 256.0;

fn cell_type(cells: &Cells, x: f32, y: f32) -> u8 {
    let n = cells.size as i32;
    let half = (n >> 1) as f32;
    let cx = ((x + half) as i32).clamp(0, n - 1);
    let cy = ((y + half) as i32).clamp(0, n - 1);
    cells.types[(cy * n + cx) as usize].0
}

/// `FUN_1400574b0(ceiling)`: each vertex through the model's transform (yaw, position) and a
/// quarter cell on; where the cells at it, one west, one south-west and (again) one west are
/// all of the mask's types, and the vertex is above (ceiling) or below (floor) the ground, the
/// height sample under it takes the vertex's height. A ceiling keeps the ground it raised.
#[allow(clippy::too_many_arguments)]
pub fn reshape(
    cells: &Cells,
    floors: &crate::floor::Floors,
    heights: &mut [i16],
    width: usize,
    vertices: &[[f32; 3]],
    mask: u32,
    yaw: f32,
    position: [f32; 3],
    ceiling: bool,
    raised: &mut Vec<[f32; 3]>,
) -> bool {
    let model = crate::math::Transform::of_model(yaw, position);
    let mut changed = false;
    for &v in vertices {
        // The entity transform (`+0x148`) times the vertex, a quarter cell on.
        let t = model.apply(v);
        let q = [t[0] + 0.25, t[1] + 0.25, t[2] + 0.0];
        let ground = {
            let grid = HeightGrid { width, heights };
            floors.ground(cells.size, &grid, q[0], q[1])
        };
        let types = [
            cell_type(cells, q[0], q[1]),
            cell_type(cells, q[0] - 1.0, q[1]),
            cell_type(cells, q[0] - 1.0, q[1] - 1.0),
            cell_type(cells, q[0] - 1.0, q[1]),
        ];
        let bits = types.iter().fold(0u32, |b, &t| b | 1 << (t & 0x1f));
        if !mask & bits != 0 {
            continue;
        }
        let moves = if ceiling {
            ground < q[2]
        } else {
            q[2] < ground
        };
        if !moves {
            continue;
        }
        let half = (width as i32 >> 1) as f32;
        let (hx, hy) = ((q[0] + half) as i32, (q[1] + half) as i32);
        if hx < 0 || hy < 0 || width as i32 <= hx || width as i32 <= hy {
            continue;
        }
        heights[hy as usize * width + hx as usize] = (q[2] * HEIGHT_SCALE) as i32 as i16;
        changed = true;
        if ceiling {
            raised.push([q[0], q[1], ground]);
        }
    }
    changed
}

/// `FUN_140057a80`: rough cells (10) of the rectangle become high ground (6), kept.
pub fn harden(cells: &mut Cells, rect: (i32, i32, i32, i32), kept: &mut Vec<i32>) {
    let n = cells.size as i32;
    let (x0, y0, w, h) = rect;
    for y in y0..y0 + h {
        for x in x0..x0 + w {
            let i = n * y + x;
            if cells.types[i as usize] == CellType::ROUGH {
                cells.types[i as usize] = CellType::HIGH;
                kept.push(i);
            }
        }
    }
}

/// `FUN_140057cb0`: the raised samples get their ground back and the kept cells become rough.
pub fn undo(cells: &mut Cells, heights: &mut [i16], width: usize, state: &TerraformState) -> bool {
    let half = (width as i32 >> 1) as f32;
    for p in &state.raised {
        let (hx, hy) = ((p[0] + half) as i32, (p[1] + half) as i32);
        if 0 <= hx && 0 <= hy && hx < width as i32 && hy < width as i32 {
            heights[hy as usize * width + hx as usize] = (p[2] * HEIGHT_SCALE) as i32 as i16;
        }
    }
    for &i in &state.cells {
        cells.types[i as usize] = CellType::ROUGH;
    }
    !state.raised.is_empty()
}

/// `FUN_1400a35b0`: each cell of the inclusive rectangle `x0, y0, x1, y1` sets its low
/// corner's height to 0 (the ground point of the cell's middle, in the height grid). `cells`
/// is the map's cells per side; the height grid is `width − 1` cells, the map in its middle.
pub fn flatten(heights: &mut [i16], width: usize, cells: usize, rect: [i32; 4]) -> bool {
    let off = ((width as i32 - 1) - cells as i32) >> 1;
    let [x0, y0, x1, y1] = rect;
    let mut changed = false;
    for y in y0..=y1 {
        for x in x0..=x1 {
            let (gx, gy) = (x + off, y + off);
            if gx < 0 || gy < 0 || width as i32 <= gx || width as i32 <= gy {
                continue;
            }
            let h = &mut heights[gy as usize * width + gx as usize];
            changed |= *h != 0;
            *h = 0;
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::flatten;

    #[test]
    fn flattening_zeroes_the_cells_low_corners() {
        // 4 map cells in a grid of 8 cells (9 points a side): the map starts at point 2.
        let width = 9;
        let mut heights = vec![100i16; width * width];
        assert!(flatten(&mut heights, width, 4, [1, 1, 2, 2]));
        for y in 0..width {
            for x in 0..width {
                let zero = (3..=4).contains(&x) && (3..=4).contains(&y);
                assert_eq!(heights[y * width + x] == 0, zero, "({x}, {y})");
            }
        }
        assert!(!flatten(&mut heights, width, 4, [1, 1, 2, 2]));
    }
}
