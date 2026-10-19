//! What the agents see of the town: the map sync, entity views and storages.

use super::*;

/// Entities placed during the update (piles, plantings) join the views; the original reads
/// its entities live.
pub(super) fn refresh_views(entities: &mut Vec<EntityView>, env: &Env) {
    if entities.len() != env.placed.len() {
        *entities = entity_views(env.placed, env.templates, env.construction);
    }
}

/// The parts of each placed entity the activities read.
pub fn entity_views(
    placed: &[Placed],
    templates: &[Template],
    construction: &crate::work::construction::Construction,
) -> Vec<EntityView> {
    // Entities and sites are both in index order, so one pass walks the sites alongside the
    // entities instead of looking each entity up in the map (28,000 lookups per call in a
    // large town). The output is the same as looking each up.
    let mut sites = construction.sites.iter().peekable();
    placed
        .iter()
        .enumerate()
        .map(|(e, p)| {
            while sites.next_if(|(&k, _)| k < e).is_some() {}
            let site = sites.next_if(|(&k, _)| k == e).map(|(_, s)| s);
            let t = &templates[p.template];
            let rect = t.map.as_ref().map(|_| {
                let (w, h) = p.footprint;
                (p.cell.0, p.cell.1, w, h)
            });
            // The entity's use points, then its MultiObject parts' (`FUN_140128540`,
            // `FUN_1401282f0`), each turned by its own transform.
            let parts = site.map_or(&[][..], |s| &s.parts[..]);
            // Most entities (trees, rocks, piles) have no use points and no parts: skip the
            // iterator chain for them. The result is the same empty list; in a town of 28,000
            // entities the chain alone was a quarter of the simulation time.
            let use_points = if parts.is_empty() && t.use_points.is_empty() {
                Vec::new()
            } else {
                std::iter::once(e)
                    .chain(parts.iter().copied())
                    .filter_map(|x| placed.get(x))
                    .flat_map(|q| {
                        let qt = &templates[q.template];
                        let model = q.model(qt);
                        qt.use_points.iter().map(move |&l| model.apply(l))
                    })
                    .collect()
            };
            // `FUN_14004b080`: registered, and not the unhappy kind.
            let happiness = site.and_then(|site| {
                let d = construction.templates[site.template].happiness?;
                (site.happiness_registered && d.kind != 5).then_some(d.pad)
            });
            EntityView {
                alive: p.alive,
                rect: if p.alive { rect } else { None },
                use_points,
                has_interact: t.has_interact,
                position: p.position,
                has_model: t.model.is_some(),
                happiness,
            }
        })
        .collect()
}

/// An entity's StorageComponent: a construction site's (houses, workplaces), else a town
/// storage's.
pub(super) fn storage_of(env: &Env, entity: usize) -> Option<usize> {
    env.construction
        .sites
        .get(&entity)
        .map(|s| s.storage)
        .or_else(|| {
            env.sites
                .iter()
                .find(|s| s.entity == entity)
                .map(|s| s.storage)
        })
}

impl Agents {
    /// The path sectors' marks in the order the cells flipped (`Cells::set_type`, made on the
    /// spot by `FUN_1400ae160` in the original: a rebuilt sector hands its node ids out in
    /// mark order); then `FUN_1400a1950` and the path grid's type commands where the map
    /// changed.
    pub(super) fn sync_map(&mut self, cells: &Cells) {
        for &i in cells
            .sector_marks
            .get(self.marks_seen..)
            .unwrap_or_default()
        {
            let i = i as usize;
            self.sectors
                .mark((i % cells.size) as i32, (i / cells.size) as i32);
        }
        self.marks_seen = cells.sector_marks.len();
        // Comparing the whole map (262,144 cells on a large map) six times an iteration was
        // the largest single cost in a large town; the revision says whether anything changed.
        if self.types_seen == cells.types_revision {
            return;
        }
        self.types_seen = cells.types_revision;
        if self.region_types == cells.types {
            return;
        }
        // The regions depend only on which cells are walkable (`regions`), so the map-wide
        // fill runs again only when a change makes a cell walkable or not walkable. In a large
        // town fields, roads and plantings change cell types often without that.
        let mut walkability_changed = false;
        for (i, (old, new)) in self.region_types.iter_mut().zip(&cells.types).enumerate() {
            if *old != *new {
                self.pathfinder.set_type(i, *new);
                walkability_changed |=
                    crate::pathfind::walkable(*old) != crate::pathfind::walkable(*new);
                *old = *new;
            }
        }
        if walkability_changed {
            self.regions = regions(cells.size, &cells.types);
        }
    }
}
