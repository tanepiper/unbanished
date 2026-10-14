//! Terrain generation (`FUN_1400d4860` and its stages in Application-steam-x64.exe).
//!
//! `Generator` mirrors the game's generation context: the float height grid at `+0x08`, record
//! collections at `+0x18..+0x48`, the `Random` at `+0x58` and the size record pointer at `+0xa28`.
//! Each stage is ported from disassembly and checked against emulated ground truth
//! (tools/emu/terrain_stages.py) and against saved maps.

mod stages;

use crate::random::Random;

/// A `TerrainGenerator` size record (0x44 bytes) in struct order. The `.crs` wire order differs:
/// fields `+0x00..+0x20`, then `+0x28..+0x40`, then `+0x24` (`FUN_1400211d0`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SizeRecord(pub [i32; 17]);

impl SizeRecord {
    /// Build from the 17 values in serialized (wire) order.
    pub fn from_wire(w: [i32; 17]) -> SizeRecord {
        let mut s = [0; 17];
        s[..9].copy_from_slice(&w[..9]);
        s[9] = w[16];
        s[10..].copy_from_slice(&w[9..16]);
        SizeRecord(s)
    }

    /// Value at byte offset `off` of the in-memory record.
    pub fn at(&self, off: usize) -> i32 {
        self.0[off / 4]
    }

    /// Half the map size in cells (`n`); the grid has `2n + 1` samples per side.
    pub fn half_size(&self) -> i32 {
        self.0[0]
    }
}

/// One entry of a path record (28 bytes): a 2D point plus five parameters copied from the
/// caller (`FUN_1400d1d20` arguments 4..8).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PathPoint {
    pub pos: [f32; 2],
    pub params: [f32; 5],
}

pub struct Generator {
    /// Grid size in cells (`2n`); the float grid is `(size + 1)^2`.
    pub size: i32,
    pub grid: Vec<f32>,
    pub paths: Vec<Vec<PathPoint>>,
    pub vec28: Vec<[f32; 3]>,
    pub points: Vec<[f32; 3]>,
    pub lakes: Vec<[f32; 3]>,
    pub rng: Random,
    pub record: SizeRecord,
}

impl Generator {
    /// Context as `FUN_1400b5700` prepares it before generation.
    pub fn new(record: SizeRecord, seed: u32) -> Generator {
        Generator {
            size: record.half_size() * 2,
            grid: Vec::new(),
            paths: Vec::new(),
            vec28: Vec::new(),
            points: Vec::new(),
            lakes: Vec::new(),
            rng: Random::new(seed),
            record,
        }
    }

    pub fn width(&self) -> usize {
        (self.size + 1) as usize
    }

    /// The map's signed 16-bit height samples (`FUN_1400b1980`): `(short)(int)(grid * 256)`.
    pub fn heights(&self) -> Vec<i16> {
        self.grid
            .iter()
            .map(|&v| (v * 256.0) as i32 as i16)
            .collect()
    }

    /// `FUN_1400d4860`: run every stage. `after_stage` sees the grid after each of the five.
    pub fn run(&mut self, mut after_stage: impl FnMut(u32, &Generator)) {
        self.size = self.record.half_size() * 2;
        self.stage1_noise();
        after_stage(1, self);
        self.stage2_records();
        after_stage(2, self);
        self.stage3_shape();
        after_stage(3, self);
        self.stage4_downhill();
        after_stage(4, self);
        self.stage5_smooth();
        after_stage(5, self);
    }
}
