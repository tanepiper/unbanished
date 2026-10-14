//! Entity-manager buckets (`FUN_1400c9460`, `FUN_1400c9560`, `FUN_1400c9d70`).
//! Stable Rust handles replace InternalBasePtr; bucket deletion still swaps with the last entry.

/// Deviation: use nanosecond ticks on every platform instead of the Windows performance timer's
/// frequency. `System::Time` converts through f64, truncating on construction (Runtime
/// `0x1800078c0`) and rounding to f32 on AsSeconds (`0x180007900`).
pub const SECONDS_PER_TICK: f64 = 1e-9;

pub fn ticks(seconds: f32) -> i64 {
    (seconds as f64 / SECONDS_PER_TICK) as i64
}

pub fn seconds(time: i64) -> f32 {
    (time as f64 * SECONDS_PER_TICK) as f32
}

/// `FUN_1400c9460`: quotas at manager +0xa8..+0xb8, in bucket order 0..4.
pub const QUOTAS: [usize; 5] = [50, 15, 8, 5, 2];

struct Entity<T> {
    value: T,
    bucket: usize,
    slot: usize,
    last_time: i64,
    live: bool,
}

pub struct EntityScheduler<T> {
    pub time: i64,
    entities: Vec<Entity<T>>,
    buckets: [Vec<usize>; 5],
    cursors: [usize; 5],
}

impl<T: Copy> Default for EntityScheduler<T> {
    fn default() -> Self {
        Self {
            time: 0,
            entities: Vec::new(),
            buckets: std::array::from_fn(|_| Vec::new()),
            cursors: [0; 5],
        }
    }
}

impl<T: Copy> EntityScheduler<T> {
    /// Port of FUN_1400c9d70: append in registration order and set last-update time now.
    /// Bucket 6 is deliberately absent from the scheduled arrays.
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

    /// Port of FUN_1400c9560's clock advance. A caller visits each bucket, then drains removals.
    pub fn advance(&mut self, dt: f32) {
        self.time += ticks(dt);
    }

    /// Port of FUN_1400c9560: wrap the cursor, visit at most a quota or one full revolution,
    /// and give each entity the time since its own last visit. Snapshot this bucket's length,
    /// so children registered during its callbacks wait until the next iteration.
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
        for _ in 0..QUOTAS[bucket].min(count) {
            let id = list[cursor];
            let entity = &mut self.entities[id];
            let dt = self.time - entity.last_time;
            if entity.live && dt > 0 {
                visits.push((id, entity.value, dt));
                entity.last_time = self.time;
            }
            cursor = (cursor + 1) % count;
        }
        self.cursors[bucket] = cursor;
        visits
    }

    /// Port of FUN_1400c8db0: swap-remove without adjusting the saved cursor. Call after all
    /// scheduled callbacks, matching FUN_1400c9560's deferred deletion phase.
    pub fn remove(&mut self, id: usize) {
        let entity = &self.entities[id];
        if !entity.live {
            return;
        }
        let (bucket, slot) = (entity.bucket, entity.slot);
        if bucket < 5 {
            self.buckets[bucket].swap_remove(slot);
            if let Some(&moved) = self.buckets[bucket].get(slot) {
                self.entities[moved].slot = slot;
            }
        }
        self.entities[id].live = false;
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
}
