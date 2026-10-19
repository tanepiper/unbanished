//! A window's remembered settings: the bytes a UIComponent keeps (`+0x28`, its UIDescription's
//! `+0x20` of them, 512 for the trading post and the town hall) and saves with the entity.
//!
//! `DialogInstance::SaveSettings` (Runtime `0x180116ad0`, from `OnClosed` and, for every open
//! window, `UserInterface::SaveAllSettings` before a game is saved) writes into it, and
//! `OnOpened` (`0x180117840`) reads it back when the window opens again:
//!
//! ```text
//! u32 0xdeadbeef
//! block (version 0): the dialog's settings (none), i32 count,
//!   count × { u32 controller type id, block (version 0): the controller's settings }
//! ```
//!
//! A block is `0xaaaaaaaa`, its size (from the version to the end of its contents), the
//! version, the contents, then `0xbbbbbbbb`; the rest of the buffer stays zero. Reading, each
//! saved controller goes to the window's first controller of its type; one with no match is
//! skipped. Only two controllers keep anything, each in a block of its own (version 1):
//! TradeUI (`FUN_14008bd90`) its sort and its tab, TownHallUI (`FUN_140086280`) its tab and
//! its five combos.

const MARK: u32 = 0xdead_beef;
const OPEN: u32 = 0xaaaa_aaaa;
const CLOSE: u32 = 0xbbbb_bbbb;

/// A block of `version` around `contents`.
pub fn block(version: i32, contents: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(contents.len() + 16);
    out.extend_from_slice(&OPEN.to_le_bytes());
    out.extend_from_slice(&((contents.len() + 4) as u32).to_le_bytes());
    out.extend_from_slice(&version.to_le_bytes());
    out.extend_from_slice(contents);
    out.extend_from_slice(&CLOSE.to_le_bytes());
    out
}

/// The block at the start of `data`: its version, contents and what follows it.
pub fn read_block(data: &[u8]) -> Option<(i32, &[u8], &[u8])> {
    let u32_at = |o: usize| {
        data.get(o..o + 4)
            .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
    };
    if u32_at(0)? != OPEN {
        return None;
    }
    let size = u32_at(4)? as usize;
    let version = u32_at(8)? as i32;
    let contents = data.get(12..8 + size)?;
    if u32_at(8 + size)? != CLOSE {
        return None;
    }
    Some((version, contents, &data[12 + size..]))
}

/// `SaveSettings`: each controller's type id and settings (a block of its own, or nothing),
/// in `capacity` bytes.
pub fn write(capacity: usize, controllers: &[(u32, Vec<u8>)]) -> Vec<u8> {
    let mut inner = (controllers.len() as i32).to_le_bytes().to_vec();
    for (id, settings) in controllers {
        inner.extend_from_slice(&id.to_le_bytes());
        inner.extend_from_slice(&block(0, settings));
    }
    let mut out = MARK.to_le_bytes().to_vec();
    out.extend_from_slice(&block(0, &inner));
    if out.len() <= capacity {
        out.resize(capacity, 0);
    }
    out
}

/// What `OnOpened` reads: each saved controller's type id and settings, or nothing when the
/// buffer was never written.
pub fn read(data: &[u8]) -> Option<Vec<(u32, &[u8])>> {
    if data.get(0..4)? != MARK.to_le_bytes() {
        return None;
    }
    let (_, inner, _) = read_block(&data[4..])?;
    let count = i32::from_le_bytes(inner.get(0..4)?.try_into().ok()?);
    let mut rest = &inner[4..];
    let mut out = Vec::new();
    for _ in 0..count.max(0) {
        let id = u32::from_le_bytes(rest.get(0..4)?.try_into().ok()?);
        let (_, settings, next) = read_block(&rest[4..])?;
        out.push((id, settings));
        rest = next;
    }
    Some(out)
}

/// A controller's own settings: a version-1 block of i32s.
pub fn ints(values: &[i32]) -> Vec<u8> {
    let bytes: Vec<u8> = values.iter().flat_map(|v| v.to_le_bytes()).collect();
    block(1, &bytes)
}

/// The i32s of a controller's settings block and its version.
pub fn read_ints(settings: &[u8]) -> Option<(i32, Vec<i32>)> {
    let (version, contents, _) = read_block(settings)?;
    Some((
        version,
        contents
            .chunks_exact(4)
            .map(|c| i32::from_le_bytes(c.try_into().unwrap()))
            .collect(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The layout of a trading post's settings in a save from the original (its trade window
    /// on its fourth tab, sorted by name): ten controllers, TradeUI the first.
    #[test]
    fn a_trading_post_buffer_reads_and_writes_back() {
        let ids = [
            0xc8b8_7610u32,
            0x3061_173b,
            0xbdb7_88dc,
            0xdf3a_1bb2,
            0xb998_cb9f,
            0x293e_60a1,
            0x2e56_a4c8,
            0xc93b_6026,
            0xcfd1_8e0b,
            0x2c3b_e79e,
        ];
        let controllers: Vec<(u32, Vec<u8>)> = ids
            .iter()
            .map(|&id| {
                (
                    id,
                    if id == 0xc8b8_7610 {
                        ints(&[0, 3])
                    } else {
                        Vec::new()
                    },
                )
            })
            .collect();
        let bytes = write(512, &controllers);
        assert_eq!(bytes.len(), 512);
        assert_eq!(
            &bytes[..16],
            &[0xef, 0xbe, 0xad, 0xde, 0xaa, 0xaa, 0xaa, 0xaa, 0xe8, 0, 0, 0, 0, 0, 0, 0]
        );
        let back = read(&bytes).unwrap();
        assert_eq!(back.len(), 10);
        assert_eq!(read_ints(back[0].1), Some((1, vec![0, 3])));
        assert!(back[1].1.is_empty());
        assert_eq!(read(&[0; 512]), None);
    }
}
