//! The stock table a rule reads: the acting player's counts and limits.

use super::Sim;

impl Sim<'_, '_> {
    /// The stock table of the player whose entity `entity` is: the (count, limit) per slot the
    /// last recount made for that player.
    pub(crate) fn stock_of(&self, entity: usize) -> &[(i32, i32); 21] {
        let owner = self.placed.get(entity).map_or(0, |p| p.owner);
        self.stock.get(usize::from(owner)).unwrap_or(&ZERO)
    }

    /// The stock table of the player whose citizen `citizen` is.
    pub(crate) fn stock_of_citizen(&self, citizen: usize) -> &[(i32, i32); 21] {
        let owner = self.owners.get(citizen).copied().unwrap_or(0);
        self.stock.get(usize::from(owner)).unwrap_or(&ZERO)
    }
}

/// A player with no table (out of range), which never limits anything.
const ZERO: [(i32, i32); 21] = [(0, 0); 21];
