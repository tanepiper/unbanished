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

/// `Path\File.rsc:object` with the delimiters and case the package index uses.
fn key(reference: &str) -> String {
    let r = reference.replace('/', "\\");
    match r.rsplit_once(':') {
        Some((path, object)) => format!("{}:{}", path.to_ascii_lowercase(), object),
        None => format!("{}:resource", r.to_ascii_lowercase()),
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

fn is_reference(s: &str) -> bool {
    let path = s.split(':').next().unwrap_or("");
    path.len() > 4
        && path.to_ascii_lowercase().ends_with(".rsc")
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || " _-.\\/:()'&".contains(c))
}

/// The loaded resources: each `Path\File.rsc:object` (keyed lower-case path, original object)
/// with its type id and the reference as first written.
#[derive(Debug, Default, Clone)]
pub struct Registry {
    pub loaded: BTreeMap<String, (u32, String)>,
}

impl Registry {
    /// Load `roots` and everything they reach.
    pub fn build(archive: &Archive, roots: &[String]) -> Registry {
        let mut loaded = BTreeMap::new();
        let mut seen = BTreeSet::new();
        let mut stack: Vec<String> = roots.iter().rev().cloned().collect();
        while let Some(r) = stack.pop() {
            let k = key(&r);
            if !seen.insert(k.clone()) {
                continue;
            }
            let Ok(data) = archive.read(&ref_to_entry(&k)) else {
                continue;
            };
            let Ok(header) = crate::crs::header(&data) else {
                continue;
            };
            let written = if r.contains(':') {
                r.replace('/', "\\")
            } else {
                format!("{}:resource", r.replace('/', "\\"))
            };
            loaded.insert(k, (header.type_id, written));
            for child in references(&data).into_iter().rev() {
                stack.push(child);
            }
        }
        Registry { loaded }
    }

    /// `ExternalFactory::FindNext` over one type: the references of that type, sorted.
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
