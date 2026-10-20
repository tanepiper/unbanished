//! The laborer, child and student professions, for their names (not in the town's
//! Professions list, which holds the workplaces' professions).

use super::*;

impl StartingTown {
    /// `Game/Profession/Profession.rsc:<key>` (read on demand; presentation only).
    pub fn profession(&self, key: &str) -> Option<unbanished_formats::game::Profession> {
        self.archive_professions
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, p)| p.clone())
    }
}
