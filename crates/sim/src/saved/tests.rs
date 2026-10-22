#[test]
fn stems() {
    assert_eq!(super::stem(r"Game\Climate\Fair.rsc:resource"), "Fair");
    assert_eq!(super::stem("Game/Terrain/Valleys.rsc"), "Valleys");
}

/// The component index gives, for every object of every save in the git-ignored
/// `save-game/`, the same components in the same order as reading `entity.components`, and
/// the same first component of each type. Skipped without the saves.
#[test]
fn indexed_components_match_the_field() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../save-game");
    let Ok(list) = std::fs::read_dir(&dir) else {
        eprintln!("no saves; skipping");
        return;
    };
    for entry in list.flatten() {
        let Ok(bytes) = std::fs::read(entry.path()) else {
            continue;
        };
        let Ok(saved) = super::SavedGame::parse(&bytes) else {
            continue;
        };
        let mut checked = 0;
        for o in saved.state.objects.values() {
            let want = saved.read_components(o);
            let got = saved.components(o);
            assert!(
                want.len() == got.len() && want.iter().zip(&got).all(|(a, b)| std::ptr::eq(*a, *b)),
                "{:?}: object {}",
                entry.file_name(),
                o.index
            );
            for c in &want {
                let first = want.iter().find(|w| w.type_name == c.type_name).copied();
                let found = saved.component(o, c.type_name);
                assert!(first.zip(found).is_some_and(|(a, b)| std::ptr::eq(a, b)));
            }
            checked += want.len();
        }
        // An object that is not the save's own (a copy) reads its own field.
        if let Some((_, e)) = saved.entities().next() {
            assert!(saved.component_ids(e).is_some(), "the table is used");
            let copy = e.clone();
            assert!(saved.component_ids(&copy).is_none());
            assert_eq!(saved.components(&copy).len(), saved.components(e).len());
        }
        eprintln!("{:?}: {checked} components", entry.file_name());
    }
}

/// The vanilla saves' models hold the rotation the port computes from their yaw: a
/// character's (`FUN_14010e990`, or its model's until its first update), a fenced area's
/// identity (`FUN_1400e02d0` never places it), any other model's `FromYawPitchOffset(yaw,
/// 0)`; and every map rotation but a natural resource's is the yaw's whole degrees wrapped
/// into 0..360. A save under 99% is not the original's (an older port's) and is skipped.
/// A few characters hold a rotation from a yaw since changed. Skipped without the
/// git-ignored `save-game/`.
#[test]
fn saved_rotations_follow_the_yaw() {
    use unbanished_formats::savegame::Value;
    use unbanished_game::math::Transform;
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../save-game");
    let Ok(list) = std::fs::read_dir(&dir) else {
        eprintln!("no saves; skipping");
        return;
    };
    let bits = |v: &[f32]| v.iter().map(|x| x.to_bits()).collect::<Vec<_>>();
    for entry in list.flatten() {
        let Ok(bytes) = std::fs::read(entry.path()) else {
            continue;
        };
        let Ok(saved) = super::SavedGame::parse(&bytes) else {
            continue;
        };
        let (mut ok, mut characters_off, mut off) = (0usize, 0usize, Vec::new());
        for (_, e) in saved.entities() {
            let r = saved.ext(&e.fields, "entity.template").unwrap_or("?");
            let Some(m) = saved.component(e, "ModelComponent") else {
                continue;
            };
            let (Some(yaw), Some(Value::F32s(q))) =
                (m.fields.f32("model.yaw"), m.fields.at("model.rotation"))
            else {
                continue;
            };
            let natural = saved.component(e, "NaturalResourceComponent").is_some();
            let area = r.ends_with("Pasture.rsc:resource") || r.ends_with("Cemetery.rsc:resource");
            let model = bits(&Transform::of_model(yaw, [0.0; 3]).q);
            if saved.component(e, "CharacterComponent").is_some() {
                if bits(q) == bits(&Transform::character_rotation(yaw)) || bits(q) == model {
                    ok += 1;
                } else {
                    characters_off += 1;
                }
                continue;
            }
            let want = if area {
                vec![0, 0, 0, 1f32.to_bits()]
            } else {
                model
            };
            let map_ok = saved.component(e, "MapComponent").is_none_or(|mp| {
                let rot = mp.fields.i32("map.rotation").unwrap_or(0);
                let mut w = if natural || area { 0 } else { yaw as i32 };
                while w < 0 {
                    w += 360;
                }
                area || rot == w
            });
            if bits(q) == want && map_ok {
                ok += 1;
            } else {
                off.push(format!("{r} yaw {yaw} q {:?}", bits(q)));
            }
        }
        let total = ok + characters_off + off.len();
        if (ok as f64) < 0.99 * total as f64 {
            eprintln!(
                "{:?}: {ok} of {total}; not the original's, skipped",
                entry.file_name()
            );
            continue;
        }
        eprintln!(
            "{:?}: {ok} of {total}, {characters_off} characters off",
            entry.file_name()
        );
        assert!(off.is_empty(), "{:?}: {off:?}", entry.file_name());
        assert!(characters_off * 100 <= total, "{:?}", entry.file_name());
    }
}

/// A wide frame keeps its centred 172:128 part: the bars either side are left out.
#[test]
fn preview_crops_the_centre() {
    let (w, h) = (344 * 2, 128 * 2);
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    for y in 0..h {
        for x in 0..w {
            let p = ((y * w + x) * 4) as usize;
            let inside = (172..516).contains(&x);
            rgba[p..p + 4].copy_from_slice(&if inside { [10, 20, 30, 255] } else { [255; 4] });
        }
    }
    let image = super::preview(&rgba, w, h);
    assert_eq!(image.len(), 172 * 128 * 4);
    assert!(image.chunks(4).all(|px| px == [30, 20, 10, 0]));
}
