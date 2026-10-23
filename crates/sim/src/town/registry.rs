//! Registering entities: the scheduler, ambience, foreign entities and markers.

use super::*;

impl StartingTown {
    /// The map ambience's region entities (`Environment.rsc:ambient`, bucket 2) of a new game: they
    /// take turns in their bucket with the populations. (Their river and wind entities have no
    /// bucket.)
    ///
    /// Original: `FUN_1400a57c0`.
    pub fn register_ambience(&mut self, regions: usize) {
        /// `Audio\Effects\Environment.rsc:ambient`'s bucket.
        const AMBIENCE_BUCKET: usize = 2;
        if self.save_ids.is_some() || !self.ambience_handles.is_empty() {
            return;
        }
        let foreign = unbanished_game::scheduler::FOREIGN;
        self.ambience_handles = (0..regions)
            .map(|r| self.scheduler.register_extra(foreign + r, AMBIENCE_BUCKET))
            .collect();
    }

    /// The entity manager's clock.
    pub fn entity_time(&self) -> i64 {
        self.scheduler.time
    }

    /// A saved entity the town does not model, kept in its bucket.
    ///
    /// Original: `FUN_1400c9d70`.
    pub fn register_foreign(&mut self, bucket: usize, n: usize) -> usize {
        self.scheduler
            .register_extra(unbanished_game::scheduler::FOREIGN + n, bucket)
    }

    // FUN_1400c9560 step 4 (FUN_1400c8db0): every entity queued for deletion (entity flag 0x400)
    // leaves its bucket, its place taken by the bucket's last entity.
    /// The deletions of the iteration: every scheduled entity that is no longer alive leaves its
    /// bucket. Before, a destroyed site, a picked-up pile or a cleared resource stayed in its
    /// bucket and took one of its visits each time its turn came.
    ///
    /// Deviation: the original deletes in the order the deletions were asked for. Here the seeder's
    /// removals go in that order and the rest in their buckets' order, which differs only when two
    /// entities of one bucket are deleted in the same iteration (the bucket's order after the
    /// swaps then differs).
    ///
    /// Original: `FUN_1400c8db0`.
    pub(super) fn remove_dead_entities(&mut self) {
        let dead: Vec<usize> = (0..unbanished_game::scheduler::QUOTAS.len())
            .flat_map(|b| self.scheduler.bucket(b).to_vec())
            .filter(|&h| self.placed.get(h).is_some_and(|p| !p.alive))
            .collect();
        for h in dead {
            self.scheduler.remove(h);
        }
    }

    /// Register a new entity in its template's bucket.
    ///
    /// Original: `FUN_1400c9d70`.
    pub fn register_entity(&mut self, id: usize) {
        // A spawned resource registered itself as it was made.
        if self.scheduler.contains(id) {
            return;
        }
        let handle = self.scheduler.register_at(id, id, self.placed[id].bucket);
        debug_assert_eq!(handle, id);
    }

    // CreateRemovalDescription +0x38
    /// A removal marker (made from `CreateRemovalDescription`, its MapComponent at `cell`,
    /// 1 × 1): registered, its ZoneComponent occupies the cell and its StateMachine is activated
    /// before the next iteration.
    ///
    /// Original: `FUN_1400c9d70`, `FUN_1400c8bf0`.
    pub fn add_marker(
        &mut self,
        archive: &Archive,
        template: usize,
        cell: (i32, i32),
        terrain: &HeightGrid,
    ) -> usize {
        let n = self.cells.size as i32;
        let half = (n >> 1) as f32 - 0.5;
        let (px, py) = (cell.0 as f32 - half, cell.1 as f32 - half);
        let mut normal = [0.0; 3];
        let z = terrain.ground(px, py, &mut normal);
        let id = self.placed.len();
        self.placed.push(Placed {
            template,
            bucket: self.templates[template].bucket,
            cell,
            footprint: (1, 1),
            position: [px, py, z],
            yaw: 0.0,
            variant: 0,
            mesh: usize::MAX,
            growth: 0.0,
            growth_state: None,
            spawn_state: None,
            alive: true,
            removal_requested: false,
            animation: None,
            scale: [1.0; 4],
        });
        self.register_entity(id);
        let i = (cell.1 * n + cell.0) as usize;
        self.cells.occupants[i] = Some(id);
        self.add_site(archive, id);
        id
    }

    /// [`Self::add_marker`] for a marker whose construction template is loaded.
    pub(super) fn add_cached_marker(
        &mut self,
        template: usize,
        cell: (i32, i32),
        terrain: &HeightGrid,
    ) {
        let reference = self.templates[template].reference.clone();
        let Some(st) = self
            .construction
            .templates
            .iter()
            .position(|t| t.reference == reference)
        else {
            return;
        };
        let n = self.cells.size as i32;
        let half = (n >> 1) as f32 - 0.5;
        let (px, py) = (cell.0 as f32 - half, cell.1 as f32 - half);
        let mut normal = [0.0; 3];
        let z = terrain.ground(px, py, &mut normal);
        let id = self.placed.len();
        self.placed.push(Placed {
            template,
            bucket: self.templates[template].bucket,
            cell,
            footprint: (1, 1),
            position: [px, py, z],
            yaw: 0.0,
            variant: 0,
            mesh: usize::MAX,
            growth: 0.0,
            growth_state: None,
            spawn_state: None,
            alive: true,
            removal_requested: false,
            animation: None,
            scale: [1.0; 4],
        });
        self.register_entity(id);
        let i = (cell.1 * n + cell.0) as usize;
        self.cells.occupants[i] = Some(id);
        self.add_site_from(st, id);
    }
}
