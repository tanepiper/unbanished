use super::*;

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("unbanished-steam-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn game_in(library: &Path) -> PathBuf {
    let w = library.join("steamapps/common/Banished/WinData");
    std::fs::create_dir_all(&w).unwrap();
    std::fs::write(w.join(MARKER), b"").unwrap();
    w
}

#[test]
fn the_vdf_lists_its_paths() {
    let text = r#""libraryfolders"
{
	"0"
	{
		"path"		"C:\\Program Files (x86)\\Steam"
		"label"		""
	}
	"1"
	{
		"path"		"/mnt/games/Steam"
	}
}"#;
    assert_eq!(
        vdf::library_paths(text),
        ["C:\\Program Files (x86)\\Steam", "/mnt/games/Steam"]
    );
    // The old layout: a number and the folder on one line.
    let old =
        "\"LibraryFolders\"\n{\n\t\"TimeNextStatsReport\"\t\"1\"\n\t\"1\"\t\"D:\\\\Steam\"\n}";
    assert_eq!(vdf::library_paths(old), ["D:\\Steam"]);
}

#[test]
fn a_chosen_folder_is_the_game_or_its_win_data() {
    let root = temp("chosen");
    let w = game_in(&root);
    let banished = w.parent().unwrap();
    assert_eq!(resolve_folder(banished), Some(w.clone()));
    assert_eq!(resolve_folder(&w), Some(w.clone()));
    assert_eq!(resolve_folder(&root), None);
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn every_library_of_a_steam_is_searched() {
    let root = temp("libraries");
    let steam = root.join("Steam");
    let other = root.join("Games");
    std::fs::create_dir_all(steam.join("steamapps")).unwrap();
    let w = game_in(&other);
    let vdf = format!(
        "\"libraryfolders\"\n{{\n\"0\"\n{{\n\"path\" \"{}\"\n}}\n\"1\"\n{{\n\"path\" \"{}\"\n}}\n}}",
        steam.display(),
        other.display()
    );
    std::fs::write(steam.join("steamapps/libraryfolders.vdf"), vdf).unwrap();
    assert_eq!(in_steam(&steam), [w]);
    std::fs::remove_dir_all(&root).unwrap();
}

#[cfg(not(windows))]
#[test]
fn a_bottle_library_on_another_drive_maps_into_the_bottle() {
    let bottle = Path::new("/b/Bottles/Steam");
    let root = bottle.join("drive_c/Program Files (x86)/Steam");
    assert_eq!(
        library_path(&root, "D:\\Games\\Steam"),
        bottle.join("drive_d/Games/Steam")
    );
    assert_eq!(
        library_path(&root, "C:\\Program Files (x86)\\Steam"),
        bottle.join("drive_c/Program Files (x86)/Steam")
    );
    assert_eq!(
        library_path(&root, "/native/Steam"),
        PathBuf::from("/native/Steam")
    );
}

#[cfg(not(windows))]
#[test]
fn a_bottle_steam_is_found_by_its_default_folder() {
    let root = temp("bottle");
    let drive_c = root.join("drive_c");
    let steam = drive_c.join("Program Files (x86)/Steam");
    let w = game_in(&steam);
    let found: Vec<PathBuf> = windows_steams("Wine", &drive_c)
        .into_iter()
        .flat_map(|(_, r)| in_steam(&r))
        .collect();
    assert_eq!(found, [w]);
    std::fs::remove_dir_all(&root).unwrap();
}
