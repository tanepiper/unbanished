//! Banished save files (`.sav`): container, game-state stream, and the fields needed to check
//! world generation. Layout notes are in docs/SAVE-FORMAT-AND-RESTORE.md.

use crate::crs::{Reader, Writer};
use crate::error::{format_err, Error, Result};
use crate::stream;

/// A save file: its two payloads, expanded. See the [module documentation](self).
pub struct Save {
    /// Expanded metadata stream (the first payload; see [`SaveMetadata`]).
    pub metadata: Vec<u8>,
    /// Expanded game-state stream (the second payload).
    pub state: Vec<u8>,
}

/// The first payload (`SaveLoad::SaveMetaData`): what the load dialog shows. One block at version
/// 3.
///
/// Original: Runtime `0x1800c3d00`.
#[derive(Debug, Clone, PartialEq)]
pub struct SaveMetadata {
    /// The town's name, as the Load dialog shows it.
    pub name: String,
    /// The preview picture's width in pixels.
    pub width: i32,
    /// The preview picture's height in pixels.
    pub height: i32,
    /// `width × height` 4-byte texels (blue, green, red, unused) of the preview.
    pub image: Vec<u8>,
    /// From version 3: an unnamed value (2 in the samples), then the active mods.
    pub value: i32,
    /// The mods that were on when the town was saved.
    pub mods: Vec<SavedMod>,
    /// The strings were UTF-16 (2015 builds).
    pub wide: bool,
}

/// A mod active when the game was saved: its name, its `.pkm` file and three values.
#[derive(Debug, Clone, PartialEq)]
pub struct SavedMod {
    /// The mod's name.
    pub name: String,
    /// The mod's file name.
    pub file: String,
    /// Three values saved with the mod (not identified yet).
    pub values: [u32; 3],
}

impl SaveMetadata {
    /// Read a [`SaveMetadata`] from the bytes of its resource.
    pub fn parse(data: &[u8]) -> Result<SaveMetadata> {
        let mut r = Reader::new(data);
        let mut meta = r.block(|r, version| {
            let name = r.string()?;
            let width = r.i32()?;
            let height = r.i32()?;
            let image = r
                .bytes(width.max(0) as usize * height.max(0) as usize * 4)?
                .to_vec();
            let (value, mods) = if version > 2 {
                let value = r.i32()?;
                let mods = (0..r.count()?)
                    .map(|_| {
                        Ok(SavedMod {
                            name: r.string()?,
                            file: r.string()?,
                            values: [r.u32()?, r.u32()?, r.u32()?],
                        })
                    })
                    .collect::<Result<_>>()?;
                (value, mods)
            } else {
                (0, Vec::new())
            };
            Ok(SaveMetadata {
                name,
                width,
                height,
                image,
                value,
                mods,
                wide: false,
            })
        })?;
        meta.wide = r.wide_seen;
        Ok(meta)
    }

    /// The block at version 3.
    pub fn write(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.wide_strings = self.wide;
        let at = w.begin_block(3);
        w.string(&self.name);
        w.i32(self.width);
        w.i32(self.height);
        w.bytes(&self.image);
        w.i32(self.value);
        w.count(self.mods.len());
        for m in &self.mods {
            w.string(&m.name);
            w.string(&m.file);
            m.values.iter().for_each(|&v| w.u32(v));
        }
        w.end_block(at);
        w.data
    }
}

/// The file header's check value (`IO::Persistence`'s writer, Runtime around `0x1800b9500`):
/// CRC-32 steps from `0xffffffff` over the block table, complemented, then over each block in
/// 32 KiB pieces, complemented after each piece.
fn header_check(table: &[u8], blocks: &[&[u8]]) -> u32 {
    let step = |c: u32, data: &[u8]| !crate::crc::crc32_raw(c, data);
    let mut c = step(!0, table);
    for b in blocks {
        for piece in b.chunks(stream::BLOCK_SIZE) {
            c = step(c, piece);
        }
    }
    c
}

/// Fields the game loop writes right after the reference table and settings block.
///
/// Original: `FUN_14009af60`.
#[derive(Debug, Clone)]
pub struct LoopHeader {
    /// The climate profile's reference.
    pub climate: String,
    /// The start condition's reference.
    pub start_condition: String,
    /// The terrain profile's reference.
    pub terrain: String,
    /// The map seed.
    pub seed: u32,
    /// The map size: 0 small, 1 medium, 2 large.
    pub size_index: i32,
}

/// The map component's terrain grid.
///
/// Original: `FUN_1400b59a0`.
#[derive(Debug, Clone)]
pub struct MapHeights {
    /// Samples per side (`2n + 1`).
    pub width: usize,
    /// The heights, row by row: height × 256.
    pub heights: Vec<i16>,
}

/// One cell of the gameplay map, as a save stores it (the cell is 48 bytes in memory).
///
/// Original: `FUN_14001b570`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MapCell {
    // +0x00; +0x08
    /// Whether the two object pointers are set.
    pub objects: [bool; 2],
    // +0x10
    /// External reference id at (`u32::MAX` when unset).
    pub external: u32,
    // +0x1c..+0x25
    /// Bytes.
    pub flags: [u8; 9],
    // +0x26
    /// Not identified yet.
    pub value: u16,
    // +0x29; +0x28..+0x2d
    /// Bytes; `[1]` is the cell type.
    pub bytes: [u8; 6],
}

impl MapCell {
    /// The cell's type (see `banished_terrain::CellType`).
    pub fn cell_type(&self) -> u8 {
        self.bytes[1]
    }
}

/// The gameplay map (serializer version 4): `size × size` cells covering the centre of the terrain
/// (the terrain grid is twice as wide).
///
/// Original: `FUN_1400a45d0`.
#[derive(Debug, Clone)]
pub struct MapCells {
    /// The number of cells on each side.
    pub size: usize,
    /// The cells, row by row.
    pub cells: Vec<MapCell>,
    // +0x90
    /// The nine values that end the map block.
    pub trailer: [i16; 9],
}

fn u32_at(b: &[u8], at: usize) -> Result<u32> {
    b.get(at..at + 4)
        .map(|s| u32::from_le_bytes(s.try_into().unwrap()))
        .ok_or(Error::Truncated { at, wanted: 4 })
}

// Game +0x138
/// One saved event-log entry (`Game`, 0x28 bytes in memory).
#[derive(Debug, Clone, PartialEq)]
pub struct SavedEvent {
    /// The colour of the entry, `0xAARRGGBB`.
    pub color: u32,
    /// The text.
    pub text: String,
    /// The kind of event.
    pub kind: i32,
    /// How serious the event is.
    pub severity: i32,
    /// The map cell it happened at (-1 for none).
    pub cell: i32,
    /// The category the log filters by.
    pub category: i32,
    /// The icon.
    pub icon: i32,
}

// +0x10..+0x15
/// The event log block: six notify flags, then the entries.
#[derive(Debug, Clone)]
pub struct SavedEventLog {
    /// Which categories show a notification.
    pub notify: [bool; 6],
    /// The entries, oldest first.
    pub entries: Vec<SavedEvent>,
}

impl Save {
    /// Read a [`Save`] from the bytes of its resource.
    pub fn parse(data: &[u8]) -> Result<Save> {
        let (meta, state) = Save::payloads(data)?;
        Ok(Save {
            metadata: stream::decode(meta)?,
            state: stream::decode(state)?,
        })
    }

    /// Only the metadata, as the save list reads it: the game state is not expanded.
    ///
    /// Original: `Persistence::BeginGetSaveGameInfo`, then `SaveLoad::LoadMetaData` (Runtime
    /// `0x1800c43a0`).
    pub fn read_metadata(data: &[u8]) -> Result<SaveMetadata> {
        SaveMetadata::parse(&stream::decode(Save::payloads(data)?.0)?)
    }

    /// The compressed metadata and game-state payloads, checked against the header.
    fn payloads(data: &[u8]) -> Result<(&[u8], &[u8])> {
        if data.len() < 2 {
            return Err(Error::Truncated { at: 0, wanted: 2 });
        }
        let version = u16::from_le_bytes([data[0], data[1]]);
        // Version 1: the first payload's offset and size at 0x08, the second's id, offset and
        // size at 0x10. Version 2: `u16 2, u16 count, 0x80808080, u32 check`, then `{id,
        // offset, size}` per payload from 0x0c.
        let (first, second) = match version {
            1 => ((0x08, 0x0c), (0x14, 0x18)),
            2 => ((0x10, 0x14), (0x1c, 0x20)),
            v => return Err(Error::Unsupported(format!("save container version {v}"))),
        };
        let payload = |(o, s): (usize, usize)| -> Result<&[u8]> {
            let (off, size) = (u32_at(data, o)? as usize, u32_at(data, s)? as usize);
            data.get(off..off + size).ok_or(Error::Truncated {
                at: off,
                wanted: size,
            })
        };
        let (meta, state) = (payload(first)?, payload(second)?);
        if state.as_ptr_range().end != data.as_ptr_range().end {
            return format_err("game-state payload does not end at EOF");
        }
        if version == 2 && u32_at(data, 4)? == 0x8080_8080 {
            let count = u16::from_le_bytes([data[2], data[3]]) as usize;
            let table = data.get(12..12 + 12 * count).ok_or(Error::Truncated {
                at: 12,
                wanted: 12 * count,
            })?;
            if header_check(table, &[meta, state]) != u32_at(data, 8)? {
                return format_err("save check value does not match");
            }
        }
        Ok((meta, state))
    }

    /// The metadata: the town's name, the preview picture and the mods.
    pub fn metadata(&self) -> Result<SaveMetadata> {
        SaveMetadata::parse(&self.metadata)
    }

    /// A version 2 save file: the metadata as payload `0x24` and the game state as payload 1,
    /// each compressed (`IO::CompressedStream`), behind the checked header.
    pub fn write(metadata: &SaveMetadata, state: &[u8]) -> Vec<u8> {
        let meta = stream::encode(&metadata.write());
        let state = stream::encode(state);
        const HEADER: usize = 12 + 2 * 12;
        let mut table = Vec::new();
        for (id, off, size) in [
            (0, HEADER, meta.len()),
            (1, HEADER + meta.len(), state.len()),
        ] {
            for v in [id as u32, off as u32, size as u32] {
                table.extend_from_slice(&v.to_le_bytes());
            }
        }
        let mut out = Vec::with_capacity(HEADER + meta.len() + state.len());
        out.extend_from_slice(&2u16.to_le_bytes());
        out.extend_from_slice(&2u16.to_le_bytes());
        out.extend_from_slice(&0x8080_8080u32.to_le_bytes());
        out.extend_from_slice(&header_check(&table, &[&meta, &state]).to_le_bytes());
        out.extend_from_slice(&table);
        out.extend_from_slice(&meta);
        out.extend_from_slice(&state);
        out
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

    /// The game loop's header: the seed, the map size, and the terrain, climate and start condition.
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
        // The length counts the terminating NUL; without the 8-bit flag the text is UTF-16
        // (2015 builds).
        let text = if len & 0x8000_0000 != 0 {
            let n = (len & 0x7fff_ffff) as usize;
            let text = body.get(o..o + n.checked_sub(1)?)?;
            o += n;
            String::from_utf8_lossy(text).into_owned()
        } else {
            let n = len as usize;
            let units: Vec<u16> = body
                .get(o..o + 2 * n.checked_sub(1)?)?
                .chunks_exact(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect();
            o += 2 * n;
            String::from_utf16_lossy(&units)
        };
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
