//! Packages (`.pkg`, and mods as `.pkm`): the archives that hold the game's data.
//!
//! Original: `IO::PackageFile::ReadPackfile`, Runtime `0x1800b6f10`.
//!
//! ```text
//! u32 header        bit31 = extended header, bits0..30 = TOC offset
//! ...file data      each entry a block stream, except `.index` stubs and raw `.str` audio
//! u32 tocSize       at TOC offset, followed by the TOC as a block stream
//! ```
//! Decoded TOC: `u32 count, u32 version, u32 gameVersion, u32 userVersion, u32 priority,
//! u32 skip, u8 canOverride, u32 crc, u32 iconW, u32 iconH, icon RGBA,
//! [v>1] u32 previewLen + bytes, [v>2] name/author/description (u32 len + bytes),
//! count x { u32 nameLen, name, u32 offset, u32 size }`.

use crate::error::{format_err, Error, Result};
use crate::stream;

/// One entry of a package: its name, and where its bytes are.
#[derive(Debug, Clone)]
pub struct Entry {
    /// Original path with backslashes, e.g. `Models\Animals\Sheep\SheepMesh.rsc:resource`... as stored.
    pub name: String,
    /// Where the entry's bytes start in the package.
    pub offset: usize,
    /// The size of the stored bytes.
    pub size: usize,
}

impl Entry {
    /// How the entry's bytes are stored.
    pub fn kind(&self) -> EntryKind {
        if self.name.ends_with(".index") {
            EntryKind::Index
        } else if self.name.ends_with(".str") || self.name.ends_with(".sav") {
            // Audio streams and the tutorials' saves (`Save\TutorialFood.sav`), stored as is.
            EntryKind::RawStream
        } else {
            EntryKind::Compressed
        }
    }

    /// Normalised resource key: forward slashes, lower case, `:` kept.
    pub fn key(&self) -> String {
        normalise(&self.name)
    }
}

/// How an entry is stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    /// 64-byte build-path stub; not a resource.
    Index,
    /// Stored uncompressed (streamed music, xWMA).
    RawStream,
    /// Block stream (`.crs`, `.sav`).
    Compressed,
}

/// The header of a package: versions, priority, and a mod's name, icon and description.
#[derive(Debug, Clone)]
pub struct Header {
    /// The package format version.
    pub version: u32,
    // +0x68; +0x6c
    /// The game version the package was made with (`0x010007ff` is 1.0.7; read with
    /// `Configuration::GetVersionString`), the author's version number (a mod's).
    pub game_version: u32,
    /// The author's version number (a mod's).
    pub user_version: u32,
    /// The priority: the entry of the package with the higher priority wins (10,000 for mods).
    pub priority: u32,
    // +0x74
    /// `Modification::RefreshList` leaves out a mod whose value is not 0 (0 in every package seen;
    /// its meaning is not traced).
    pub skip: u32,
    // +0x78/+0x7c/+0x80
    /// The icon (48×48 for mods), `0xAARRGGBB` texels.
    pub icon: (u32, u32, Vec<u8>),
    // +0x98
    /// The preview image's file (jpg or png), from version 2.
    pub preview: Vec<u8>,
    /// Whether a mod may replace this package's entries.
    pub can_override: bool,
    /// A checksum of the package.
    pub crc: u32,
    /// A mod's name.
    pub name: String,
    /// A mod's author.
    pub author: String,
    /// A mod's description.
    pub description: String,
}

/// One package: its header, its entries and its bytes.
pub struct Package {
    data: Vec<u8>,
    /// The header.
    pub header: Header,
    /// The entries, in the order of the table of contents.
    pub entries: Vec<Entry>,
}

/// The normalised form of an entry name or a reference: forward slashes, lower case.
pub fn normalise(name: &str) -> String {
    name.replace('\\', "/").to_ascii_lowercase()
}

struct Cursor<'a> {
    d: &'a [u8],
    p: usize,
}

impl<'a> Cursor<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let s = self.d.get(self.p..self.p + n).ok_or(Error::Truncated {
            at: self.p,
            wanted: n,
        })?;
        self.p += n;
        Ok(s)
    }
    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn cstr(&mut self, n: usize) -> Result<String> {
        let b = self.take(n)?;
        let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
        Ok(String::from_utf8(b[..end].to_vec())
            .unwrap_or_else(|_| b[..end].iter().map(|&c| c as char).collect()))
    }
    /// `n` bytes of UTF-16 (packages before version 3), to the first NUL.
    fn wstr(&mut self, n: usize) -> Result<String> {
        let b = self.take(n)?;
        let units: Vec<u16> = b
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .take_while(|&u| u != 0)
            .collect();
        Ok(String::from_utf16_lossy(&units))
    }
    /// A length-prefixed string: UTF-8 bytes from version 3, UTF-16 before.
    fn string(&mut self, version: u32) -> Result<String> {
        let n = self.u32()? as usize;
        if version < 3 {
            self.wstr(n)
        } else {
            self.cstr(n)
        }
    }
}

impl Package {
    /// Read a package from its bytes.
    pub fn parse(data: Vec<u8>) -> Result<Package> {
        let head = Cursor { d: &data, p: 0 }.u32()?;
        if head >> 31 == 0 {
            return Err(Error::Unsupported("legacy package header".into()));
        }
        let toc_at = (head & 0x7FFF_FFFF) as usize;
        let toc_size = Cursor {
            d: &data,
            p: toc_at,
        }
        .u32()? as usize;
        let toc_bytes = data
            .get(toc_at + 4..toc_at + 4 + toc_size)
            .ok_or(Error::Truncated {
                at: toc_at + 4,
                wanted: toc_size,
            })?;
        let toc = stream::decode(toc_bytes)?;
        let mut c = Cursor { d: &toc, p: 0 };

        // `PackageFile::ReadPackfile` (Runtime `0x1800b6f10`): the preview from version 2,
        // strings and names as UTF-8 from version 3 (UTF-16 before). Mods (`.pkm`) of every
        // version use this layout.
        let count = c.u32()? as usize;
        let version = c.u32()?;
        if version == 0 {
            return Err(Error::Unsupported(format!("package version {version}")));
        }
        let game_version = c.u32()?;
        let user_version = c.u32()?;
        let priority = c.u32()?;
        let skip = c.u32()?;
        let can_override = c.take(1)?[0] != 0;
        let crc = c.u32()?;
        let (w, h) = (c.u32()? as usize, c.u32()? as usize);
        let icon = c.take(w * h * 4)?.to_vec();
        let preview = if version > 1 {
            let n = c.u32()? as usize;
            // Some third-party packers leave the length unset (`0xe1e1e1e1`) with no image
            // after it: inferred, a length past the table is no preview.
            if n > c.d.len() - c.p {
                Vec::new()
            } else {
                c.take(n)?.to_vec()
            }
        } else {
            Vec::new()
        };
        let mut strings = [String::new(), String::new(), String::new()];
        for s in &mut strings {
            *s = c.string(version)?;
        }
        let [name, author, description] = strings;

        let mut entries = Vec::with_capacity(count);
        for _ in 0..count {
            let name = c.string(version)?;
            let offset = c.u32()? as usize;
            let size = c.u32()? as usize;
            if offset + size > data.len() {
                return format_err(format!("entry {name} extends past end of package"));
            }
            entries.push(Entry { name, offset, size });
        }
        if c.p != toc.len() {
            return format_err(format!("{} unparsed TOC bytes", toc.len() - c.p));
        }
        let header = Header {
            version,
            game_version,
            user_version,
            priority,
            skip,
            icon: (w as u32, h as u32, icon),
            preview,
            can_override,
            crc,
            name,
            author,
            description,
        };
        Ok(Package {
            data,
            header,
            entries,
        })
    }

    /// An entry's bytes as stored (still compressed for most entries).
    pub fn raw(&self, entry: &Entry) -> &[u8] {
        &self.data[entry.offset..entry.offset + entry.size]
    }

    /// Entry contents: decompressed for block streams, as stored otherwise.
    pub fn read(&self, entry: &Entry) -> Result<Vec<u8>> {
        match entry.kind() {
            EntryKind::Compressed => stream::decode(self.raw(entry)),
            EntryKind::RawStream | EntryKind::Index => Ok(self.raw(entry).to_vec()),
        }
    }
}

/// A set of packages resolved by priority: higher priority shadows lower for the same name.
#[derive(Default)]
pub struct Archive {
    packages: Vec<Package>,
    index: std::collections::HashMap<String, (usize, usize)>,
    // IO::FileSystem +0x38
    /// `IO::FileSystem`: the user pack files (enabled mods) the packages were set up with
    /// (`AddUserPackFile`, `AreUserPackFilesEnabled`).
    pub user_packs: usize,
}

impl Archive {
    /// Add one of the game's packages. An entry replaces one of the same name when its package's
    /// priority is not lower.
    pub fn add(&mut self, pkg: Package) {
        let pi = self.packages.len();
        for (ei, e) in pkg.entries.iter().enumerate() {
            if e.kind() == EntryKind::Index {
                continue;
            }
            let key = e.key();
            let replace = match self.index.get(&key) {
                Some(&(p, _)) => pkg.header.priority >= self.packages[p].header.priority,
                None => true,
            };
            if replace {
                self.index.insert(key, (pi, ei));
            }
        }
        self.packages.push(pkg);
    }

    // +0xac
    /// `FileSystem::LoadPackages` for a mod (`.pkm`), added after the game's packages: the enabled
    /// mods go in from the last in the mod list to the first, and a file replaces one already there
    /// only when that one's package allows it. So the first mod in the list wins, and nothing
    /// overrides `data1` or `patch1`.
    ///
    /// Original: Runtime `0x1800ac3b0`.
    pub fn add_mod(&mut self, pkg: Package) {
        let pi = self.packages.len();
        for (ei, e) in pkg.entries.iter().enumerate() {
            if e.kind() == EntryKind::Index {
                continue;
            }
            let key = e.key();
            let replace = match self.index.get(&key) {
                Some(&(p, _)) => self.packages[p].header.can_override,
                None => true,
            };
            if replace {
                self.index.insert(key, (pi, ei));
            }
        }
        self.packages.push(pkg);
    }

    /// How many packages are loaded (the game's first, then the mods).
    pub fn package_count(&self) -> usize {
        self.packages.len()
    }

    /// Drop every package after the first `keep` (the mods), as `FileSystem::ClosePackages`
    /// before the packages load again.
    pub fn remove_after(&mut self, keep: usize) {
        let packages: Vec<Package> = std::mem::take(&mut self.packages);
        self.index.clear();
        for p in packages.into_iter().take(keep) {
            self.add(p);
        }
    }

    /// The package holding `name`'s current entry (its header names the mod).
    pub fn package_of(&self, name: &str) -> Option<&Package> {
        self.index
            .get(&normalise(name))
            .map(|&(p, _)| &self.packages[p])
    }

    /// The number of entries.
    pub fn len(&self) -> usize {
        self.index.len()
    }

    /// Whether there are no entries.
    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }

    /// Resource names, sorted.
    pub fn names(&self) -> Vec<&str> {
        let mut v: Vec<&str> = self
            .index
            .values()
            .map(|&(p, e)| self.packages[p].entries[e].name.as_str())
            .collect();
        v.sort_unstable();
        v
    }

    /// An entry by name.
    pub fn get(&self, name: &str) -> Option<&Entry> {
        self.index
            .get(&normalise(name))
            .map(|&(p, e)| &self.packages[p].entries[e])
    }

    /// An entry's bytes, expanded, by entry name (for example `Dialog\Toolbar_resource.crs`).
    pub fn read(&self, name: &str) -> Result<Vec<u8>> {
        let &(p, e) = self
            .index
            .get(&normalise(name))
            .ok_or_else(|| Error::Format(format!("no such resource: {name}")))?;
        self.packages[p].read(&self.packages[p].entries[e])
    }

    /// Read the compiled resource behind a reference such as `Models\X\Y.rsc:resource`.
    /// The package stores it as `Models\X\Y_resource.crs`.
    pub fn read_ref(&self, reference: &str) -> Result<Vec<u8>> {
        self.read(&ref_to_entry(reference))
    }
}

/// `Dir\Name.rsc:object` -> `Dir\Name_object.crs`.
pub fn ref_to_entry(reference: &str) -> String {
    match reference.rsplit_once(':') {
        Some((path, object)) => {
            let stem = path.strip_suffix(".rsc").unwrap_or(path);
            format!("{stem}_{object}.crs")
        }
        None => reference.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reference_names() {
        assert_eq!(
            ref_to_entry(r"Models\Animals\Sheep\MaterialInstance\Sheep.rsc:resource"),
            r"Models\Animals\Sheep\MaterialInstance\Sheep_resource.crs"
        );
    }
}
