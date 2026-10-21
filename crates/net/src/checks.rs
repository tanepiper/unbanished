//! The server's check that the players of a room still play the same game. Every few updates
//! each player sends a digest of its game state. Once every player in the room has sent the
//! digest of an update, they agree, or they differ. When they differ, the game most players have
//! is kept (when as many have one as another, the host's), and one player who has it sends the
//! others its save.
//!
//! A digest carries the player's epoch, the resyncs it has been through: a digest made before a
//! resync is not compared with one made after it, and the first digest of a newer epoch starts
//! the check again.

use std::collections::BTreeMap;

/// What a complete check found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// Every player has the same game.
    Agree,
    /// The players differ: `from` has the game to keep and sends its save to the others.
    Differ {
        /// The player who sends its save (the lowest numbered with the kept game).
        from: u8,
    },
}

/// The digests a room's players have sent, by update.
#[derive(Debug, Default)]
pub struct Checks {
    epoch: u32,
    held: BTreeMap<u64, BTreeMap<u8, u32>>,
}

impl Checks {
    /// Player `player`'s digest `crc` of `update`, made in `epoch`. `present` are the players in
    /// the room. When every one of them has sent the digest of `update`, the verdict.
    pub fn digest(
        &mut self,
        player: u8,
        epoch: u32,
        update: u64,
        crc: u32,
        present: &[u8],
    ) -> Option<(u64, Verdict)> {
        if epoch < self.epoch {
            return None;
        }
        if epoch > self.epoch {
            self.epoch = epoch;
            self.held.clear();
        }
        self.held.entry(update).or_default().insert(player, crc);
        self.complete(update, present)
    }

    /// A player left: the checks that every remaining player has now sent, with their verdicts.
    pub fn left(&mut self, present: &[u8]) -> Vec<(u64, Verdict)> {
        let updates: Vec<u64> = self.held.keys().copied().collect();
        updates
            .into_iter()
            .filter_map(|u| self.complete(u, present))
            .collect()
    }

    fn complete(&mut self, update: u64, present: &[u8]) -> Option<(u64, Verdict)> {
        let held = self.held.get(&update)?;
        if present.is_empty() || !present.iter().all(|p| held.contains_key(p)) {
            return None;
        }
        let held = self.held.remove(&update)?;
        let digests: Vec<(u8, u32)> = present.iter().map(|p| (*p, held[p])).collect();
        Some((update, verdict(&digests)))
    }
}

/// The server's own game, when it runs one: its digests are the game to keep.
pub const SERVER: u8 = u8::MAX;

/// The verdict on one update's digests, by player.
fn verdict(digests: &[(u8, u32)]) -> Verdict {
    let first = digests[0].1;
    if digests.iter().all(|&(_, c)| c == first) {
        return Verdict::Agree;
    }
    // A server that runs the game keeps its own.
    if digests.iter().any(|&(p, _)| p == SERVER) {
        return Verdict::Differ { from: SERVER };
    }
    let mut counts: BTreeMap<u32, usize> = BTreeMap::new();
    for &(_, c) in digests {
        *counts.entry(c).or_default() += 1;
    }
    let most = counts.values().copied().max().unwrap_or(0);
    // The kept game: the one most players have; among equals, the lowest-numbered player's
    // (the host's, when the host is one of them).
    let (from, _) = digests
        .iter()
        .copied()
        .filter(|(_, c)| counts[c] == most)
        .min_by_key(|&(p, _)| p)
        .expect("one digest at least");
    Verdict::Differ { from }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn players_agree_once_every_digest_is_in() {
        let mut c = Checks::default();
        assert_eq!(c.digest(0, 0, 600, 7, &[0, 1, 2]), None);
        assert_eq!(c.digest(2, 0, 600, 7, &[0, 1, 2]), None);
        assert_eq!(
            c.digest(1, 0, 600, 7, &[0, 1, 2]),
            Some((600, Verdict::Agree))
        );
    }

    #[test]
    fn the_majority_game_is_kept() {
        let mut c = Checks::default();
        c.digest(0, 0, 600, 1, &[0, 1, 2]);
        c.digest(1, 0, 600, 2, &[0, 1, 2]);
        // Players 1 and 2 agree, the host differs: player 1 sends its save.
        let v = c.digest(2, 0, 600, 2, &[0, 1, 2]);
        assert_eq!(v, Some((600, Verdict::Differ { from: 1 })));
    }

    #[test]
    fn the_servers_own_game_is_kept() {
        let mut c = Checks::default();
        let present = [0, 1, SERVER];
        c.digest(0, 0, 600, 1, &present);
        c.digest(1, 0, 600, 1, &present);
        // Both players agree with each other, and not with the server: the server's is kept.
        let v = c.digest(SERVER, 0, 600, 2, &present);
        assert_eq!(v, Some((600, Verdict::Differ { from: SERVER })));
    }

    #[test]
    fn a_tie_keeps_the_hosts_game() {
        let mut c = Checks::default();
        c.digest(1, 0, 600, 2, &[0, 1]);
        let v = c.digest(0, 0, 600, 1, &[0, 1]);
        assert_eq!(v, Some((600, Verdict::Differ { from: 0 })));
    }

    #[test]
    fn a_digest_from_before_a_resync_is_not_compared() {
        let mut c = Checks::default();
        c.digest(0, 1, 1200, 5, &[0, 1]);
        assert_eq!(c.digest(1, 0, 1200, 9, &[0, 1]), None, "an older epoch");
        assert_eq!(
            c.digest(1, 1, 1200, 5, &[0, 1]),
            Some((1200, Verdict::Agree))
        );
    }

    #[test]
    fn a_player_who_leaves_is_not_waited_for() {
        let mut c = Checks::default();
        c.digest(0, 0, 600, 3, &[0, 1, 2]);
        c.digest(1, 0, 600, 3, &[0, 1, 2]);
        assert_eq!(c.left(&[0, 1]), [(600, Verdict::Agree)]);
    }
}
