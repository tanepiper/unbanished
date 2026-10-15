//! The pointer resources (`UI::Cursor`), using the player's packages when present.
use std::path::PathBuf;

use banished_formats::pkg::{Archive, Package};

#[test]
fn every_cursor_names_its_sheet_and_hotspot() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../game-data/original/WinData");
    if !dir.join("data0.pkg").exists() {
        eprintln!("game data not found; skipping");
        return;
    }
    let mut archive = Archive::default();
    for name in ["data1.pkg", "patch1.pkg", "patch0.pkg", "data0.pkg"] {
        archive.add(Package::parse(std::fs::read(dir.join(name)).unwrap()).unwrap());
    }
    for name in ["Arrow", "Hand", "Hourglass", "Move", "Place", "Query"] {
        let data = archive
            .read_ref(&format!("Cursor/{name}.rsc:cursor"))
            .unwrap();
        let c = banished_formats::ui::cursor(&data).unwrap();
        let sheet = c.sprite_sheet.unwrap();
        assert_eq!(sheet, format!(r"Cursor\{name}.rsc:spriteSheet"));
        let sheet = banished_formats::ui::sprite_sheet(
            &archive.read_ref(&sheet.replace('\\', "/")).unwrap(),
        )
        .unwrap();
        eprintln!(
            "{name}: hotspot {:?}, sheet {}x{}, {} sprites",
            c.hotspot,
            sheet.width,
            sheet.height,
            sheet.sprites.len()
        );
        assert!(c.hotspot[0] < sheet.width && c.hotspot[1] < sheet.height);
    }
}
