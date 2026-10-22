//! The map blocks of a save: the heights and the cells.

/// The map component's terrain grid.
///
/// Original: `FUN_1400b59a0`.
#[derive(Debug, Clone)]
pub struct MapHeights {
    /// Samples per side (`2n + 1`).
    pub width: usize,
    /// The heights, row by row: height × 256.
    pub heights: Vec<i16>,
}

/// One cell of the gameplay map, as a save stores it (the cell is 48 bytes in memory).
///
/// Original: `FUN_14001b570`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MapCell {
    // +0x00; +0x08
    /// Whether the two object pointers are set.
    pub objects: [bool; 2],
    // +0x10
    /// External reference id at (`u32::MAX` when unset).
    pub external: u32,
    // +0x1c..+0x25
    /// Bytes.
    pub flags: [u8; 9],
    // +0x26
    /// Not identified yet.
    pub value: u16,
    // +0x29; +0x28..+0x2d
    /// Bytes; `[1]` is the cell type.
    pub bytes: [u8; 6],
}

/// The gameplay map (serializer version 4): `size × size` cells covering the centre of the terrain
/// (the terrain grid is twice as wide).
///
/// Original: `FUN_1400a45d0`.
#[derive(Debug, Clone)]
pub struct MapCells {
    /// The number of cells on each side.
    pub size: usize,
    /// The cells, row by row.
    pub cells: Vec<MapCell>,
    // +0x90
    /// The nine values that end the map block.
    pub trailer: [i16; 9],
}

impl MapCell {
    /// The cell's type (see `unbanished_terrain::CellType`).
    pub fn cell_type(&self) -> u8 {
        self.bytes[1]
    }
}
