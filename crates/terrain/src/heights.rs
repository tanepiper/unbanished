//! A map's heights, owned by the game state (not in the original).

use crate::HeightGrid;

/// A map's heights, owned, as the game state holds them (not in the original, which keeps them
/// in its terrain): `(size + 1)²` values, row by row, height × 256. [`Heights::grid`] lends them
/// as a [`HeightGrid`] for queries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Heights {
    /// The cells on each side.
    pub size: usize,
    /// The heights, height × 256.
    pub values: Vec<i16>,
}

impl Heights {
    /// The number of samples on each side, `size + 1`.
    pub fn width(&self) -> usize {
        self.size + 1
    }

    /// The heights as a [`HeightGrid`].
    pub fn grid(&self) -> HeightGrid<'_> {
        HeightGrid {
            width: self.width(),
            heights: &self.values,
        }
    }
}
