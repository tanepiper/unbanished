//! Compiled resources (`.crs`): serialized engine objects.
//!
//! ```text
//! block    = u32 0xaaaaaaaa, u32 size, u32 version, payload, u32 0xbbbbbbbb   (size covers payload + terminator)
//! resource = block { u32 0x5412a91f, u32 typeId, u32 fileVersion,
//!                    u32 depCount, depCount x { String, u64 FILETIME }, object... }
//! String   = u32 n|0x80000000 then n bytes (8-bit, NUL included) | u32 n then n UTF-16 units
//! ```
//! Sources: `IO::BinaryWriter` ctor (0x1800a9980), `Object::External::Save` (0x1800be4a0),
//! `System::String::Serialize` (0x1800cc1a0).

use crate::error::{format_err, Error, Result};

pub const BLOCK_START: u32 = 0xAAAA_AAAA;
pub const BLOCK_END: u32 = 0xBBBB_BBBB;
pub const MAGIC: u32 = 0x5412_A91F;

pub struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
    end: usize,
}

impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Reader {
            data,
            pos: 0,
            end: data.len(),
        }
    }

    pub fn pos(&self) -> usize {
        self.pos
    }

    pub fn remaining(&self) -> usize {
        self.end - self.pos
    }

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

    pub fn u8(&mut self) -> Result<u8> {
        Ok(self.arr::<1>()?[0])
    }
    pub fn bool(&mut self) -> Result<bool> {
        Ok(self.u8()? != 0)
    }
    pub fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.arr()?))
    }
    pub fn i32(&mut self) -> Result<i32> {
        Ok(i32::from_le_bytes(self.arr()?))
    }
    pub fn u64(&mut self) -> Result<u64> {
        Ok(u64::from_le_bytes(self.arr()?))
    }
    pub fn i64(&mut self) -> Result<i64> {
        Ok(i64::from_le_bytes(self.arr()?))
    }
    pub fn f32(&mut self) -> Result<f32> {
        Ok(f32::from_le_bytes(self.arr()?))
    }
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

    pub fn string(&mut self) -> Result<String> {
        let n = self.u32()?;
        if n & 0x8000_0000 != 0 {
            let b = self.bytes((n & 0x7FFF_FFFF) as usize)?;
            let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
            Ok(b[..end].iter().map(|&c| c as char).collect())
        } else {
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

    /// Skip a block without interpreting it.
    pub fn skip_block(&mut self) -> Result<()> {
        self.block(|r, _| {
            r.pos = r.end;
            Ok(())
        })
    }
}

#[derive(Debug, Clone)]
pub struct Header {
    pub type_id: u32,
    pub file_version: u32,
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
