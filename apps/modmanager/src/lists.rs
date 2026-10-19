//! Named mod lists: which mods are on, in which order, saved as a text file each. One line per
//! mod, in list order: `+file` for an enabled mod and `-file` for a disabled one (the file name
//! without `.pkm`).

use std::path::Path;

use banished_mods::Modification;

/// A list's text for the mods as they are now.
pub fn to_text(mods: &Modification) -> String {
    mods.mods
        .iter()
        .map(|m| format!("{}{}\n", if m.enabled { '+' } else { '-' }, m.file))
        .collect()
}

/// Apply a list: the listed mods take its order and state, at the top; mods it does not name
/// keep their order after them and are turned off. Returns the files it names that are not
/// installed.
pub fn apply(mods: &mut Modification, text: &str) -> Vec<String> {
    let mut wanted: Vec<(String, bool)> = Vec::new();
    for line in text.lines().map(str::trim) {
        let (on, file) = match line.split_at_checked(1) {
            Some(("+", f)) => (true, f),
            Some(("-", f)) => (false, f),
            _ => continue,
        };
        wanted.push((file.trim().to_string(), on));
    }
    let mut missing = Vec::new();
    let mut ordered = Vec::with_capacity(mods.mods.len());
    let mut rest = std::mem::take(&mut mods.mods);
    for (file, on) in &wanted {
        match rest.iter().position(|m| m.file.eq_ignore_ascii_case(file)) {
            Some(i) => {
                let mut m = rest.remove(i);
                m.enabled = *on;
                ordered.push(m);
            }
            None => missing.push(file.clone()),
        }
    }
    for mut m in rest {
        m.enabled = false;
        ordered.push(m);
    }
    for (i, m) in ordered.iter_mut().enumerate() {
        m.index = i as i32;
    }
    mods.mods = ordered;
    mods.dirty = true;
    missing
}

/// The saved lists' names, sorted.
pub fn names(dir: &Path) -> Vec<String> {
    let Ok(read) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut names: Vec<String> = read
        .flatten()
        .filter_map(|e| {
            let p = e.path();
            (p.extension()? == "txt").then(|| p.file_stem()?.to_str().map(str::to_string))?
        })
        .collect();
    names.sort_by_key(|n| n.to_lowercase());
    names
}

/// A list name made safe for a file name.
pub fn file_name(name: &str) -> String {
    let clean: String = name
        .trim()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || " -_.".contains(c) {
                c
            } else {
                '_'
            }
        })
        .collect();
    format!("{}.txt", clean.trim_matches('.'))
}

#[cfg(test)]
mod tests {
    use banished_mods::text::TextSettings;
    use banished_mods::Modification;

    /// The sample mods (git-ignored), or `None` without them.
    fn sample() -> Option<Modification> {
        let dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../game-data/mods/downloads");
        let found = banished_mods::find(&dir);
        (found.len() >= 3).then(|| Modification::refresh(found, &TextSettings::default()))
    }

    #[test]
    fn lists_round_trip() {
        let Some(mut mods) = sample() else { return };
        let files: Vec<String> = mods.mods.iter().map(|m| m.file.clone()).collect();
        // Put the last mod first and turn it on; name one that is not installed.
        let text = format!("+{}\n-{}\n+NotInstalled\n", files[2], files[0]);
        let missing = super::apply(&mut mods, &text);
        assert_eq!(missing, vec!["NotInstalled".to_string()]);
        assert_eq!(mods.mods[0].file, files[2]);
        assert!(mods.mods[0].enabled && !mods.mods[1].enabled);
        assert!(mods.mods.iter().skip(2).all(|m| !m.enabled));
        let indices: Vec<i32> = mods.mods.iter().map(|m| m.index).collect();
        assert_eq!(indices, (0..mods.mods.len() as i32).collect::<Vec<_>>());
        assert!(super::to_text(&mods).starts_with(&format!("+{}\n-{}\n", files[2], files[0])));
    }

    #[test]
    fn list_names_are_safe() {
        assert_eq!(super::file_name("My list"), "My list.txt");
        assert_eq!(super::file_name("../../evil/x"), "_.._evil_x.txt");
    }
}
