#![doc = include_str!("../README.md")]
#![doc(
    html_logo_url = "https://tanepiper.github.io/unbanished/assets/un-banished-icon.svg",
    html_favicon_url = "https://tanepiper.github.io/unbanished/favicon.svg"
)]

// Original: terrain generation FUN_1400d4860 and its stages (Application-steam-x64.exe).
// `Generator` mirrors the generation context: the float height grid at +0x08, the record
// collections at +0x18..+0x48, the Random at +0x58 and the size record pointer at +0xa28. Each
// stage is checked against emulated ground truth (tools/emu/terrain_stages.py) and saved maps.

mod mesh;
mod query;
mod river;
mod stages;
mod surface;

pub use mesh::{tile_indices, HeightGrid, TerrainMesh, TerrainVertex, TILE_CELLS, TILE_VERTS};
pub use query::CellType;
pub use river::{river_strip, water_mesh, WaterMesh};

use unbanished_core::random::Random;

// The .crs wire order is +0x00..+0x20, then +0x28..+0x40, then +0x24 (FUN_1400211d0).
/// The settings of one map size in a terrain profile: 17 values, in the order the game keeps them
/// in memory. The first value is `n`, half the terrain's size in cells. The resource stores them
/// in another order; [`SizeRecord::from_wire`] converts.
///
/// [`unbanished_formats::game::TerrainGenerator`] reads a profile, with one size record for each
/// map size (small, medium, large).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SizeRecord(pub [i32; 17]);

impl SizeRecord {
    /// A size record from the 17 values in the order the resource stores them.
    pub fn from_wire(w: [i32; 17]) -> SizeRecord {
        let mut s = [0; 17];
        s[..9].copy_from_slice(&w[..9]);
        s[9] = w[16];
        s[10..].copy_from_slice(&w[9..16]);
        SizeRecord(s)
    }

    /// The value at byte offset `off` of the record, as the original reads it.
    pub fn at(&self, off: usize) -> i32 {
        self.0[off / 4]
    }

    /// Half the terrain's size in cells, `n`: 256, 384 or 512. The height grid has `2n + 1`
    /// samples on each side, and the playable map is the central `n × n` cells.
    pub fn half_size(&self) -> i32 {
        self.0[0]
    }
}

// 28 bytes; the parameters are FUN_1400d1d20's arguments 4..8.
/// One point of a river path: a 2D position and five parameters from the size record (for
/// example the river's width).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PathPoint {
    /// The position, in cells from the centre of the terrain.
    pub pos: [f32; 2],
    /// The parameters copied from the size record when the path was made.
    pub params: [f32; 5],
}

/// The map generator: it makes a terrain from a size record and a seed, in five stages.
///
/// ```no_run
/// # fn record() -> unbanished_terrain::SizeRecord { unimplemented!() }
/// use unbanished_terrain::Generator;
///
/// let mut generator = Generator::new(record(), 290);
/// generator.run(|_stage, _g| {});
/// let heights = generator.heights(); // as a save stores them
/// ```
///
/// Original: `FUN_1400d4860`.
pub struct Generator {
    /// The terrain's size in cells, `2n`. The grid has `(size + 1)²` samples.
    pub size: i32,
    /// The float heights, row by row.
    pub grid: Vec<f32>,
    /// The river paths. The map draws each one with the River material.
    pub paths: Vec<Vec<PathPoint>>,
    /// The hill circles of stage 3 (x, y, radius), including the bridges that join them.
    pub vec28: Vec<[f32; 3]>,
    /// The lake points: circles (x, y, radius) around each lake, and its centre.
    pub points: Vec<[f32; 3]>,
    /// The lakes (x, y, depth). The map draws them with the Lake material.
    pub lakes: Vec<[f32; 3]>,
    /// The random number generator, seeded with the map seed. The stages draw from it in order.
    pub rng: Random,
    /// The size record.
    pub record: SizeRecord,
}

impl Generator {
    /// A generator for a size record and a seed, ready to [`run`](Self::run).
    ///
    /// Original: `FUN_1400b5700`.
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

    /// The number of samples on each side of the grid, `size + 1`.
    pub fn width(&self) -> usize {
        (self.size + 1) as usize
    }

    /// The heights as a save stores them: each float height × 256, as a 16-bit integer.
    ///
    /// Original: `FUN_1400b1980`.
    pub fn heights(&self) -> Vec<i16> {
        self.grid
            .iter()
            .map(|&v| (v * 256.0) as i32 as i16)
            .collect()
    }

    /// Run the five stages. `after_stage` is called after each one, with its number (1 to 5).
    ///
    /// Original: `FUN_1400d4860`.
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
