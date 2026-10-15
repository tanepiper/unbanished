//! Banished save files (`.sav`): container, game-state stream, and the fields needed to check
//! world generation. Layout notes are in docs/SAVE-FORMAT-AND-RESTORE.md.

use crate::crs::Reader;
use crate::error::{format_err, Error, Result};
use crate::stream;

pub struct Save {
    /// Expanded game-state stream (the second payload).
    pub state: Vec<u8>,
}

/// Fields the game loop writes right after the reference table and settings block
/// (`FUN_14009af60`).
#[derive(Debug, Clone)]
pub struct LoopHeader {
    pub climate: String,
    pub start_condition: String,
    pub terrain: String,
    pub seed: u32,
    pub size_index: i32,
}

/// The map component's terrain grid (`FUN_1400b59a0`).
#[derive(Debug, Clone)]
pub struct MapHeights {
    /// Samples per side (`2n + 1`).
    pub width: usize,
    pub heights: Vec<i16>,
}

/// One gameplay map cell as `FUN_14001b570` serializes it (in-memory record is 48 bytes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MapCell {
    /// Whether the two object pointers (`+0x00`, `+0x08`) are set.
    pub objects: [bool; 2],
    /// External reference id at `+0x10` (`u32::MAX` when unset).
    pub external: u32,
    /// Bytes `+0x1c..+0x25`.
    pub flags: [u8; 9],
    /// `+0x26`.
    pub value: u16,
    /// Bytes `+0x28..+0x2d`; `[1]` (`+0x29`) is the cell type.
    pub bytes: [u8; 6],
}

impl MapCell {
    pub fn cell_type(&self) -> u8 {
        self.bytes[1]
    }
}

/// The gameplay map (`FUN_1400a45d0`, serializer version 4): `size × size` cells covering the
/// centre of the terrain (the terrain grid is twice as wide).
#[derive(Debug, Clone)]
pub struct MapCells {
    pub size: usize,
    pub cells: Vec<MapCell>,
    /// The nine shorts at map `+0x90`.
    pub trailer: [i16; 9],
}

fn u32_at(b: &[u8], at: usize) -> Result<u32> {
    b.get(at..at + 4)
        .map(|s| u32::from_le_bytes(s.try_into().unwrap()))
        .ok_or(Error::Truncated { at, wanted: 4 })
}

/// One saved event-log entry (`Game +0x138`, 0x28 bytes in memory).
#[derive(Debug, Clone, PartialEq)]
pub struct SavedEvent {
    pub color: u32,
    pub text: String,
    pub kind: i32,
    pub severity: i32,
    pub cell: i32,
    pub category: i32,
    pub icon: i32,
}

/// The event log block: six notify flags (`+0x10..+0x15`), then the entries.
#[derive(Debug, Clone)]
pub struct SavedEventLog {
    pub notify: [bool; 6],
    pub entries: Vec<SavedEvent>,
}

impl Save {
    pub fn parse(data: &[u8]) -> Result<Save> {
        let version = u16::from_le_bytes([data[0], data[1]]);
        let (second_offset, second_size) = match version {
            1 => (u32_at(data, 0x14)?, u32_at(data, 0x18)?),
            2 => (u32_at(data, 0x1c)?, u32_at(data, 0x20)?),
            v => return Err(Error::Unsupported(format!("save container version {v}"))),
        };
        let (off, size) = (second_offset as usize, second_size as usize);
        if off + size != data.len() {
            return format_err("game-state payload does not end at EOF");
        }
        Ok(Save {
            state: stream::decode(&data[off..off + size])?,
        })
    }

    /// Find the event-log block (version 1: six notify bytes, a count and entries of colour,
    /// text, type, severity, cell, category, icon). Located by scanning for a block whose
    /// contents parse as exactly that and fill the block; the game-state order before it is
    /// not walked.
    pub fn event_log(&self) -> Option<SavedEventLog> {
        let d = &self.state;
        let marker = 0xaaaa_aaaau32.to_le_bytes();
        let mut at = 0;
        while let Some(p) = d[at..].windows(4).position(|w| w == marker) {
            let start = at + p;
            at = start + 4;
            if let Some(log) = parse_event_block(&d[start..]) {
                return Some(log);
            }
        }
        None
    }

    /// External reference table: (id, path) pairs.
    pub fn references(&self) -> Result<Vec<(u32, String)>> {
        let mut r = Reader::new(&self.state);
        r.block(|r, _| {
            r.block(|r, _| {
                let n = r.count()?;
                (0..n).map(|_| Ok((r.u32()?, r.string()?))).collect()
            })
            .map(|refs| {
                // Skip the rest of the outer block; only the table is needed.
                skip_rest(r);
                refs
            })
        })
    }

    pub fn loop_header(&self) -> Result<LoopHeader> {
        let refs = self.references()?;
        let name = |id: u32| {
            refs.iter()
                .find(|(i, _)| *i == id)
                .map(|(_, p)| p.clone())
                .unwrap_or_default()
        };
        let mut r = Reader::new(&self.state);
        r.block(|r, _| {
            r.skip_block()?; // reference table
            r.skip_block()?; // UI settings
            let (climate, start, terrain) = (r.u32()?, r.u32()?, r.u32()?);
            let seed = r.u32()?;
            let size_index = r.i32()?;
            skip_rest(r);
            Ok(LoopHeader {
                climate: name(climate),
                start_condition: name(start),
                terrain: name(terrain),
                seed,
                size_index,
            })
        })
    }

    /// Locate the map block by its dimension header (`2n, n, 32, 33, 2n+1, 33^2`).
    pub fn map_heights(&self) -> Result<MapHeights> {
        for n in [256i32, 384, 512] {
            let pat: Vec<u8> = [2 * n, n, 32, 33, 2 * n + 1, 33 * 33]
                .iter()
                .flat_map(|v| v.to_le_bytes())
                .collect();
            if let Some(at) = self
                .state
                .windows(pat.len())
                .position(|w| w == pat.as_slice())
            {
                let count = u32_at(&self.state, at + 24)? as usize;
                let width = (2 * n + 1) as usize;
                if count != width * width {
                    return format_err("map height count does not match its dimensions");
                }
                let start = at + 28;
                let bytes = self
                    .state
                    .get(start..start + 2 * count)
                    .ok_or(Error::Truncated {
                        at: start,
                        wanted: 2 * count,
                    })?;
                let heights = bytes
                    .chunks_exact(2)
                    .map(|c| i16::from_le_bytes([c[0], c[1]]))
                    .collect();
                return Ok(MapHeights { width, heights });
            }
        }
        format_err("no map block found")
    }
}

impl Save {
    /// Locate the gameplay map block (`0xaaaaaaaa, size, 4, n, n²`) and read its cells.
    /// `n` is half the terrain size (see `map_heights`).
    pub fn map_cells(&self) -> Result<MapCells> {
        let n = (self.map_heights()?.width - 1) / 2;
        let pat: Vec<u8> = [0xaaaa_aaaau32, 0, 4, n as u32, (n * n) as u32]
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect();
        let at = self
            .state
            .windows(pat.len())
            .position(|w| w[..4] == pat[..4] && w[8..] == pat[8..])
            .ok_or_else(|| Error::Format("no map cell block found".into()))?;
        let mut r = Reader::new(&self.state[at + 20..]);
        let ptr = |r: &mut Reader| -> Result<bool> {
            let set = r.u8()? != 0;
            if set {
                r.bytes(12)?;
            }
            Ok(set)
        };
        let mut cells = Vec::with_capacity(n * n);
        for _ in 0..n * n {
            let objects = [ptr(&mut r)?, ptr(&mut r)?];
            let external = r.u32()?;
            let flags: [u8; 9] = r.bytes(9)?.try_into().unwrap();
            let value = u16::from_le_bytes(r.bytes(2)?.try_into().unwrap());
            let bytes: [u8; 6] = r.bytes(6)?.try_into().unwrap();
            cells.push(MapCell {
                objects,
                external,
                flags,
                value,
                bytes,
            });
        }
        let mut trailer = [0i16; 9];
        for t in &mut trailer {
            *t = i16::from_le_bytes(r.bytes(2)?.try_into().unwrap());
        }
        if r.u32()? != 0xbbbb_bbbb {
            return format_err("map cell block does not end with its terminator");
        }
        Ok(MapCells {
            size: n,
            cells,
            trailer,
        })
    }
}

fn skip_rest(r: &mut Reader) {
    let rest = r.remaining();
    let _ = r.bytes(rest);
}

fn parse_event_block(b: &[u8]) -> Option<SavedEventLog> {
    let u32_at = |o: usize| {
        b.get(o..o + 4)
            .map(|s| u32::from_le_bytes(s.try_into().unwrap()))
    };
    // `aaaaaaaa | size | body | bbbbbbbb`; the body starts with the version.
    let size = u32_at(4)? as usize;
    if size < 14 || u32_at(8 + size)? != 0xbbbb_bbbb || u32_at(8)? != 1 {
        return None;
    }
    let body = &b[8..8 + size];
    if body[4..10].iter().any(|&v| v > 1) {
        return None;
    }
    let notify: [bool; 6] = std::array::from_fn(|i| body[4 + i] == 1);
    let mut o = 10;
    let rd = |o: &mut usize| -> Option<u32> {
        let v = body.get(*o..*o + 4)?;
        *o += 4;
        Some(u32::from_le_bytes(v.try_into().unwrap()))
    };
    let count = rd(&mut o)? as usize;
    if count > 100_000 {
        return None;
    }
    let mut entries = Vec::with_capacity(count);
    for _ in 0..count {
        let color = rd(&mut o)?;
        let len = rd(&mut o)?;
        if len & 0x8000_0000 == 0 {
            return None;
        }
        // The length counts the terminating NUL.
        let n = (len & 0x7fff_ffff) as usize;
        let text = body.get(o..o + n.checked_sub(1)?)?;
        o += n;
        let text = String::from_utf8_lossy(text).into_owned();
        let mut i = || rd(&mut o).map(|v| v as i32);
        entries.push(SavedEvent {
            color,
            text,
            kind: i()?,
            severity: i()?,
            cell: i()?,
            category: i()?,
            icon: i()?,
        });
    }
    (o == body.len()).then_some(SavedEventLog { notify, entries })
}
