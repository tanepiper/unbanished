//! Compare each generation stage with ground truth emulated from the original executable.
//! Produce it with `tools/emu/terrain_stages.py game-data/derived/emu/valleys-small-290 290 ...`;
//! the test is skipped when that git-ignored folder is absent.

use std::path::{Path, PathBuf};
use unbanished_game::terrain::{Generator, HeightGrid, SizeRecord, TerrainMesh};
use unbanished_game::world::map::Map;

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
fn check_records(dir: &Path, g: &Generator) {
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
    if let Ok(want) = std::fs::read(dir.join("map_types.u8")) {
        let map = Map::new(g.size as usize / 2, &grid);
        let got: Vec<u8> = map.types.iter().map(|t| t.0).collect();
        assert_eq!(got, want, "gameplay map types");
        let start = std::fs::read_to_string(dir.join("start.txt")).unwrap();
        let v: Vec<i32> = start
            .split_whitespace()
            .map(|s| s.parse().unwrap())
            .collect();
        assert_eq!(map.find_start(v[0]), (v[1], v[2]), "start point");
        eprintln!(
            "gameplay map and start point ({}, {}): bit-exact",
            v[1], v[2]
        );
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

/// `FUN_1400d4fd0` against `tools/emu/river_mesh.py`'s `rivers.bin`: every path's strip.
#[test]
fn river_strips_valleys_small_290() {
    let Some(dir) = truth_dir("valleys-small-290") else {
        return;
    };
    let Ok(bytes) = std::fs::read(dir.join("rivers.bin")) else {
        return;
    };
    let mut g = Generator::new(SizeRecord::from_wire(VALLEYS_SMALL), 290);
    g.stage1_noise();
    g.stage2_records();
    let mut at = 0;
    let paths = read_i32(&bytes, &mut at) as usize;
    assert_eq!(paths, g.paths.len());
    for (p, path) in g.paths.iter().enumerate() {
        let vc = read_i32(&bytes, &mut at) as usize;
        let want_v: Vec<[u32; 4]> = (0..vc)
            .map(|_| std::array::from_fn(|_| read_i32(&bytes, &mut at) as u32))
            .collect();
        let ic = read_i32(&bytes, &mut at) as usize;
        let want_i: Vec<u16> = (0..ic)
            .map(|k| u16::from_le_bytes([bytes[at + 2 * k], bytes[at + 2 * k + 1]]))
            .collect();
        at += 2 * ic;
        let (got_v, got_i) = unbanished_game::terrain::river_strip(path);
        let got_v: Vec<[u32; 4]> = got_v.iter().map(|v| v.map(f32::to_bits)).collect();
        let first = got_v.iter().zip(&want_v).position(|(a, b)| a != b);
        assert!(
            got_v.len() == want_v.len() && first.is_none(),
            "path {p}: {} vertices, want {}; first difference at {first:?}: got {:?} want {:?}",
            got_v.len(),
            want_v.len(),
            first.map(|i| got_v[i].map(f32::from_bits)),
            first.map(|i| want_v[i].map(f32::from_bits)),
        );
        assert_eq!(got_i, want_i, "path {p} indices");
    }
}

/// `FUN_1400b0730`: the rivers' strips at `−192 − i`, then each lake's faded quads at
/// `i − 191`, with ranges that cover the buffers in order.
#[test]
fn water_mesh_valleys_small_1() {
    if truth_dir("valleys-small-1").is_none() {
        return;
    }
    let mut g = Generator::new(SizeRecord::from_wire(VALLEYS_SMALL), 1);
    g.run(|_, _| {});
    let heights = g.heights();
    let mesh = unbanished_game::terrain::water_mesh(&g.paths, &g.lakes, &heights, g.size as usize);
    assert_eq!(mesh.rivers.len(), g.paths.len());
    assert_eq!(mesh.lakes.len(), g.lakes.len());
    let (mut index, mut vertex) = (0, 0);
    for (k, r) in mesh.rivers.iter().chain(&mesh.lakes).enumerate() {
        assert_eq!((r[0], r[1]), (index, vertex), "range {k}");
        let z = if k < mesh.rivers.len() {
            -192 - k as i16
        } else {
            (k - mesh.rivers.len()) as i16 - 191
        };
        for v in &mesh.vertices[r[1] as usize..(r[1] + r[2]) as usize] {
            assert_eq!(i16::from_le_bytes([v[4], v[5]]), z);
        }
        for &i in &mesh.indices[r[0] as usize..(r[0] + r[3]) as usize] {
            assert!((i as i32) < r[2]);
        }
        index += r[3];
        vertex += r[2];
    }
    assert_eq!(index as usize, mesh.indices.len());
    assert_eq!(vertex as usize, mesh.vertices.len());
    // Seed 1 has two lakes.
    assert_eq!(mesh.lakes.len(), 2);
    assert!(mesh.lakes.iter().all(|l| 0 < l[3]), "{:?}", mesh.lakes);
}
