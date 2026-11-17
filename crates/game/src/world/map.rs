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

    /// A start point at least `away` cells from every point in `taken`, searched as
    /// [`Map::find_start`] does. A separate towns game gives each player one. When no clear
    /// point is far enough, the one farthest from the taken points is used, so a small map
    /// still never gives two players the same start.
    pub fn find_start_away(&self, radius: i32, taken: &[(i32, i32)], away: f32) -> (i32, i32) {
        let centre = self.size as i32 >> 1;
        let mut count = 4;
        let mut dist = 0.0f32;
        // The clearest point seen, and its distance to the nearest taken one.
        let mut best: Option<(f32, (i32, i32))> = None;
        while dist <= self.size as f32 {
            for i in 0..count {
                let angle = i as f32 * (360.0 / count as f32);
                let (sin, cos) = sin_cos_deg(angle);
                let x = (cos * dist) as i32 + centre;
                let y = (sin * dist) as i32 + centre;
                if !self.is_clear(x, y, radius, radius * radius) {
                    continue;
                }
                let nearest = taken.iter().fold(f32::INFINITY, |near, &(tx, ty)| {
                    let (dx, dy) = ((x - tx) as f32, (y - ty) as f32);
                    near.min(dx * dx + dy * dy)
                });
                if nearest >= away * away {
                    return (x, y);
                }
                if best.is_none_or(|(b, _)| nearest > b) {
                    best = Some((nearest, (x, y)));
                }
            }
            count = (count * 2).min(256);
            dist += ((radius * 3) / 4) as f32;
        }
        best.map_or_else(|| self.find_start(radius), |(_, point)| point)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ground(size: usize) -> Map {
        Map {
            size,
            types: vec![CellType::GROUND; size * size],
        }
    }

    /// A point far enough is used; when the map has none, the clearest point still differs from
    /// the first town's.
    #[test]
    fn a_start_away_is_a_start_of_its_own() {
        let map = ground(32);
        let first = map.find_start(2);
        let other = map.find_start_away(2, &[first], 10.0);
        let (dx, dy) = ((other.0 - first.0) as f32, (other.1 - first.1) as f32);
        assert!(
            dx * dx + dy * dy >= 100.0,
            "{other:?} is far from {first:?}"
        );

        let small = ground(8);
        let first = small.find_start(2);
        let other = small.find_start_away(2, &[first], 1000.0);
        assert_ne!(other, first);
        assert!(small.is_clear(other.0, other.1, 2, 4), "{other:?}");
    }
}
