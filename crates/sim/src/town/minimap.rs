//! The minimap overlay of the town's entities (saves store its image).

use super::*;

impl StartingTown {
    // +0x32; Game +0x148
    /// The minimap overlay (`Game`): every live entity whose MapDescription has the minimap flag
    /// fills its map rectangle with its colour (rows flipped so the top row is north). RGBA bytes,
    /// `size²` pixels.
    ///
    /// Original: `FUN_140129680`, `FUN_1400aa200`.
    pub fn minimap_overlay(&self) -> Vec<u8> {
        let n = self.cells.size as i32;
        let mut out = vec![0u8; (n * n * 4) as usize];
        for p in self.placed.iter().filter(|p| p.alive) {
            let Some(map) = self.templates[p.template]
                .map
                .as_ref()
                .filter(|m| m.minimap)
            else {
                continue;
            };
            let c = map.color;
            let rgba = [(c >> 16) as u8, (c >> 8) as u8, c as u8, (c >> 24) as u8];
            let ((x, y), (w, h)) = (p.cell, p.footprint);
            for j in 0..h {
                for i in 0..w {
                    let (px, row) = (x + i, n - y - j - 1);
                    if (0..n).contains(&px) && (0..n).contains(&row) {
                        let o = ((row * n + px) * 4) as usize;
                        out[o..o + 4].copy_from_slice(&rgba);
                    }
                }
            }
        }
        out
    }
}
