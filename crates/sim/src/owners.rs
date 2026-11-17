//! A separate towns game's ownership, written beside its save (docs/SEPARATE-TOWNS.md).
//! Deviation: not in the original, which has no multiplayer and no owners to keep.
//!
//! The original's save format has no owner field and must not grow one, so whose each placed
//! entity and citizen is travels in a sidecar file next to the save. A shared town game, and a
//! single-player one, has every owner 0 and needs no sidecar.
//!
//! Each owner is keyed by the saved object itself, not its position: a save drops the entities
//! that died and moves the last ones into their places, so positions mean different things
//! before and after a load.

use unbanished_formats::savegame::ObjId;
use unbanished_game::work::stock::STOCK_SLOTS;

/// The magic and version [`Owners::encode`] starts with.
const MAGIC: &[u8; 5] = b"UNBO\x02";

/// Whose each entity and citizen is, the number of players, and each player's stock limits.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Owners {
    /// The players the game has (1 in a single-player game).
    pub players: u8,
    /// Each placed entity's player, by the entity's object id in the save; an entity the list
    /// does not name belongs to player 0.
    pub placed: Vec<(ObjId, u8)>,
    /// Each citizen's player, by the citizen's object id in the save.
    pub citizens: Vec<(ObjId, u8)>,
    /// Each player's stock limits, [`STOCK_SLOTS`] each.
    pub limits: Vec<[i32; STOCK_SLOTS]>,
    /// Each player's start cell on the map, for the camera and the interface (empty when the
    /// sidecar predates them).
    pub starts: Vec<(i32, i32)>,
}

impl Owners {
    /// Whether the game is a separate towns one: any entity or citizen belongs to a player past
    /// the first, or the game has more than one player.
    pub fn separate(&self) -> bool {
        self.players > 1
            || self.placed.iter().any(|&(_, o)| o != 0)
            || self.citizens.iter().any(|&(_, o)| o != 0)
    }

    /// The sidecar's bytes: the magic, then the fields, little endian.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = MAGIC.to_vec();
        out.push(self.players);
        for list in [&self.placed, &self.citizens] {
            out.extend((list.len() as u32).to_le_bytes());
            for &(id, owner) in list {
                out.extend(id.to_le_bytes());
                out.push(owner);
            }
        }
        out.extend((self.limits.len() as u32).to_le_bytes());
        for limits in &self.limits {
            for limit in limits {
                out.extend(limit.to_le_bytes());
            }
        }
        out.extend((self.starts.len() as u32).to_le_bytes());
        for (x, y) in &self.starts {
            out.extend(x.to_le_bytes());
            out.extend(y.to_le_bytes());
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
        let placed = r.pairs()?;
        let citizens = r.pairs()?;
        let count = r.u32()? as usize;
        // A corrupt length must not ask for an allocation the file cannot hold.
        if count > 64 {
            return None;
        }
        let mut limits = Vec::with_capacity(count);
        for _ in 0..count {
            let mut l = [0i32; STOCK_SLOTS];
            for limit in &mut l {
                *limit = r.i32()?;
            }
            limits.push(l);
        }
        let count = r.u32()? as usize;
        if count > 64 || r.0.len() < count * 8 {
            return None;
        }
        let mut starts = Vec::with_capacity(count);
        for _ in 0..count {
            starts.push((r.i32()?, r.i32()?));
        }
        Some(Owners {
            players,
            placed,
            citizens,
            limits,
            starts,
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

    /// A list of `(object, owner)` pairs.
    fn pairs(&mut self) -> Option<Vec<(ObjId, u8)>> {
        let n = self.u32()? as usize;
        // Each pair is five bytes: a corrupt count must not ask for a huge allocation.
        if self.0.len() < n.checked_mul(5)? {
            return None;
        }
        let mut out = Vec::with_capacity(n);
        for _ in 0..n {
            let id = self.i32()?;
            out.push((id, self.take(1)?[0]));
        }
        Some(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_was_written_reads_back() {
        let owners = Owners {
            players: 2,
            placed: vec![(10, 0), (11, 1), (12, 0), (13, 1)],
            citizens: vec![(20, 1), (21, 0), (22, 1)],
            limits: vec![[3; STOCK_SLOTS], [9; STOCK_SLOTS]],
            starts: vec![(10, 20), (30, 40)],
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
        // A list whose count outruns the file is refused, not allocated.
        let mut huge = bytes.clone();
        huge[6..10].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(Owners::decode(&huge), None);
    }
}
