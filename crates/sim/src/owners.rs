//! A separate towns game's ownership, written beside its save (docs/SEPARATE-TOWNS.md).
//!
//! The original's save format has no owner field and must not grow one, so whose each placed
//! entity and citizen is travels in a sidecar file next to the save. A shared town game, and a
//! single-player one, has every owner 0 and needs no sidecar.

use unbanished_game::work::stock::STOCK_SLOTS;

/// The magic and version [`Owners::encode`] starts with.
const MAGIC: &[u8; 5] = b"UNBO\x01";

/// Whose each entity and citizen is, the number of players, and each player's stock limits.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Owners {
    /// The players the game has (1 in a single-player game).
    pub players: u8,
    /// Each placed entity's player, by entity index; a list shorter than the town's leaves the
    /// rest to player 0.
    pub placed: Vec<u8>,
    /// Each citizen's player, by citizen index.
    pub citizens: Vec<u8>,
    /// Each player's stock limits, [`STOCK_SLOTS`] each.
    pub limits: Vec<[i32; STOCK_SLOTS]>,
}

impl Owners {
    /// Whether the game is a separate towns one: any entity or citizen belongs to a player past
    /// the first, or the game has more than one player.
    pub fn separate(&self) -> bool {
        self.players > 1
            || self.placed.iter().any(|&o| o != 0)
            || self.citizens.iter().any(|&o| o != 0)
    }

    /// The sidecar's bytes: the magic, then the fields, little endian.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = MAGIC.to_vec();
        out.push(self.players);
        for list in [&self.placed, &self.citizens] {
            out.extend((list.len() as u32).to_le_bytes());
            out.extend(list);
        }
        out.extend((self.limits.len() as u32).to_le_bytes());
        for limits in &self.limits {
            for limit in limits {
                out.extend(limit.to_le_bytes());
            }
        }
        out
    }

    /// The sidecar [`Owners::encode`] wrote, or None for any other bytes (a truncated or older
    /// file, which the caller reads as a game with every owner 0).
    pub fn decode(bytes: &[u8]) -> Option<Owners> {
        let mut r = Reader(bytes);
        if r.take(MAGIC.len())? != MAGIC {
            return None;
        }
        let players = r.take(1)?[0];
        let placed = r.list()?;
        let citizens = r.list()?;
        let count = r.u32()? as usize;
        let mut limits = Vec::with_capacity(count);
        for _ in 0..count {
            let mut l = [0i32; STOCK_SLOTS];
            for limit in &mut l {
                *limit = r.i32()?;
            }
            limits.push(l);
        }
        Some(Owners {
            players,
            placed,
            citizens,
            limits,
        })
    }
}

struct Reader<'a>(&'a [u8]);

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        if self.0.len() < n {
            return None;
        }
        let (head, tail) = self.0.split_at(n);
        self.0 = tail;
        Some(head)
    }

    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }

    fn i32(&mut self) -> Option<i32> {
        Some(i32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }

    fn list(&mut self) -> Option<Vec<u8>> {
        let n = self.u32()? as usize;
        Some(self.take(n)?.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_was_written_reads_back() {
        let owners = Owners {
            players: 2,
            placed: vec![0, 1, 0, 1, 7],
            citizens: vec![1, 0, 1],
            limits: vec![[3; STOCK_SLOTS], [9; STOCK_SLOTS]],
        };
        let bytes = owners.encode();
        assert_eq!(Owners::decode(&bytes), Some(owners));
        assert!(!Owners::default().separate());
        let two = Owners {
            players: 2,
            ..Owners::default()
        };
        assert!(two.separate());
        // Truncated, and a file of something else entirely.
        assert_eq!(Owners::decode(&bytes[..bytes.len() - 1]), None);
        assert_eq!(Owners::decode(b"something else"), None);
    }
}
