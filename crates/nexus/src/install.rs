//! The `.pkm` packages in a downloaded file.
//!
//! Banished mods on Nexus Mods come as archives with the `.pkm` package inside (and often a
//! readme). Of the 80 mods listed in October 2026, the main file was a `.zip` for 62, a `.7z`
//! for 7 and a `.rar` for 9. This module opens a `.zip` or a bare `.pkm`; for `.7z` and `.rar`
//! it says the player must unpack the file themselves.

use std::io::Read;

use crate::Error;

/// The largest package taken from an archive, so a damaged or hostile archive cannot fill the
/// memory or the disk.
pub const MAX_PACKAGE: u64 = 1 << 30;

/// A mod package to install: its file name (no folders) and its bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Package {
    /// The file name, ending in `.pkm`, with any folders in the archive removed.
    pub name: String,
    pub data: Vec<u8>,
}

/// The kind of a download, from its first bytes.
fn kind(data: &[u8]) -> Option<&'static str> {
    if data.starts_with(b"PK\x03\x04") || data.starts_with(b"PK\x05\x06") {
        Some("zip")
    } else if data.starts_with(b"7z\xbc\xaf\x27\x1c") {
        Some("7z")
    } else if data.starts_with(b"Rar!\x1a\x07") {
        Some("rar")
    } else {
        None
    }
}

/// The last part of a path in an archive (either slash), if it is a `.pkm` file name.
fn package_name(path: &str) -> Option<String> {
    let name = path.rsplit(['/', '\\']).next()?.trim();
    let ok = name.len() > 4
        && name.to_ascii_lowercase().ends_with(".pkm")
        && !name.starts_with('.')
        && !name.contains(['\0', ':']);
    ok.then(|| name.to_string())
}

/// The packages in a download named `file_name` (the name decides only for a bare `.pkm`).
pub fn packages(file_name: &str, data: &[u8]) -> Result<Vec<Package>, Error> {
    match kind(data) {
        Some("zip") => from_zip(data),
        Some(other) => Err(Error::Unsupported(other.to_string())),
        None => match package_name(file_name) {
            Some(name) => Ok(vec![Package {
                name,
                data: data.to_vec(),
            }]),
            None => Err(Error::Unsupported(crate::api::extension(file_name))),
        },
    }
}

fn from_zip(data: &[u8]) -> Result<Vec<Package>, Error> {
    let bad = |e: zip::result::ZipError| Error::Answer(format!("the zip file is damaged: {e}"));
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(data)).map_err(bad)?;
    let mut out: Vec<Package> = Vec::new();
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(bad)?;
        if !entry.is_file() {
            continue;
        }
        let Some(name) = entry.name().ok().and_then(|n| package_name(&n)) else {
            continue;
        };
        if entry.size() > MAX_PACKAGE {
            return Err(Error::Answer(format!("{name} is too large")));
        }
        let mut bytes = Vec::with_capacity(entry.size() as usize);
        (&mut entry)
            .take(MAX_PACKAGE + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| Error::Answer(format!("could not unpack {name}: {e}")))?;
        if bytes.len() as u64 > MAX_PACKAGE {
            return Err(Error::Answer(format!("{name} is too large")));
        }
        // Two folders with the same package: keep the first.
        if !out.iter().any(|p| p.name.eq_ignore_ascii_case(&name)) {
            out.push(Package { name, data: bytes });
        }
    }
    if out.is_empty() {
        return Err(Error::NoPackage);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn zip_of(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        for (name, data) in files {
            w.start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            w.write_all(data).unwrap();
        }
        w.finish().unwrap().into_inner()
    }

    #[test]
    fn takes_packages_from_a_zip() {
        let z = zip_of(&[
            ("Readme.txt", b"hello"),
            ("MoreTerrain/MoreTerrain.pkm", b"PKG1"),
            ("../../evil/Other.PKM", b"PKG2"),
        ]);
        let p = packages("More Terrain v6-19-6.zip", &z).unwrap();
        let names: Vec<&str> = p.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["MoreTerrain.pkm", "Other.PKM"]);
        assert_eq!(p[0].data, b"PKG1");
        assert_eq!(
            packages("x.zip", &zip_of(&[("a.txt", b"x")])),
            Err(Error::NoPackage)
        );
    }

    #[test]
    fn bare_packages_and_other_archives() {
        assert_eq!(
            packages("Apiary.pkm", b"data").unwrap()[0].name,
            "Apiary.pkm"
        );
        assert_eq!(
            packages("a.rar", b"Rar!\x1a\x07\x00rest"),
            Err(Error::Unsupported("rar".into()))
        );
        assert_eq!(
            packages("a.7z", b"7z\xbc\xaf\x27\x1crest"),
            Err(Error::Unsupported("7z".into()))
        );
        assert_eq!(
            packages("notes.txt", b"x"),
            Err(Error::Unsupported("txt".into()))
        );
    }
}
