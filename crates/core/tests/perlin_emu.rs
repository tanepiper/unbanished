//! Perlin noise against the original Runtime (`PerlinNoise::Generate` under the emulator,
//! `tools/emu/perlin.py`): the temperature's settings and others, seeds over the whole int range
//! and negative, whole, fractional and large coordinates, each value bit for bit. Skips when the
//! ground truth has not been made.
use unbanished_core::noise::PerlinNoise;

#[test]
fn original_perlin() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../game-data/derived/emu/perlin/cases.tsv");
    let Ok(data) = std::fs::read_to_string(path) else {
        eprintln!("perlin ground truth missing; skipping");
        return;
    };
    let (mut count, mut differ) = (0, Vec::new());
    for line in data.lines().skip(1) {
        let p: Vec<u32> = line
            .split('\t')
            .map(|v| u32::from_str_radix(v, 16).unwrap())
            .collect();
        let f = f32::from_bits;
        let noise = PerlinNoise::new(f(p[0]), f(p[1]), p[2] as i32, p[3] as i32);
        let v = noise.generate(f(p[4]), f(p[5]));
        if v.to_bits() != p[6] {
            differ.push((f(p[0]), f(p[1]), f(p[4]), f(p[5]), v, f(p[6])));
        }
        count += 1;
    }
    assert!(count > 0);
    assert!(
        differ.is_empty(),
        "{} of {count} differ, first: {:?}",
        differ.len(),
        &differ[..differ.len().min(5)]
    );
    eprintln!("{count} noise values match the original");
}
