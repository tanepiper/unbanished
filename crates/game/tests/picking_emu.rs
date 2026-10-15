use banished_game::picking::{point, triangle};

#[test]
fn original_ground_clips_selection_ray() {
    let file = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../game-data/derived/emu/picking.tsv.terrain");
    let Ok(text) = std::fs::read_to_string(file) else {
        eprintln!("emulator data missing; skipping");
        return;
    };
    let mut count = 0;
    for (case, line) in text.lines().enumerate() {
        let p: Vec<_> = line.split('\t').collect();
        let v: Vec<f32> = p[0].split(',').map(|v| v.parse().unwrap()).collect();
        let heights: Vec<i16> = p[1].split(',').map(|v| v.parse().unwrap()).collect();
        let grid = banished_game::terrain::HeightGrid {
            width: 17,
            heights: &heights,
        };
        let hit = grid.intersect(
            v[..3].try_into().unwrap(),
            v[3..6].try_into().unwrap(),
            v[6],
        );
        assert_eq!(hit.is_some(), p[2] == "1", "case {case}");
        if let Some((t, position)) = hit {
            assert_eq!(t.to_bits(), p[3].parse::<u32>().unwrap(), "case {case}");
            let bits: Vec<u32> = p[4].split(',').map(|v| v.parse().unwrap()).collect();
            assert_eq!(position.map(f32::to_bits).as_slice(), bits, "case {case}");
        }
        count += 1;
    }
    assert_eq!(count, 200);
}

#[test]
fn original_nearest_front_facing_triangle() {
    let file = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../game-data/derived/emu/picking.tsv");
    let Ok(text) = std::fs::read_to_string(file) else {
        eprintln!("emulator data missing; skipping");
        return;
    };
    let mut count = 0;
    for (case, line) in text.lines().enumerate() {
        let parts: Vec<_> = line.split('\t').collect();
        let v: Vec<f32> = parts[0].split(',').map(|v| v.parse().unwrap()).collect();
        let expected: u32 = parts[1].parse().unwrap();
        let bits: Vec<u32> = parts[2].split(',').map(|v| v.parse().unwrap()).collect();
        let origin = v[..3].try_into().unwrap();
        let direction = v[3..6].try_into().unwrap();
        let mut maximum = v[6];
        let mut picked = u32::MAX;
        let mut hit = [0.; 3];
        for (i, t) in v[7..].chunks_exact(9).enumerate() {
            let verts = [
                t[..3].try_into().unwrap(),
                t[3..6].try_into().unwrap(),
                t[6..9].try_into().unwrap(),
            ];
            if let Some(distance) = triangle(origin, direction, verts, maximum) {
                maximum = distance;
                picked = i as u32;
                hit = point(origin, direction, distance);
            }
        }
        assert_eq!(picked, expected, "case {case}");
        assert_eq!(hit.map(f32::to_bits).as_slice(), bits, "case {case}");
        count += 1;
    }
    assert_eq!(count, 500);
}
