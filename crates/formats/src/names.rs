//! Banished's compact name-generation database (`.ndb`) resources.
//!
//! The database stores four-character transition records, an index of starting records, and a
//! sorted string pool of accepted names. The game uses the transitions to compose candidates and
//! rejects candidates already present in the pool. This decoder exposes both the transition data
//! and the shipped names so callers can generate valid names without reimplementing the engine's
//! candidate-selection policy.

use crate::crc::type_id;
use crate::crs::{self, Reader};
use crate::error::{format_err, Error, Result};

/// One record of a name database: a four-character state and the letters that may follow it.
#[derive(Debug, Clone, Copy)]
pub struct NameTransition {
    /// Four ASCII bytes used as a transition lookup key; short personal-name prefixes are NUL padded.
    pub prefix: [u8; 4],
    /// Nonzero when the engine treats this prefix as a word boundary.
    pub boundary: u8,
    /// Bits 0..25 permit `a`..`z`; bit 26 permits termination.
    pub next_characters: u32,
}

/// A name database (`Game/Names/*`): the records from which citizen and town names are made.
#[derive(Debug, Clone)]
pub struct NameDatabase {
    /// The transition records.
    pub transitions: Vec<NameTransition>,
    /// Indices into `transitions`, sampled by the game's generator.
    pub starting_transitions: Vec<u16>,
    /// NUL-terminated packed ASCII strings from the resource.
    pub string_pool: Vec<u8>,
    /// Byte offsets into `string_pool`, sorted by name in the shipped resources.
    pub name_offsets: Vec<u32>,
    /// Decoded names in the resource's order.
    pub names: Vec<String>,
}

impl NameDatabase {
    /// The resource class name.
    pub const CLASS: &'static str = "NameDatabase";

    /// Read a [`Self`] from the bytes of its resource.
    pub fn read(data: &[u8]) -> Result<Self> {
        let (_, value) = crs::parse(data, |r: &mut Reader, header| {
            if header.type_id != type_id(Self::CLASS) {
                return format_err(format!(
                    "expected NameDatabase, found {:#010x}",
                    header.type_id
                ));
            }
            r.block(|r, _| {
                let transition_count = r.count()?;
                let mut transitions = Vec::with_capacity(transition_count);
                for _ in 0..transition_count {
                    let prefix = r.bytes(4)?.try_into().unwrap();
                    let boundary = r.u8()?;
                    let next_characters = r.u32()?;
                    transitions.push(NameTransition {
                        prefix,
                        boundary,
                        next_characters,
                    });
                }

                let starting_count = r.count()?;
                let mut starting_transitions = Vec::with_capacity(starting_count);
                for _ in 0..starting_count {
                    starting_transitions.push(u16::from_le_bytes(r.bytes(2)?.try_into().unwrap()));
                }

                let pool_len = r.count()?;
                let string_pool = r.bytes(pool_len)?.to_vec();
                let name_count = r.count()?;
                let mut name_offsets = Vec::with_capacity(name_count);
                let mut names = Vec::with_capacity(name_count);
                for _ in 0..name_count {
                    let offset = r.u32()?;
                    let start = offset as usize;
                    let Some(rest) = string_pool.get(start..) else {
                        return Err(Error::Format(format!(
                            "name offset {offset:#x} exceeds string pool size {}",
                            string_pool.len()
                        )));
                    };
                    let Some(end) = rest.iter().position(|&b| b == 0) else {
                        return Err(Error::Format(format!(
                            "name at offset {offset:#x} is not NUL terminated"
                        )));
                    };
                    let name = std::str::from_utf8(&rest[..end])
                        .map_err(|_| Error::Format(format!("name at {offset:#x} is not UTF-8")))?;
                    names.push(name.to_owned());
                    name_offsets.push(offset);
                }

                if transitions
                    .iter()
                    .any(|entry| !entry.prefix.is_ascii() || entry.next_characters >> 27 != 0)
                {
                    return format_err("NameDatabase contains an invalid transition entry");
                }
                if starting_transitions
                    .iter()
                    .any(|&index| index as usize >= transitions.len())
                {
                    return format_err("NameDatabase starting index exceeds transition table");
                }
                if names.windows(2).any(|pair| pair[0] > pair[1]) {
                    return format_err("NameDatabase name offsets are not sorted by name");
                }

                Ok(Self {
                    transitions,
                    starting_transitions,
                    string_pool,
                    name_offsets,
                    names,
                })
            })
        })?;
        Ok(value)
    }
}
