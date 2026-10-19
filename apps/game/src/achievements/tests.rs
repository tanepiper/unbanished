use super::*;
use crate::start::load::tests_support::{archive, root};

/// The manager, goal lists and TimeGoals against the Runtime's over scripted goal states
/// (`tools/emu/achievements.py`; skipped without its output in game-data/derived/emu):
/// the same goals asked in the same order, and the same flags, times, elapsed times,
/// cursor and awards after every step.
#[test]
fn checks_match_the_runtime() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../game-data/derived/emu/achievements.tsv"
    );
    let Ok(text) = std::fs::read_to_string(path) else {
        eprintln!("no emulator output; skipping");
        return;
    };
    let mut lines = text.lines();
    let (setup, awarded) = lines.next().unwrap().split_once('\t').unwrap();
    let setup: Vec<Vec<Option<i32>>> = setup
        .split('/')
        .map(|a| {
            a.split(',')
                .map(|g| g.strip_prefix('T').map(|y| y.parse().unwrap()))
                .collect()
        })
        .collect();
    let mut m = AchievementManager {
        awarded: awarded.chars().map(|c| c == '1').collect(),
        instances: setup
            .iter()
            .map(|gs| AchievementInstance {
                goals: vec![GoalInstance::default(); gs.len()],
            })
            .collect(),
        elapsed: vec![0; setup.len()],
        cursor: 0,
    };
    let (mut steps, mut won) = (0, 0);
    for line in lines {
        let c: Vec<&str> = line.split('\t').collect();
        let dt: i64 = c[0].parse().unwrap();
        let mut script = c[1]
            .split(';')
            .filter(|s| !s.is_empty())
            .map(|s| {
                let v: Vec<usize> = s.split(',').map(|x| x.parse().unwrap()).collect();
                (v[0], v[1], v[2])
            })
            .collect::<Vec<_>>()
            .into_iter();
        let setup = &setup;
        let award = m.update_with(setup.len(), dt, |a, inst, elapsed| {
            inst.update_with(
                |g| setup[a][g].is_some(),
                elapsed,
                |g, gi| match setup[a][g] {
                    Some(years) => {
                        if time_goal_met(years, gi.time) {
                            GoalState::Met
                        } else {
                            GoalState::NotMet
                        }
                    }
                    None => {
                        let (sa, sg, v) = script.next().expect("asked more than the original");
                        assert_eq!((sa, sg), (a, g), "{line}");
                        [GoalState::NotMet, GoalState::Met, GoalState::Failed][v]
                    }
                },
            )
        });
        assert!(
            script.next().is_none(),
            "asked fewer than the original: {line}"
        );
        won += award.is_some() as usize;
        let awarded: String = m
            .awarded
            .iter()
            .map(|&b| if b { '1' } else { '0' })
            .collect();
        assert_eq!(awarded, c[2], "{line}");
        let elapsed: Vec<String> = m.elapsed.iter().map(|e| e.to_string()).collect();
        assert_eq!(elapsed.join(" "), c[3], "{line}");
        assert_eq!(m.cursor.to_string(), c[4], "{line}");
        let goals: Vec<String> = m
            .instances
            .iter()
            .map(|i| {
                i.goals
                    .iter()
                    .map(|g| format!("{}:{}", g.met as u32 | (g.failed as u32) << 1, g.time))
                    .collect::<Vec<_>>()
                    .join(",")
            })
            .collect();
        assert_eq!(goals.join("/"), c[5], "{line}");
        steps += 1;
    }
    assert!(steps > 0 && won > 0);
}

/// The vanilla saves' achievement instances read back into the manager, and writing them
/// again leaves every goal instance as saved.
#[test]
fn saved_achievements_read_back() {
    use banished_formats::savegame::Value;
    let Some(archive) = archive() else {
        return;
    };
    let list = load(&archive);
    let mut met = 0;
    for name in [
        "Save2.sav",
        "Save3.sav",
        "Save4.sav",
        "Save5.sav",
        "Save83.sav",
    ] {
        let Ok(bytes) = std::fs::read(root().join("save-game").join(name)) else {
            continue;
        };
        let saved = crate::saves::saved::SavedGame::parse(&bytes).unwrap();
        let mut m = AchievementManager {
            awarded: vec![false; list.len()],
            instances: list.iter().map(AchievementInstance::new).collect(),
            elapsed: vec![0; list.len()],
            cursor: 0,
        };
        m.restore(&list, &saved);
        met += m
            .instances
            .iter()
            .flat_map(|x| &x.goals)
            .filter(|g| g.met)
            .count();
        let mut g = saved.state.clone();
        let Some(Value::List(items)) = saved.state.achievements.at("achievements.instances") else {
            panic!("{name}: no instances");
        };
        for item in items {
            let id = item.ptr("instance").unwrap();
            let r = saved
                .ext(&saved.object(id).unwrap().fields, "instance.achievement")
                .unwrap();
            let i = list
                .iter()
                .position(|a| same_reference(&a.reference, r))
                .unwrap();
            write_instance(&mut g, id, &list[i], &m.instances[i]);
        }
        for (id, o) in &saved.state.objects {
            assert_eq!(
                format!("{:?}", o.fields),
                format!("{:?}", g.objects[id].fields),
                "{name}: object {id}"
            );
        }
    }
    eprintln!("{met} goals met in the saves");
}
