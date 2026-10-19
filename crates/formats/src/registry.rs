//! The resources the game has loaded (`Object::ExternalFactory`'s objects), which it enumerates
//! by type (`ExternalFactory::FindNext`): the toolbar entries, professions, raw materials and
//! component descriptions (templates).
//!
//! At startup `Engine::UpdateLoop::LoadResources` (Runtime `0x1800e0020`) loads
//! `Game\GameResources.rsc` (the loop's name with `Resources.rsc`), then, for each enabled mod, the
//! mod's `<file>Resources.rsc:resource` when the packages hold it. Loading a resource loads the
//! resources it points at (`ExternalPtr`), so the loaded set is everything those roots reach.
//!
//! The port finds a resource's pointers by their encoding rather than by each type's
//! serializer: a present flag (1) then a string naming an `.rsc` object.

use std::collections::{BTreeMap, BTreeSet};

use crate::pkg::{ref_to_entry, Archive};

/// `Path\File.rsc:object` with backslashes and in lower case: the factory's map compares
/// names with `_stricmp` (`ExternalFactory::FindNext`, Runtime `0x1800c1240`), so this is
/// also its order.
fn key(reference: &str) -> String {
    let r = reference.replace('/', "\\").to_ascii_lowercase();
    if r.contains(':') {
        r
    } else {
        format!("{r}:resource")
    }
}

/// Every `Path\File.rsc[:object]` pointer in a compiled resource, in order.
pub fn references(data: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    let mut i = 0;
    while i + 5 <= data.len() {
        if data[i] != 1 {
            i += 1;
            continue;
        }
        let n = u32::from_le_bytes(data[i + 1..i + 5].try_into().unwrap());
        let found = if n & 0x8000_0000 != 0 {
            let len = (n & 0x7fff_ffff) as usize;
            data.get(i + 5..i + 5 + len).and_then(|b| {
                let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
                let s = std::str::from_utf8(&b[..end]).ok()?;
                Some((s.to_string(), 5 + len))
            })
        } else if (4..512).contains(&n) {
            let len = n as usize * 2;
            data.get(i + 5..i + 5 + len).map(|b| {
                let units: Vec<u16> = b
                    .chunks_exact(2)
                    .map(|c| u16::from_le_bytes([c[0], c[1]]))
                    .take_while(|&u| u != 0)
                    .collect();
                (String::from_utf16_lossy(&units), 5 + len)
            })
        } else {
            None
        };
        match found {
            Some((s, len)) if is_reference(&s) => {
                out.push(s);
                i += len;
            }
            _ => i += 1,
        }
    }
    out
}

/// Each resource `roots` reach (by key), with its type id and pointers, or `None` when it
/// cannot be read. Reading and scanning a resource depends only on its bytes, so each wave of
/// newly found resources is read in parallel; [`Registry::build`] then walks them in the
/// factory's order.
type Parsed = BTreeMap<String, Option<(u32, Vec<String>)>>;

fn parse_reachable(archive: &Archive, roots: &[String]) -> Parsed {
    let mut parsed = Parsed::new();
    let mut wave: Vec<String> = roots.iter().map(|r| key(r)).collect();
    while !wave.is_empty() {
        wave.sort_unstable();
        wave.dedup();
        let results = crate::par::map(&wave, |k| {
            let data = archive.read(&ref_to_entry(k)).ok()?;
            let header = crate::crs::header(&data).ok()?;
            Some((header.type_id, references(&data)))
        });
        let mut next = Vec::new();
        for (k, result) in wave.into_iter().zip(results) {
            if let Some((_, children)) = &result {
                next.extend(
                    children
                        .iter()
                        .map(|c| key(c))
                        .filter(|c| !parsed.contains_key(c)),
                );
            }
            parsed.insert(k, result);
        }
        next.retain(|c| !parsed.contains_key(c));
        wave = next;
    }
    parsed
}

fn is_reference(s: &str) -> bool {
    let path = s.split(':').next().unwrap_or("");
    path.len() > 4
        && path.to_ascii_lowercase().ends_with(".rsc")
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || " _-.\\/:()'&".contains(c))
}

/// The loaded resources: each `Path\File.rsc:object` (keyed in lower case) with its type id
/// and the reference as first written.
#[derive(Debug, Default, Clone)]
pub struct Registry {
    pub loaded: BTreeMap<String, (u32, String)>,
}

impl Registry {
    /// Load `roots` and everything they reach.
    pub fn build(archive: &Archive, roots: &[String]) -> Registry {
        let parsed = parse_reachable(archive, roots);
        let mut loaded = BTreeMap::new();
        let mut seen = BTreeSet::new();
        let mut stack: Vec<String> = roots.iter().rev().cloned().collect();
        while let Some(r) = stack.pop() {
            let k = key(&r);
            if !seen.insert(k.clone()) {
                continue;
            }
            let Some(Some((type_id, children))) = parsed.get(&k) else {
                continue;
            };
            let written = if r.contains(':') {
                r.replace('/', "\\")
            } else {
                format!("{}:resource", r.replace('/', "\\"))
            };
            loaded.insert(k, (*type_id, written));
            for child in children.iter().rev() {
                stack.push(child.clone());
            }
        }
        Registry { loaded }
    }

    /// `ExternalFactory::FindNext` over one type: the references of that type, in the
    /// factory's order (by name, without case).
    pub fn of_type(&self, type_id: u32) -> Vec<&str> {
        self.loaded
            .values()
            .filter(|(t, _)| *t == type_id)
            .map(|(_, r)| r.as_str())
            .collect()
    }

    pub fn contains(&self, reference: &str) -> bool {
        self.loaded.contains_key(&key(reference))
    }
}

/// The game's own root list and each enabled mod's (`<file>Resources.rsc:resource`), as
/// `LoadResources` names them.
pub fn roots(mod_files: &[String]) -> Vec<String> {
    let mut out = vec![r"Game\GameResources.rsc:resource".to_string()];
    out.extend(
        mod_files
            .iter()
            .map(|f| format!("{f}Resources.rsc:resource")),
    );
    out
}
