//! Original StatusBarUI aggregation, half-icon meter and StatusIcon mask writes.
use unbanished_game::people::status::{aggregate, meter, set_icon, CitizenValue};

fn fixture(name: &str) -> Option<String> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../game-data/derived/emu/ui-status")
        .join(name);
    std::fs::read_to_string(path)
        .map_err(|_| eprintln!("{name} emulator data missing; skipping"))
        .ok()
}

#[test]
fn original_half_icon_meter() {
    let Some(data) = fixture("meter.tsv") else {
        return;
    };
    for (case, row) in data.lines().enumerate() {
        let p: Vec<_> = row.split('\t').collect();
        let values = meter(
            p[0].parse().unwrap(),
            p[1].parse().unwrap(),
            p[2].parse().unwrap(),
        );
        let fmt = |half: bool| {
            values
                .iter()
                .map(|v| {
                    if if half { v.1 } else { v.0 } {
                        '1'
                    } else {
                        '0'
                    }
                })
                .collect::<String>()
        };
        assert_eq!(fmt(false), p[3], "whole case {case}");
        assert_eq!(fmt(true), p[4], "half case {case}");
    }
    assert_eq!(data.lines().count(), 400);
}

#[test]
fn original_status_icon_mask() {
    let Some(data) = fixture("icons.tsv") else {
        return;
    };
    for (case, row) in data.lines().enumerate() {
        let p: Vec<u32> = row.split('\t').map(|v| v.parse().unwrap()).collect();
        let mut mask = p[0];
        set_icon(&mut mask, p[1] as u8, p[2] != 0, p[3] != 0);
        assert_eq!(mask, p[4], "icon mask case {case}");
    }
    assert_eq!(data.lines().count(), 400);
}

#[test]
fn original_status_bar_aggregation() {
    let Some(data) = fixture("town.tsv") else {
        return;
    };
    for (case, row) in data.lines().enumerate() {
        let (people, expected) = row.split_once('\t').unwrap();
        let values = if people == "-" {
            vec![]
        } else {
            people
                .split(',')
                .map(|s| {
                    let p: Vec<_> = s.split(':').collect();
                    CitizenValue {
                        age: p[0].parse().unwrap(),
                        adult_age: 12.0,
                        school: p[1] == "1",
                        health: p[2].parse().unwrap(),
                        happiness: p[3].parse().unwrap(),
                        status: p[4].parse().unwrap(),
                    }
                })
                .collect()
        };
        let result = aggregate(values);
        let actual = [
            result.adults,
            result.students,
            result.children,
            result.counts[0],
            result.counts[1],
            result.counts[2],
            result.counts[3],
            result.counts[4],
            result.counts[5],
            result.health.to_bits(),
            result.happiness.to_bits(),
        ];
        let expected: Vec<u32> = expected.split(',').map(|v| v.parse().unwrap()).collect();
        assert_eq!(actual.as_slice(), expected, "town case {case}");
    }
    assert_eq!(data.lines().count(), 200);
}
