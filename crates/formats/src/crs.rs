//! Compiled resources (`.crs`): serialized engine objects.
//!
//! ```text
//! block    = u32 0xaaaaaaaa, u32 size, u32 version, payload, u32 0xbbbbbbbb   (size covers payload + terminator)
//! resource = block { u32 0x5412a91f, u32 typeId, u32 fileVersion,
//!                    u32 depCount, depCount x { String, u64 FILETIME }, object... }
//! String   = u32 n|0x80000000 then n bytes (8-bit, NUL included) | u32 n then n UTF-16 units
//! ```
//! Original: the constructor of `IO::BinaryWriter` (Runtime `0x1800a9980`),
//! `Object::External::Save` (Runtime `0x1800be4a0`), `System::String::Serialize` (Runtime
//! `0x1800cc1a0`).

use crate::error::{format_err, Error, Result};

/// The marker at the start of every block.
pub const BLOCK_START: u32 = 0xAAAA_AAAA;
/// The marker at the end of every block.
pub const BLOCK_END: u32 = 0xBBBB_BBBB;
/// The magic number at the start of a resource's payload.
pub const MAGIC: u32 = 0x5412_A91F;

/// A reader of a resource's fields, little-endian, that knows the block structure.
pub struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
    end: usize,
    /// Set in reference mode (saves): external pointers are `u32` ids into this table.
    reference_ids: Option<std::sync::Arc<std::collections::BTreeMap<u32, String>>>,
    /// A UTF-16 `System::String` was read (older builds write every string that way).
    pub wide_seen: bool,
}

impl<'a> Reader<'a> {
    /// A reader at the start of `data`.
    pub fn new(data: &'a [u8]) -> Self {
        Reader {
            data,
            pos: 0,
            end: data.len(),
            reference_ids: None,
            wide_seen: false,
        }
    }

    /// Read external pointers as reference ids (`ExternalFactory` reference mode, used by saves).
    pub fn set_reference_ids(
        &mut self,
        ids: std::sync::Arc<std::collections::BTreeMap<u32, String>>,
    ) {
        self.reference_ids = Some(ids);
    }

    /// The current position, in bytes.
    pub fn pos(&self) -> usize {
        self.pos
    }

    /// The number of bytes left before the end of the readable range.
    pub fn remaining(&self) -> usize {
        self.end - self.pos
    }

    /// Bytes already read or ahead, by absolute position.
    pub fn slice(&self, start: usize, end: usize) -> Result<&'a [u8]> {
        self.data.get(start..end).ok_or(Error::Truncated {
            at: start,
            wanted: end.saturating_sub(start),
        })
    }

    /// The next `n` bytes.
    pub fn bytes(&mut self, n: usize) -> Result<&'a [u8]> {
        if n > self.end - self.pos {
            return Err(Error::Truncated {
                at: self.pos,
                wanted: n,
            });
        }
        let s = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }

    fn arr<const N: usize>(&mut self) -> Result<[u8; N]> {
        Ok(self.bytes(N)?.try_into().unwrap())
    }

    /// Read a `u8`.
    pub fn u8(&mut self) -> Result<u8> {
        Ok(self.arr::<1>()?[0])
    }
    /// Read a `bool` (one byte, not zero is true).
    pub fn bool(&mut self) -> Result<bool> {
        Ok(self.u8()? != 0)
    }
    /// Read a `u32`.
    pub fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.arr()?))
    }
    /// Read an `i32`.
    pub fn i32(&mut self) -> Result<i32> {
        Ok(i32::from_le_bytes(self.arr()?))
    }
    /// Read a `u64`.
    pub fn u64(&mut self) -> Result<u64> {
        Ok(u64::from_le_bytes(self.arr()?))
    }
    /// Read an `i64`.
    pub fn i64(&mut self) -> Result<i64> {
        Ok(i64::from_le_bytes(self.arr()?))
    }
    /// Read an `i16`.
    pub fn i16(&mut self) -> Result<i16> {
        Ok(i16::from_le_bytes(self.bytes(2)?.try_into().unwrap()))
    }

    /// Read a `u16`.
    pub fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_le_bytes(self.bytes(2)?.try_into().unwrap()))
    }

    /// Narrow the readable range to end at `end` (inside the current one); returns the old
    /// end for [`Reader::unlimit`].
    pub fn limit(&mut self, end: usize) -> Result<usize> {
        if end > self.end || end < self.pos {
            return format_err(format!(
                "limit {end:#x} outside {:#x}..{:#x}",
                self.pos, self.end
            ));
        }
        Ok(std::mem::replace(&mut self.end, end))
    }

    /// Restore the readable range that [`limit`](Self::limit) returned.
    pub fn unlimit(&mut self, end: usize) {
        self.end = end;
    }

    /// Move to a position.
    pub fn set_pos(&mut self, pos: usize) {
        self.pos = pos;
    }

    /// The `u32` at an absolute offset.
    pub fn u32_at(&self, at: usize) -> Option<u32> {
        self.data
            .get(at..at + 4)
            .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
    }

    /// The next `u32` without consuming it (`None` at the end).
    pub fn peek_u32(&self) -> Option<u32> {
        self.data
            .get(self.pos..self.pos + 4)
            .filter(|_| self.pos + 4 <= self.end)
            .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
    }

    /// A `u32` count of UTF-16 units, the terminating NUL included (0 for an empty string), then
    /// the units.
    ///
    /// Original: `System::String::Serialize`, Runtime `0x1800cc1a0`.
    pub fn wstring(&mut self) -> Result<String> {
        let n = self.u32()? as usize;
        let b = self.bytes(n * 2)?;
        let units: Vec<u16> = b
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        let end = units.iter().position(|&c| c == 0).unwrap_or(units.len());
        Ok(String::from_utf16_lossy(&units[..end]))
    }

    /// Read an `f32`.
    pub fn f32(&mut self) -> Result<f32> {
        Ok(f32::from_le_bytes(self.arr()?))
    }
    /// Read `N` `f32` values.
    pub fn f32s<const N: usize>(&mut self) -> Result<[f32; N]> {
        let mut out = [0.0; N];
        for v in &mut out {
            *v = self.f32()?;
        }
        Ok(out)
    }

    /// Element count; rejects negative values.
    pub fn count(&mut self) -> Result<usize> {
        let n = self.i32()?;
        usize::try_from(n).map_err(|_| Error::Format(format!("negative count {n}")))
    }

    /// Read a string: a length with bit 31 set for 8-bit text (else UTF-16), then the characters
    /// with the terminating zero.
    pub fn string(&mut self) -> Result<String> {
        let n = self.u32()?;
        if n & 0x8000_0000 != 0 {
            let b = self.bytes((n & 0x7FFF_FFFF) as usize)?;
            let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
            // 8-bit strings hold UTF-8 (e.g. the build notice's `©`); fall back to Latin-1.
            Ok(match std::str::from_utf8(&b[..end]) {
                Ok(s) => s.to_owned(),
                Err(_) => b[..end].iter().map(|&c| c as char).collect(),
            })
        } else {
            self.wide_seen = true;
            let b = self.bytes(n as usize * 2)?;
            let units: Vec<u16> = b
                .chunks_exact(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect();
            let end = units.iter().position(|&c| c == 0).unwrap_or(units.len());
            Ok(String::from_utf16_lossy(&units[..end]))
        }
    }

    /// `ExternalPtr`: bool present, then a `Path\File.rsc:object` reference.
    pub fn external_ref(&mut self) -> Result<Option<String>> {
        if let Some(ids) = self.reference_ids.clone() {
            let at = self.pos;
            let id = self.u32()?;
            if id == u32::MAX {
                return Ok(None);
            }
            return match ids.get(&id) {
                Some(path) => Ok(Some(path.clone())),
                None => format_err(format!("unknown reference id {id} at {at:#x}")),
            };
        }
        if self.bool()? {
            Ok(Some(self.string()?))
        } else {
            Ok(None)
        }
    }

    /// Read one block, running `f` over its payload. Fails unless `f` consumes it exactly.
    pub fn block<T>(&mut self, f: impl FnOnce(&mut Self, u32) -> Result<T>) -> Result<T> {
        let start = self.pos;
        if self.u32()? != BLOCK_START {
            return format_err(format!("expected block marker at {start:#x}"));
        }
        let size = self.u32()? as usize;
        let version = self.u32()?;
        let end = self.pos + size;
        if size < 4 || end > self.end {
            return format_err(format!("block at {start:#x} (size {size}) overruns parent"));
        }
        if u32::from_le_bytes(self.data[end - 4..end].try_into().unwrap()) != BLOCK_END {
            return format_err(format!("block at {start:#x} lacks terminator"));
        }
        let outer_end = std::mem::replace(&mut self.end, end - 4);
        let result = f(self, version);
        let consumed = self.pos;
        self.end = outer_end;
        let value = result?;
        if consumed != end - 4 {
            return format_err(format!(
                "block at {start:#x} has {} unread bytes",
                end - 4 - consumed
            ));
        }
        self.pos = end;
        Ok(value)
    }

    /// Skip the rest of the current block (for partially decoded types).
    pub fn skip_rest(&mut self) {
        self.pos = self.end;
    }

    /// Skip a block without interpreting it.
    pub fn skip_block(&mut self) -> Result<()> {
        self.block(|r, _| {
            r.pos = r.end;
            Ok(())
        })
    }
}

/// The header of a resource.
#[derive(Debug, Clone)]
pub struct Header {
    /// The id of the object's type.
    pub type_id: u32,
    /// The file version.
    pub file_version: u32,
    /// The resources it depends on.
    pub dependencies: Vec<String>,
}

/// Parse a resource, handing the object payload to `f` (called inside the outer block).
pub fn parse<T>(
    data: &[u8],
    f: impl FnOnce(&mut Reader, &Header) -> Result<T>,
) -> Result<(Header, T)> {
    let mut r = Reader::new(data);
    let out = r.block(|r, _| {
        if r.u32()? != MAGIC {
            return format_err("missing resource magic");
        }
        let type_id = r.u32()?;
        let file_version = r.u32()?;
        let n = r.count()?;
        let mut dependencies = Vec::with_capacity(n);
        for _ in 0..n {
            dependencies.push(r.string()?);
            r.u64()?; // source FILETIME
        }
        let header = Header {
            type_id,
            file_version,
            dependencies,
        };
        let value = f(r, &header)?;
        Ok((header, value))
    })?;
    if r.pos != data.len() {
        return format_err(format!(
            "{} trailing bytes after resource",
            data.len() - r.pos
        ));
    }
    Ok(out)
}

/// Header only; skips the object payload.
pub fn header(data: &[u8]) -> Result<Header> {
    parse(data, |r, _| {
        r.pos = r.end;
        Ok(())
    })
    .map(|(h, ())| h)
}

/// The writing side of [`Reader`] (`IO::BinaryWriter`): little-endian values, 8-bit strings and
/// blocks whose size is filled in when they close.
#[derive(Default)]
pub struct Writer {
    /// The bytes written.
    pub data: Vec<u8>,
    /// Set in reference mode (saves): external pointers are written as ids from this table.
    reference_ids: Option<std::collections::BTreeMap<String, u32>>,
    /// Write `System::String`s as UTF-16 (as older builds did) instead of 8-bit.
    pub wide_strings: bool,
}

impl Writer {
    /// An empty writer.
    pub fn new() -> Self {
        Self::default()
    }

    /// Write external pointers as reference ids (the inverse of the reference table).
    pub fn set_reference_ids(&mut self, ids: &std::collections::BTreeMap<u32, String>) {
        self.reference_ids = Some(ids.iter().map(|(&id, p)| (p.clone(), id)).collect());
    }

    /// Write bytes as they are.
    pub fn bytes(&mut self, b: &[u8]) {
        self.data.extend_from_slice(b);
    }
    /// Write a `u8`.
    pub fn u8(&mut self, v: u8) {
        self.data.push(v);
    }
    /// Write a `bool` as one byte.
    pub fn bool(&mut self, v: bool) {
        self.data.push(v as u8);
    }
    /// Write an `i16`.
    pub fn i16(&mut self, v: i16) {
        self.bytes(&v.to_le_bytes());
    }
    /// Write a `u16`.
    pub fn u16(&mut self, v: u16) {
        self.bytes(&v.to_le_bytes());
    }
    /// Write a `u32`.
    pub fn u32(&mut self, v: u32) {
        self.bytes(&v.to_le_bytes());
    }
    /// Write an `i32`.
    pub fn i32(&mut self, v: i32) {
        self.bytes(&v.to_le_bytes());
    }
    /// Write an `i64`.
    pub fn i64(&mut self, v: i64) {
        self.bytes(&v.to_le_bytes());
    }
    /// Write an `f32`.
    pub fn f32(&mut self, v: f32) {
        self.bytes(&v.to_le_bytes());
    }

    /// A count as the reader's `count` takes it.
    pub fn count(&mut self, n: usize) {
        self.i32(n as i32);
    }

    /// `System::String` with the 8-bit flag (or UTF-16 with [`Writer::wide_strings`]); the
    /// length counts the NUL.
    pub fn string(&mut self, s: &str) {
        if self.wide_strings {
            self.wstring(s);
            return;
        }
        self.u32((s.len() as u32 + 1) | 0x8000_0000);
        self.bytes(s.as_bytes());
        self.u8(0);
    }

    /// A UTF-16 string: the unit count with its NUL, then the units.
    pub fn wstring(&mut self, s: &str) {
        let units: Vec<u16> = s.encode_utf16().chain([0]).collect();
        self.u32(units.len() as u32);
        for u in units {
            self.u16(u);
        }
    }

    /// `ExternalPtr`, in reference mode an id (`u32::MAX` for none), else a present flag and
    /// the path.
    pub fn external_ref(&mut self, path: Option<&str>) -> Result<()> {
        if let Some(ids) = &self.reference_ids {
            let id = match path {
                None => u32::MAX,
                Some(p) => *ids
                    .get(p)
                    .ok_or_else(|| Error::Format(format!("no reference id for {p}")))?,
            };
            self.u32(id);
            return Ok(());
        }
        self.bool(path.is_some());
        if let Some(p) = path {
            self.string(p);
        }
        Ok(())
    }

    /// Open a block at `version`; returns where its size goes, for [`Writer::end_block`].
    pub fn begin_block(&mut self, version: u32) -> usize {
        self.u32(BLOCK_START);
        let size_at = self.data.len();
        self.u32(0);
        self.u32(version);
        size_at
    }

    /// Close the block opened at `size_at`. The size covers the payload and terminator, not
    /// the version.
    pub fn end_block(&mut self, size_at: usize) {
        self.u32(BLOCK_END);
        let size = (self.data.len() - size_at - 8) as u32;
        self.data[size_at..size_at + 4].copy_from_slice(&size.to_le_bytes());
    }

    /// One block at `version`, its payload written by `f`.
    pub fn block<T>(&mut self, version: u32, f: impl FnOnce(&mut Self) -> Result<T>) -> Result<T> {
        let at = self.begin_block(version);
        let out = f(self)?;
        self.end_block(at);
        Ok(out)
    }
}
