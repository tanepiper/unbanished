//! Restoring the piles of raw material from a save.

use super::*;

impl StartingTown {
    /// RawMaterialComponent entities: loose piles, their Pickup work set by the job restore.
    pub(super) fn restore_piles(
        &mut self,
        archive: &Archive,
        saved: &SavedGame,
        index: &BTreeMap<ObjId, usize>,
    ) {
        for (id, entity) in saved.entities() {
            let Some(&p) = index.get(&id) else { continue };
            let Some(raw) = saved.component(entity, "RawMaterialComponent") else {
                continue;
            };
            let reference = self.templates[self.placed[p].template].reference.clone();
            let Some(material) = material_index(archive, &mut self.materials, &reference) else {
                log::warn!("{reference}: no raw material");
                continue;
            };
            let work = saved
                .component(entity, "WorkComponent")
                .map_or(-1, |w| int(&w.fields, "work.work_id"));
            self.construction.piles.insert(
                p,
                Pile {
                    material,
                    count: int(&raw.fields, "raw.count"),
                    age: time(&raw.fields, "raw.age"),
                    work,
                    mask: unbanished_game::town::pile_mask(self.materials[material].flags),
                    priority: None,
                    auto_pickup: None,
                    state: 0,
                    claimer: None,
                },
            );
        }
    }
}
