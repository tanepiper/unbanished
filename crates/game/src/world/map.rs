//! The gameplay map: `n × n` cells over the centre of the `2n` terrain (see docs/MAP-CELLS.md).

use crate::math::sin_cos_deg;
use crate::terrain::{CellType, HeightGrid};

/// The gameplay map: the type of each cell. See [`Map::find_start`] for the start of a new town.
pub struct Map {
    /// Cells per side (`n`).
    pub size: usize,
    // +0x29
    /// Cell type (byte) per cell, row-major.
    pub types: Vec<CellType>,
}

impl Map {
    /// Classify every cell from the terrain.
    ///
    /// Original: `FUN_1400a43f0`.
    pub fn new(size: usize, terrain: &HeightGrid) -> Map {
        Map {
            size,
            types: (0..size * size)
                .map(|i| terrain.natural_cell_type(size, i))
                .collect(),
        }
    }

    /// Every cell within `radius` (`dx² + dy² ≤ radius_sq`) of `(x, y)` is in the map and buildable
    /// ground.
    ///
    /// Original: `FUN_1400a7d50`.
    pub fn is_clear(&self, x: i32, y: i32, radius: i32, radius_sq: i32) -> bool {
        let n = self.size as i32;
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                if dx * dx + dy * dy > radius_sq {
                    continue;
                }
                let (cx, cy) = (x + dx, y + dy);
                if cx < 0 || cy < 0 || cx >= n || cy >= n {
                    return false;
                }
                if self.types[(cy * n + cx) as usize] != CellType::GROUND {
                    return false;
                }
            }
        }
        true
    }

    /// The town's start cell. Rings outward from the map centre, testing `4, 8, 16, …` evenly
    /// spaced angles per ring and moving out `3r/4` cells each ring. Loops forever (as the original
    /// does) if no clear area exists.
    ///
    /// Original: `FUN_1400a7bf0`.
    pub fn find_start(&self, radius: i32) -> (i32, i32) {
        let centre = self.size as i32 >> 1;
        let mut count = 4;
        let mut dist = 0.0f32;
        loop {
            for i in 0..count {
                let angle = i as f32 * (360.0 / count as f32);
                let (sin, cos) = sin_cos_deg(angle);
                let x = (cos * dist) as i32 + centre;
                let y = (sin * dist) as i32 + centre;
                if self.is_clear(x, y, radius, radius * radius) {
                    return (x, y);
                }
            }
            count *= 2;
            dist += ((radius * 3) / 4) as f32;
        }
    }
}
