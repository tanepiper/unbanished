//! The event log of a save: its entries and the reader of its block.

use super::*;

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

pub(super) fn skip_rest(r: &mut Reader) {
    let rest = r.remaining();
    let _ = r.bytes(rest);
}

pub(super) fn parse_event_block(b: &[u8]) -> Option<SavedEventLog> {
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
