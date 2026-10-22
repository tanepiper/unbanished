//! Removing, prioritising and finishing work in the job lists.

use super::*;

impl Jobs {
    /// Unregister, refresh, cancel every worker, delete.
    ///
    /// Original: `FUN_1400fe300`.
    pub fn remove(&mut self, id: i32, host: &mut impl WorkHost) {
        let Some(mut work) = self.works.remove(&id) else {
            return;
        };
        for c in &mut self.categories {
            c.remove(&id);
        }
        if self.priorities.get(&work.priority) == Some(&id) {
            self.priorities.remove(&work.priority);
        }
        if let Some(set) = self.professions.get_mut(&work.profession) {
            set.remove(&id);
        }
        host.refresh(&mut work);
        host.cancel(&mut work);
        self.drain(host);
    }

    /// Deliver the host's pending job events in order.
    pub fn drain(&mut self, host: &mut impl WorkHost) {
        loop {
            let events = host.take_events();
            if events.is_empty() {
                return;
            }
            for e in events {
                if e.complete {
                    self.complete(e.work, e.citizen, e.param, host);
                } else {
                    self.fail(e.work, e.citizen, e.param, host);
                }
            }
        }
    }

    /// Move a work to the front (`lowest priority − 1`).
    ///
    /// Original: `FUN_1400fefa0`.
    pub fn prioritize(&mut self, id: i32) {
        let Some(work) = self.works.get(&id) else {
            return;
        };
        let top = self
            .priorities
            .keys()
            .next()
            .map_or(0, |&p| p.wrapping_sub(1));
        let old = work.priority;
        if self.priorities.get(&old) == Some(&id) {
            self.priorities.remove(&old);
        }
        self.works.get_mut(&id).unwrap().priority = top;
        self.priorities.entry(top).or_insert(id);
    }

    /// Move a work from one profession's list to another's.
    ///
    /// Original: `FUN_1400fd8d0`.
    pub fn change_profession(&mut self, id: i32, from: usize, to: usize) {
        if !self.works.contains_key(&id) {
            return;
        }
        let Some(set) = self.professions.get_mut(&from) else {
            self.professions.entry(from).or_default();
            return;
        };
        if !set.remove(&id) {
            return;
        }
        self.professions.entry(to).or_default().insert(id);
        self.works.get_mut(&id).unwrap().profession = to;
    }
}
