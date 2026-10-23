//! Game time, and the entity manager's update buckets.
//!
//! **Time.** The engine counts time in integer microseconds (`System::Time`). [`ticks`] and
//! [`seconds`] convert as the original does: through `f64`, truncating to time and rounding to
//! `f32` seconds.
//!
//! **Buckets.** A town has tens of thousands of entities, so the game does not update all of them
//! in every simulation iteration. [`EntityScheduler`] keeps them in five buckets, and each
//! iteration visits at most [`QUOTAS`] entities of each bucket, starting where the last visit
//! stopped. Each entity receives the time since its own last visit, so no time is lost.
//!
//! ```
//! use unbanished_core::scheduler::{seconds, ticks};
//!
//! assert_eq!(ticks(1.5), 1_500_000);
//! assert_eq!(seconds(ticks(1.5)), 1.5);
//! ```
//!
//! The port's handles are stable indices; the engine uses object pointers. Removing an entity
//! still swaps it with the last one of its bucket, as in the engine.
//!
//! Original: `FUN_1400c9460`, `FUN_1400c9560`, `FUN_1400c9d70`.

/// The length of one tick of game time, in seconds: the engine's timer counts 1,000,000 ticks per
/// second.
///
/// Original: `System::Timer::Startup`, Runtime `0x1800cd030`.
pub const SECONDS_PER_TICK: f64 = 1e-6;

/// Seconds to game time: through `f64`, truncated.
///
/// Original: constructing a `System::Time`, Runtime `0x1800078c0`.
pub fn ticks(seconds: f32) -> i64 {
    (seconds as f64 / SECONDS_PER_TICK) as i64
}

/// Game time to seconds: through `f64`, rounded to `f32`.
///
/// Original: `System::Time::AsSeconds`, Runtime `0x180007900`.
pub fn seconds(time: i64) -> f32 {
    (time as f64 * SECONDS_PER_TICK) as f32
}

// The entity manager's +0xa8..+0xb8.
/// How many entities of each bucket one iteration visits, for buckets 0 to 4.
///
/// Original: `FUN_1400c9460`.
pub const QUOTAS: [usize; 5] = [50, 15, 8, 5, 2];

struct Entity<T> {
    value: T,
    bucket: usize,
    slot: usize,
    last_time: i64,
    live: bool,
}

/// The bucket of a handle held free by [`EntityScheduler::register_at`] until its entity registers.
const FREE: usize = usize::MAX;

/// Handles of entities registered with [`EntityScheduler::register_extra`] start here, so that
/// a placed entity's handle stays its index. The game gives such entities values from here too:
/// citizens `EXTRA + citizen`, and entities the town does not model (the map ambience, a
/// saved game's entities of templates it skips) from [`FOREIGN`].
pub const EXTRA: usize = 1 << 30;
/// Values of scheduled entities with no behaviour in the port (visited for the cursors' sake).
pub const FOREIGN: usize = EXTRA + (1 << 29);

/// The entity manager's buckets: which entities are visited in each simulation iteration, and
/// the time each one has had since its last visit. `T` is the caller's value for an entity.
pub struct EntityScheduler<T> {
    /// The current game time.
    pub time: i64,
    entities: Vec<Entity<T>>,
    /// Entities outside the placed index space (handles from [`EXTRA`]).
    extra: Vec<Entity<T>>,
    buckets: [Vec<usize>; 5],
    cursors: [usize; 5],
    /// This iteration's visits, in order: the value and the time since its last visit.
    visited: Vec<(T, i64)>,
}

impl<T: Copy> Default for EntityScheduler<T> {
    fn default() -> Self {
        Self {
            time: 0,
            entities: Vec::new(),
            extra: Vec::new(),
            buckets: std::array::from_fn(|_| Vec::new()),
            cursors: [0; 5],
            visited: Vec::new(),
        }
    }
}

impl<T: Copy> EntityScheduler<T> {
    /// Register an entity in a bucket, at the end, with its last visit set to now. Returns its
    /// handle. Bucket 6 means "not scheduled": the entity is registered but never visited.
    ///
    /// Original: `FUN_1400c9d70`.
    pub fn register(&mut self, value: T, bucket: usize) -> usize {
        assert!(
            bucket < 5 || bucket == 6,
            "invalid component-description bucket"
        );
        let id = self.entities.len();
        let slot = if bucket < 5 {
            let slot = self.buckets[bucket].len();
            self.buckets[bucket].push(id);
            slot
        } else {
            0
        };
        self.entities.push(Entity {
            value,
            bucket,
            slot,
            last_time: self.time,
            live: true,
        });
        id
    }

    /// [`register`](Self::register) at handle `id`: the entity's own index, whatever the
    /// order the entities are registered in. Indices below `id` not yet registered are held
    /// free until they are. The port makes some entities first and registers them at the end
    /// of the iteration (piles), while others register as they are made (spawned trees), so
    /// handing out the next free handle gave a tree a pile's handle and its bucket.
    ///
    /// Original: `FUN_1400c9d70`.
    pub fn register_at(&mut self, id: usize, value: T, bucket: usize) -> usize {
        assert!(
            bucket < 5 || bucket == 6,
            "invalid component-description bucket"
        );
        while self.entities.len() < id {
            self.entities.push(Entity {
                value,
                bucket: FREE,
                slot: 0,
                last_time: self.time,
                live: false,
            });
        }
        let slot = if bucket < 5 {
            let slot = self.buckets[bucket].len();
            self.buckets[bucket].push(id);
            slot
        } else {
            0
        };
        let entity = Entity {
            value,
            bucket,
            slot,
            last_time: self.time,
            live: true,
        };
        if id < self.entities.len() {
            self.entities[id] = entity;
        } else {
            self.entities.push(entity);
        }
        id
    }

    /// [`register`](Self::register) in the extra handle space (from [`EXTRA`]): the same
    /// buckets, cursors and timing, without taking a placed entity's index.
    pub fn register_extra(&mut self, value: T, bucket: usize) -> usize {
        let id = EXTRA + self.extra.len();
        let slot = if bucket < 5 {
            let slot = self.buckets[bucket].len();
            self.buckets[bucket].push(id);
            slot
        } else {
            0
        };
        self.extra.push(Entity {
            value,
            bucket,
            slot,
            last_time: self.time,
            live: true,
        });
        id
    }

    fn entity_mut(&mut self, id: usize) -> &mut Entity<T> {
        if id >= EXTRA {
            &mut self.extra[id - EXTRA]
        } else {
            &mut self.entities[id]
        }
    }

    /// Whether `handle` was registered.
    pub fn contains(&self, handle: usize) -> bool {
        if handle >= EXTRA {
            handle - EXTRA < self.extra.len()
        } else {
            self.entities.get(handle).is_some_and(|e| e.bucket != FREE)
        }
    }

    /// A bucket's handles in visiting order.
    pub fn bucket(&self, bucket: usize) -> &[usize] {
        &self.buckets[bucket]
    }

    // +0xbc
    /// A bucket's cursor, the slot its next visit starts at.
    pub fn cursor(&self, bucket: usize) -> usize {
        self.cursors[bucket]
    }

    /// An entity's bucket (6: none) and last visit time.
    pub fn schedule(&self, handle: usize) -> (usize, i64) {
        let e = if handle >= EXTRA {
            &self.extra[handle - EXTRA]
        } else {
            &self.entities[handle]
        };
        (e.bucket, e.last_time)
    }

    /// A saved game's bucket lists, cursors and schedules: `order` lists each bucket's handles
    /// in its saved order, `saved` each saved entity's bucket (6: none) and last visit time.
    /// Registered entities the save does not know keep their bucket and follow its listed ones.
    pub fn restore(
        &mut self,
        order: &[Vec<usize>; 5],
        cursors: [usize; 5],
        saved: &[(usize, usize, i64)],
    ) {
        let listed: std::collections::HashSet<usize> = order
            .iter()
            .flatten()
            .copied()
            .chain(saved.iter().map(|s| s.0))
            .collect();
        let mut lists: [Vec<usize>; 5] = std::array::from_fn(|b| {
            order[b]
                .iter()
                .copied()
                .filter(|&h| self.contains(h))
                .collect()
        });
        for b in 0..5 {
            for &h in &self.buckets[b] {
                if !listed.contains(&h) {
                    lists[b].push(h);
                }
            }
        }
        for (b, list) in lists.iter().enumerate() {
            for (slot, &h) in list.iter().enumerate() {
                let e = self.entity_mut(h);
                e.bucket = b;
                e.slot = slot;
            }
        }
        self.buckets = lists;
        self.cursors = cursors;
        let in_lists: std::collections::HashSet<usize> =
            self.buckets.iter().flatten().copied().collect();
        for &(h, bucket, t) in saved {
            if !self.contains(h) {
                continue;
            }
            let e = self.entity_mut(h);
            e.last_time = t;
            if !in_lists.contains(&h) {
                e.bucket = if bucket < 5 { 6 } else { bucket };
            }
        }
    }

    /// The values visited since the last call, each with the time since its previous visit,
    /// in visiting order (the bucket pass's scheduled components run for these).
    pub fn take_visits(&mut self) -> Vec<(T, i64)> {
        std::mem::take(&mut self.visited)
    }

    /// Advance the clock by `dt` seconds. Then the caller visits each bucket, then removes the
    /// entities that were deleted.
    ///
    /// Original: `FUN_1400c9560` (the clock).
    pub fn advance(&mut self, dt: f32) {
        self.time += ticks(dt);
    }

    /// Visit one bucket: from its cursor, at most its quota of entities or one full round. Each
    /// visit gives the entity's handle, value and the time since its own last visit. Entities
    /// registered during the visits wait until the next iteration.
    ///
    /// Original: `FUN_1400c9560`.
    pub fn visit_bucket(&mut self, bucket: usize) -> Vec<(usize, T, i64)> {
        let list = &self.buckets[bucket];
        let count = list.len();
        if count == 0 {
            return Vec::new();
        }
        let mut cursor = if self.cursors[bucket] < count {
            self.cursors[bucket]
        } else {
            0
        };
        let mut visits = Vec::with_capacity(QUOTAS[bucket].min(count));
        let time = self.time;
        for _ in 0..QUOTAS[bucket].min(count) {
            let id = self.buckets[bucket][cursor];
            let entity = self.entity_mut(id);
            let dt = time - entity.last_time;
            if entity.live && dt > 0 {
                entity.last_time = time;
                let value = entity.value;
                visits.push((id, value, dt));
                self.visited.push((value, dt));
            }
            cursor = (cursor + 1) % count;
        }
        self.cursors[bucket] = cursor;
        visits
    }

    /// Remove an entity: it swaps places with the last entity of its bucket, and the cursor does
    /// not move. Call it after all visits of the iteration, as the original deletes late.
    ///
    /// Original: `FUN_1400c8db0`.
    pub fn remove(&mut self, id: usize) {
        let entity = self.entity_mut(id);
        if !entity.live {
            return;
        }
        let (bucket, slot) = (entity.bucket, entity.slot);
        if bucket < 5 {
            self.buckets[bucket].swap_remove(slot);
            if let Some(&moved) = self.buckets[bucket].get(slot) {
                self.entity_mut(moved).slot = slot;
            }
        }
        self.entity_mut(id).live = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotas_elapsed_time_and_registration() {
        let mut scheduler = EntityScheduler::default();
        for i in 0..11 {
            scheduler.register(i, 3);
        }
        scheduler.register(99, 6);
        scheduler.advance(0.5);
        let first = scheduler.visit_bucket(3);
        assert_eq!(
            first.iter().map(|v| v.1).collect::<Vec<_>>(),
            [0, 1, 2, 3, 4]
        );
        assert!(first.iter().all(|v| v.2 == ticks(0.5)));
        scheduler.advance(0.5);
        let second = scheduler.visit_bucket(3);
        assert_eq!(
            second.iter().map(|v| v.1).collect::<Vec<_>>(),
            [5, 6, 7, 8, 9]
        );
        assert!(second.iter().all(|v| v.2 == 2 * ticks(0.5)));
        scheduler.advance(0.5);
        let third = scheduler.visit_bucket(3);
        assert_eq!(
            third.iter().map(|v| v.1).collect::<Vec<_>>(),
            [10, 0, 1, 2, 3]
        );
        assert_eq!(third[0].2, 3 * ticks(0.5));
        assert_eq!(third[1].2, 2 * ticks(0.5));
    }

    #[test]
    fn deletion_preserves_swap_order_and_zero_delta_skips_callbacks() {
        let mut scheduler = EntityScheduler::default();
        let ids: Vec<_> = (0..4).map(|i| scheduler.register(i, 4)).collect();
        assert!(scheduler.visit_bucket(4).is_empty());
        // Even zero-delta visits move the cursor; the next live visit starts with 2, 3.
        scheduler.advance(1.0);
        scheduler.visit_bucket(4);
        scheduler.remove(ids[1]); // [0, 3, 2], cursor still 0
        scheduler.advance(1.0);
        assert_eq!(
            scheduler
                .visit_bucket(4)
                .iter()
                .map(|v| v.1)
                .collect::<Vec<_>>(),
            [0, 3]
        );
        scheduler.remove(ids[3]);
        scheduler.advance(1.0);
        assert_eq!(
            scheduler
                .visit_bucket(4)
                .iter()
                .map(|v| v.1)
                .collect::<Vec<_>>(),
            [0, 2]
        );
    }

    /// A pile made first and registered at the end of the iteration keeps its own handle and
    /// bucket when a tree made after it registers at once.
    #[test]
    fn registering_out_of_order_keeps_each_handle_and_bucket() {
        let mut scheduler = EntityScheduler::default();
        scheduler.register_at(0, 0usize, 4);
        assert!(!scheduler.contains(1));
        assert_eq!(scheduler.register_at(2, 2, 4), 2);
        assert!(!scheduler.contains(1));
        assert_eq!(scheduler.register_at(1, 1, 3), 1);
        assert!(scheduler.contains(1));
        assert_eq!(scheduler.bucket(4), &[0, 2]);
        assert_eq!(scheduler.bucket(3), &[1]);
        assert_eq!(scheduler.schedule(1).0, 3);
    }
}
