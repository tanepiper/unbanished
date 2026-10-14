//! Compare each generation stage with ground truth emulated from the original executable.
//! Produce it with `tools/emu/terrain_stages.py game-data/derived/emu/valleys-small-290 290 ...`;
//! the test is skipped when that git-ignored folder is absent.

use banished_game::terrain::{Generator, SizeRecord};
use std::path::PathBuf;

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
        eprintln!("stage {n}: bit-exact");
    }
}

const VALLEYS_SMALL: [i32; 17] = [
    256, 50, 20, 30, 4, 20, 30, 3, 3, 0, 2, 20, 30, 40, 6, 12, 4_000_000,
];

#[test]
fn valleys_small_seed_290() {
    let upto = std::env::var("TERRAIN_STAGES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);
    check("valleys-small-290", 290, VALLEYS_SMALL, upto);
}
