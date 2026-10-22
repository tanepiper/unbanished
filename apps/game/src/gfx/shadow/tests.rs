use super::*;

fn camera() -> MainCamera {
    // The new-game camera: yaw 0, pitch −40, distance 25 around the origin.
    let forward = crate::view::scene::from_yaw_pitch(0.0, -40.0);
    let right = forward.cross(Vec3::Z).normalize();
    MainCamera {
        eye: -forward * 25.0,
        right,
        forward,
        up: right.cross(forward),
        hfov: 40.0 * 16.0 / 9.0,
        vfov: 40.0,
        near: 2.0,
    }
}

#[test]
fn lookup_maps_the_camera_centre_to_the_map_centre() {
    let sun = sun_camera(
        &camera(),
        Vec3::new(0.3, 0.5, -0.8).normalize(),
        shadow_size(1),
    );
    let p = sun.lookup() * (sun.position + sun.forward * 100.0).extend(1.0);
    assert!((p.x - 0.5).abs() < 1e-5 && (p.y - 0.5).abs() < 1e-5);
    assert!((p.z - 99.0 / 255.0).abs() < 1e-5);
    // Up in the sun's view is up (smaller v) in the map.
    let q = sun.lookup() * (sun.position + sun.forward * 100.0 + sun.up).extend(1.0);
    assert!(q.y < 0.5);
}

/// Against the original under the emulator (`tools/emu/sun_camera.py`, output git-ignored).
#[test]
fn matches_the_original_get_sun_camera() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../game-data/derived/emu/sun_camera.tsv"
    );
    let Ok(text) = std::fs::read_to_string(path) else {
        eprintln!("skipping: {path} missing (run tools/emu/sun_camera.py)");
        return;
    };
    let parse = |s: &str| -> Vec<f32> {
        s.split(',')
            .map(|b| f32::from_bits(b.parse().unwrap()))
            .collect()
    };
    let (mut cases, mut exact) = (0, 0);
    for line in text.lines() {
        let cols: Vec<&str> = line.split('\t').collect();
        let i = parse(cols[0]);
        let v = |k: usize| Vec3::new(i[k], i[k + 1], i[k + 2]);
        let cam = MainCamera {
            eye: v(0),
            right: v(3),
            forward: v(6),
            up: v(9),
            hfov: i[12],
            vfov: i[13],
            near: 2.0,
        };
        let o = parse(cols[1]);
        let radius = -f32::from_bits(cols[2].parse().unwrap());
        let want = SunCamera {
            right: Vec3::new(o[0], o[1], o[2]),
            forward: Vec3::new(o[3], o[4], o[5]),
            up: Vec3::new(o[6], o[7], o[8]),
            position: Vec3::new(o[9], o[10], o[11]),
            radius,
        };
        let got = sun_camera(&cam, v(14), 1024);
        cases += 1;
        if got == want {
            exact += 1;
            continue;
        }
        // CRT sin/cos/sqrt may differ from Rust's by an ulp; allow that much.
        let texel = want.radius * 2.0 / 1024.0;
        assert!(
            (got.radius - want.radius).abs() <= want.radius * 1e-5,
            "{got:?} vs {want:?}"
        );
        assert!(
            (got.position - want.position).length() <= texel * 1.01,
            "{got:?} vs {want:?}"
        );
        assert!(
            (got.forward - want.forward).length() < 1e-6,
            "{got:?} vs {want:?}"
        );
    }
    assert!(cases > 0);
    eprintln!("{exact}/{cases} sun cameras bit-exact");
}

#[test]
fn position_is_snapped_to_texels() {
    let sun = sun_camera(
        &camera(),
        Vec3::new(0.3, 0.5, -0.8).normalize(),
        shadow_size(1),
    );
    let texel = sun.radius * 2.0 / shadow_size(1) as f32;
    for axis in [sun.right, sun.forward, sun.up] {
        let v = axis.dot(sun.position) / texel;
        assert!((v - v.round()).abs() < 1e-2, "{v}");
    }
}
