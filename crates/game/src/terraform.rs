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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Terraform {
    Dig(usize),
    Undo(usize),
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
    let (s, c) = yaw.to_radians().sin_cos();
    let mut changed = false;
    for v in vertices {
        let q = [
            c * v[0] - s * v[1] + position[0] + 0.25,
            s * v[0] + c * v[1] + position[1] + 0.25,
            v[2] + position[2],
        ];
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
