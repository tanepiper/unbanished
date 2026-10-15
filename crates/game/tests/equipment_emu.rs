use banished_game::agents::{CitizenRulesAi, CitizenState};
use banished_game::character::{Component, Message};

#[test]
fn original_equipment_messages() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../game-data/derived/emu/storage/equipment.tsv");
    let Ok(data) = std::fs::read_to_string(path) else {
        eprintln!("equipment ground truth missing; skipping");
        return;
    };
    let mut count = 0;
    for row in data.lines() {
        let p: Vec<_> = row.split('\t').collect();
        let mut state = CitizenState {
            tool_uses: p[1].parse().unwrap(),
            flags: p[3].parse().unwrap(),
            ..Default::default()
        };
        let rules = CitizenRulesAi {
            clothing_duration: p[4].parse::<f32>().unwrap() * 300.0,
            ..Default::default()
        };
        state.message(
            Message {
                component: Component::Citizen,
                message: p[0].parse().unwrap(),
                entity: None,
                a: p[2].parse().unwrap(),
                b: 0,
            },
            rules,
        );
        let actual = [
            state.tool_status as i64,
            state.tool_uses as i64,
            state.clothing_status as i64,
            state.clothing as i64,
            state.flags as i64,
            state.clothing_remaining,
        ];
        let expected: Vec<i64> = p[5..].iter().map(|v| v.parse().unwrap()).collect();
        assert_eq!(actual.as_slice(), expected, "message case {count}");
        count += 1;
    }
    assert_eq!(count, 300);
}
