//! Compare each generation stage with ground truth emulated from the original executable.
//! Produce it with `tools/emu/terrain_stages.py game-data/derived/emu/valleys-small-290 290 ...`;
//! the test is skipped when that git-ignored folder is absent.

use banished_game::terrain::{Generator, HeightGrid, SizeRecord, TerrainMesh};
use std::path::{Path, PathBuf};

fn truth_dir(name: &str) -> Option<PathBuf> {
    let d = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../game-data/derived/emu")
        .join(name);
    d.join("grid_1.f32").exists().then_some(d)
}

fn load(path: &PathBuf) -> Vec<f32> {
    std::fs::read(path)
        .unwrap()
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes(c.try_into().unwrap()))
        .collect()
}

/// Compare one stage; returns a description of the first difference.
fn diff(stage: u32, got: &[f32], want: &[f32], width: usize) -> Option<String> {
    assert_eq!(got.len(), want.len(), "stage {stage}: grid size");
    let bad: Vec<usize> = (0..got.len())
        .filter(|&i| got[i].to_bits() != want[i].to_bits())
        .collect();
    let i = *bad.first()?;
    Some(format!(
        "stage {stage}: {} of {} cells differ; first at ({}, {}): got {} want {}",
        bad.len(),
        got.len(),
        i % width,
        i / width,
        got[i],
        want[i]
    ))
}

fn read_i32(b: &[u8], at: &mut usize) -> i32 {
    let v = i32::from_le_bytes(b[*at..*at + 4].try_into().unwrap());
    *at += 4;
    v
}

fn floats(b: &[u8]) -> Vec<u32> {
    b.chunks_exact(4)
        .map(|c| u32::from_le_bytes(c.try_into().unwrap()))
        .collect()
}

/// Compare the stage-2 records (paths, lake-shaping points, lake centres) bit for bit.
/// The truth files are written after all stages, so only fields stage 2 sets are compared.
fn check_records(dir: &PathBuf, g: &Generator) {
    let b = std::fs::read(dir.join("paths.bin")).unwrap();
    let mut at = 0;
    let n = read_i32(&b, &mut at) as usize;
    assert_eq!(g.paths.len(), n, "path count");
    for (pi, path) in g.paths.iter().enumerate() {
        let c = read_i32(&b, &mut at) as usize;
        assert_eq!(path.len(), c, "path {pi} length");
        for (i, p) in path.iter().enumerate() {
            let want = floats(&b[at + 28 * i..at + 28 * i + 28]);
            let got: Vec<u32> = p.pos.iter().chain(&p.params).map(|f| f.to_bits()).collect();
            assert_eq!(got, want, "path {pi} point {i}");
        }
        at += 28 * c;
    }
    for (name, list) in [("points.bin", &g.points), ("lakes.bin", &g.lakes)] {
        let b = std::fs::read(dir.join(name)).unwrap();
        let mut at = 0;
        let n = read_i32(&b, &mut at) as usize;
        assert_eq!(list.len(), n, "{name} count");
        let got: Vec<u32> = list.iter().flatten().map(|f| f.to_bits()).collect();
        assert_eq!(got, floats(&b[4..]), "{name}");
    }
    eprintln!(
        "stage 2 records: {} paths, {} points, {} lakes bit-exact",
        g.paths.len(),
        g.points.len(),
        g.lakes.len()
    );
}

/// Gameplay terrain queries: natural cell types and ground height/normal samples.
fn check_queries(dir: &Path, g: &Generator) {
    let heights = g.heights();
    let grid = HeightGrid {
        width: g.width(),
        heights: &heights,
    };
    if let Ok(want) = std::fs::read(dir.join("cell_types.u8")) {
        let bad: Vec<usize> = (0..want.len())
            .filter(|&i| grid.natural_cell_type(g.size as usize, i).0 != want[i])
            .collect();
        if let Some(&i) = bad.first() {
            panic!(
                "cell types: {} of {} differ; first {i}: got {} want {}",
                bad.len(),
                want.len(),
                grid.natural_cell_type(g.size as usize, i).0,
                want[i]
            );
        }
        eprintln!("natural cell types: bit-exact ({} cells)", want.len());
    }
    if let Ok(b) = std::fs::read(dir.join("height_samples.bin")) {
        for (k, rec) in b.chunks_exact(24).enumerate() {
            let f = floats(rec);
            let (x, y) = (f32::from_bits(f[0]), f32::from_bits(f[1]));
            let mut normal = [0.0f32; 3];
            let h = grid.ground(x, y, &mut normal);
            let got = [
                h.to_bits(),
                normal[0].to_bits(),
                normal[1].to_bits(),
                normal[2].to_bits(),
            ];
            assert_eq!(
                got,
                [f[2], f[3], f[4], f[5]],
                "ground sample {k} at ({x}, {y})"
            );
        }
        eprintln!("ground height samples: bit-exact ({})", b.len() / 24);
    }
}

/// Run stages up to `upto`, failing at the first stage that differs.
fn check(dir: &str, seed: u32, wire: [i32; 17], upto: u32) {
    let Some(dir) = truth_dir(dir) else {
        eprintln!("no emulated ground truth for {dir}; skipping");
        return;
    };
    let mut g = Generator::new(SizeRecord::from_wire(wire), seed);
    let stages: [fn(&mut Generator); 5] = [
        Generator::stage1_noise,
        Generator::stage2_records,
        Generator::stage3_shape,
        Generator::stage4_downhill,
        Generator::stage5_smooth,
    ];
    for (i, stage) in stages.iter().enumerate().take(upto as usize) {
        let n = i as u32 + 1;
        stage(&mut g);
        let want = load(&dir.join(format!("grid_{n}.f32")));
        if let Some(msg) = diff(n, &g.grid, &want, g.width()) {
            panic!("{msg}");
        }
        if n == 2 {
            check_records(&dir, &g);
        }
        if n == 3 {
            let b = std::fs::read(dir.join("vec28.bin")).unwrap();
            let count = i32::from_le_bytes(b[..4].try_into().unwrap()) as usize;
            assert_eq!(g.vec28.len(), count, "hill circle count");
            let got: Vec<u32> = g.vec28.iter().flatten().map(|f| f.to_bits()).collect();
            assert_eq!(got, floats(&b[4..]), "hill circles");
            eprintln!("stage 3 records: {count} hill circles bit-exact");
        }
        eprintln!("stage {n}: bit-exact");
    }
    let tiles = dir.join("tiles.bin");
    if upto >= 5 && tiles.exists() {
        let heights = g.heights();
        let grid = HeightGrid {
            width: g.width(),
            heights: &heights,
        };
        let got = TerrainMesh::build(&grid, g.size as usize).vertex_bytes();
        let want = std::fs::read(tiles).unwrap();
        assert_eq!(got.len(), want.len(), "tile vertex bytes");
        if let Some(i) = (0..got.len()).find(|&i| got[i] != want[i]) {
            let v = i / 16;
            panic!(
                "tile vertex {v} differs: got {:02x?} want {:02x?}",
                &got[v * 16..v * 16 + 16],
                &want[v * 16..v * 16 + 16]
            );
        }
        eprintln!("terrain tile vertices: bit-exact");
    }
    if upto >= 5 {
        check_queries(&dir, &g);
    }
    let surface = dir.join("surface.u32");
    if upto >= 5 && surface.exists() {
        let want = floats(&std::fs::read(surface).unwrap());
        let got = g.surface_map();
        assert_eq!(got.len(), want.len(), "surface size");
        let bad: Vec<usize> = (0..got.len()).filter(|&i| got[i] != want[i]).collect();
        if let Some(&i) = bad.first() {
            let n = g.size as usize;
            panic!(
                "surface: {} of {} texels differ; first at ({}, {}): got {:06x} want {:06x}",
                bad.len(),
                got.len(),
                i % n,
                i / n,
                got[i],
                want[i]
            );
        }
        eprintln!("surface map: bit-exact");
    }
}

const VALLEYS_SMALL: [i32; 17] = [
    256, 50, 20, 30, 4, 20, 30, 3, 3, 0, 2, 20, 30, 40, 6, 12, 4_000_000,
];

const MOUNTAINS_SMALL: [i32; 17] = [
    256, 30, 10, 30, 4, 20, 30, 3, 4, 0, 2, 20, 30, 40, 6, 12, 4_000_000,
];

fn stages() -> u32 {
    std::env::var("TERRAIN_STAGES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(2)
}

#[test]
fn valleys_small_seed_290() {
    check("valleys-small-290", 290, VALLEYS_SMALL, stages());
}

#[test]
fn valleys_small_seed_1() {
    check("valleys-small-1", 1, VALLEYS_SMALL, stages());
}

#[test]
fn valleys_small_seed_12345() {
    check("valleys-small-12345", 12345, VALLEYS_SMALL, stages());
}

#[test]
fn mountains_small_seed_290() {
    check("mountains-small-290", 290, MOUNTAINS_SMALL, stages());
}
