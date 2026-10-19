//! Giving citizens their orders: workers of a building, queued actions and the think hook.

use super::*;

impl Sim<'_, '_> {
    pub fn workers(&self) -> Vec<Worker> {
        self.states
            .iter()
            .zip(self.adult)
            .map(|(s, &adult)| Worker {
                adult,
                profession: s.profession,
                workplace: s.workplace,
                home: s.home,
            })
            .collect()
    }

    pub fn set_workers(&mut self, workers: &[Worker]) {
        for (s, w) in self.states.iter_mut().zip(workers) {
            s.profession = w.profession;
            s.workplace = w.workplace;
        }
    }

    /// `FUN_1400ca8d0` on a citizen: abort everything; aborted CompleteTasks of `current` go
    /// straight to its fail slot, the others to the manager's queue.
    pub(crate) fn clear_citizen(&mut self, c: usize, mut current: Option<&mut Work>) {
        let groups = self.groups;
        let Some(group) = groups.get(c).copied().flatten() else {
            return;
        };
        let ch = &mut self.characters[c];
        let mut stack = std::mem::take(&mut ch.stack);
        stack.clear(ch, group, self.storages);
        let outbox = std::mem::take(&mut stack.outbox);
        ch.stack = stack;
        for m in outbox {
            let e = JobEvent {
                work: m.a,
                citizen: c,
                param: m.b,
                complete: m.message == 0,
            };
            match current.as_deref_mut() {
                Some(w) if w.id == e.work => self.fail(w, c, e.param),
                _ => self.events.push(e),
            }
        }
    }

    pub(crate) fn push(&mut self, c: usize, action: Action) {
        let groups = self.groups;
        let Some(group) = groups.get(c).copied().flatten() else {
            return;
        };
        let ch = &mut self.characters[c];
        let mut stack = std::mem::take(&mut ch.stack);
        stack.push(ch, group, action);
        ch.stack = stack;
    }

    pub(crate) fn on_empty_think(&mut self, c: usize) {
        self.characters[c]
            .stack
            .set_on_empty(vec![Action::SendMessage(Message {
                component: Component::CitizenAi,
                message: 0,
                entity: None,
                a: 0,
                b: 0,
            })]);
    }

    /// `FUN_1400cebf0`: PerformWork, slower without a tool when it uses one.
    pub(crate) fn perform(
        &self,
        c: usize,
        facing: f32,
        anim: usize,
        secs: f32,
        tool: bool,
    ) -> Action {
        let mut duration = ticks(secs);
        if tool && self.states[c].tool_uses < 1 {
            duration = ticks(seconds(duration) * self.rules.no_tool_factor);
        }
        Action::PerformWork {
            facing,
            anim,
            duration,
            elapsed: 0,
            consume: tool,
        }
    }

    /// The writes every successful assignment makes on the citizen.
    pub(crate) fn took_job(&mut self, c: usize, work: &Work) {
        let s = &mut self.states[c];
        s.health_events |= 4;
        s.job = work.id;
        s.supply_activity = 0;
        if work.profession == s.profession && work.id >= 0 {
            s.flags |= 0x20000;
        }
    }
}
