//! Event-log entries in real saves (`save-game/*.sav`, local only) against the port's colours,
//! categories, icons and the text of events without arguments.

use unbanished_formats::save::Save;
use unbanished_game::events::{category, severity_color};

#[test]
fn saved_events_match() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../save-game");
    let Ok(entries) = std::fs::read_dir(dir) else {
        eprintln!("no saves; skipping");
        return;
    };
    let seasons = [
        "Early Spring",
        "Spring",
        "Late Spring",
        "Early Summer",
        "Summer",
        "Late Summer",
        "Early Autumn",
        "Autumn",
        "Late Autumn",
        "Early Winter",
        "Winter",
        "Late Winter",
    ];
    let fixed = |kind: i32| match kind {
        17 => Some("The reserve of logs is low!"),
        18 => Some("The reserve of stone is low!"),
        19 => Some("The reserve of iron is low!"),
        20 => Some("The reserve of firewood is low!"),
        21 => Some("The reserve of tools is low!"),
        22 => Some("The reserve of food is low!"),
        23 => Some("Storage for produced goods is near capacity!"),
        24 => Some("Storage for logs, stone, and iron is near capacity!"),
        30 => Some("A tornado has touched down near the town!"),
        31 => Some("A building has caught fire!"),
        _ => None,
    };
    let (mut total, mut texts) = (0, 0);
    for e in entries.flatten() {
        let path = e.path();
        if path.extension().is_none_or(|x| x != "sav") {
            continue;
        }
        let save = Save::parse(&std::fs::read(&path).unwrap()).unwrap();
        let log = save
            .event_log()
            .unwrap_or_else(|| panic!("{path:?}: no event log"));
        for ev in &log.entries {
            assert_eq!(ev.color, severity_color(ev.severity), "{path:?} {ev:?}");
            assert_eq!(ev.category, category(ev.kind), "{path:?} {ev:?}");
            let icon = match ev.kind {
                0x1a | 0x1c => 12,
                0x1b | 0x1d => 13,
                _ => -1,
            };
            assert_eq!(ev.icon, icon, "{path:?} {ev:?}");
            if let Some(body) = fixed(ev.kind) {
                // "@0 @1: body" with a season and a 1-based year.
                let (prefix, rest) = ev.text.split_once(": ").unwrap();
                assert_eq!(rest, body, "{path:?}");
                let (season, year) = prefix.rsplit_once(' ').unwrap();
                assert!(seasons.contains(&season), "{prefix}");
                assert!(year.parse::<u32>().unwrap() >= 1);
                texts += 1;
            }
            total += 1;
        }
    }
    eprintln!("{total} saved events checked, {texts} full texts");
    assert!(total > 0);
}
