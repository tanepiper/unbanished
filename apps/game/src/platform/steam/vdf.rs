//! Reads the library folders from Steam's `steamapps/libraryfolders.vdf`, a Valve KeyValues
//! text file. Only the `"path"` values are needed, so only those are read: each line
//! `"path"  "VALUE"`, with `\\` for a backslash. Older files list a library as `"1"  "VALUE"`;
//! those are read too.

/// The library folders the file lists, in its order.
pub fn library_paths(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| {
            let mut parts = quoted(line);
            let key = parts.next()?;
            let value = parts.next()?;
            let library = key == "path" || (key.parse::<u32>().is_ok() && looks_like_path(&value));
            library.then_some(value)
        })
        .collect()
}

fn looks_like_path(v: &str) -> bool {
    v.starts_with('/') || v.as_bytes().get(1) == Some(&b':')
}

/// The quoted strings on one line, with `\\` and `\"` unescaped.
fn quoted(line: &str) -> impl Iterator<Item = String> + '_ {
    let mut chars = line.chars();
    std::iter::from_fn(move || {
        chars.by_ref().find(|&c| c == '"')?;
        let mut s = String::new();
        while let Some(c) = chars.next() {
            match c {
                '"' => return Some(s),
                '\\' => s.push(chars.next()?),
                c => s.push(c),
            }
        }
        None
    })
}
