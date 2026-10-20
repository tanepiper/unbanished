//! The action stack: the actions a character carries out in turn, their reservations, and saving
//! and restoring them.

use super::*;

#[derive(Debug, Clone)]
pub(super) struct Task {
    action: Action,
    started: bool,
}

// +0x48; +0x58; +0x60
/// The CharacterComponent's task stack (top index) and its "on empty" chain.
#[derive(Debug, Clone, Default)]
pub struct ActionStack {
    chains: Vec<Vec<Task>>,
    on_empty: Vec<Task>,
    /// Job-manager messages from aborted CompleteTasks, for the owner to deliver right after the
    /// clear.
    ///
    /// Original: `FUN_1400caf90`.
    pub outbox: Vec<Message>,
}

impl ActionStack {
    // +0x18; +0x10
    /// A saved stack: its entries bottom to top, each one action and whether it started, and the
    /// "on empty" action. Actions linked to an entry are not saved.
    ///
    /// Original: `FUN_14010ef80`.
    pub fn restored(entries: Vec<(Action, bool)>, on_empty: Option<Action>) -> ActionStack {
        ActionStack {
            chains: entries
                .into_iter()
                .map(|(action, started)| vec![Task { action, started }])
                .collect(),
            on_empty: on_empty
                .into_iter()
                .map(|action| Task {
                    action,
                    started: false,
                })
                .collect(),
            outbox: Vec::new(),
        }
    }

    /// What a save holds: each entry's first action with its started flag, and the "on empty"
    /// chain's first action.
    ///
    /// Original: `FUN_14010ef80`.
    pub fn saved(&self) -> (Vec<(&Action, bool)>, Option<&Action>) {
        (
            self.chains
                .iter()
                .filter_map(|c| c.first().map(|t| (&t.action, t.started)))
                .collect(),
            self.on_empty.first().map(|t| &t.action),
        )
    }

    // +0x60 + 1
    /// How many entries the stack holds.
    pub fn depth(&self) -> usize {
        self.chains.len()
    }

    // +0x60 >= 0
    /// `>= 0`: something is queued (the AI's "is busy" test).
    pub fn is_active(&self) -> bool {
        !self.chains.is_empty()
    }

    // +0x40
    /// Abort every chain (slot on all actions) and drop the "on empty" chain.
    ///
    /// Original: `FUN_1400ca8d0`.
    pub fn clear(&mut self, ch: &mut Character, group: &AnimationGroup, storages: &mut [Storage]) {
        for chain in &mut self.chains {
            for t in chain.iter_mut() {
                if let Action::CompleteTask { work, param } = t.action {
                    if work >= 0 {
                        self.outbox.push(Message {
                            component: Component::Jobs,
                            message: 1,
                            entity: None,
                            a: work,
                            b: param,
                        });
                    }
                }
                // `FUN_1400cff70`: the resource is free again.
                if let Action::RemoveResource { entity, .. } = t.action {
                    self.outbox.push(Message {
                        component: Component::NaturalResource,
                        message: 1,
                        entity: Some(entity),
                        a: 0,
                        b: 0,
                    });
                }
                t.action.abort(ch, group, storages);
            }
        }
        self.chains.clear();
        self.on_empty.clear();
    }

    /// The stored goods queued actions hold reserved: `(storage, material, count)` of each
    /// unstarted PickupStoredRawMaterial and reserving ConsumeStoredRawMaterial.
    pub fn pending_reservations(&self) -> Vec<(usize, usize, i32)> {
        let mut out = Vec::new();
        for t in self.chains.iter().flatten().filter(|t| !t.started) {
            match &t.action {
                Action::PickupStoredRawMaterial { storage, items, .. }
                | Action::ConsumeStoredRawMaterial {
                    storage,
                    items,
                    reserved: true,
                } => out.extend(items.iter().map(|&(m, n)| (*storage, m, n))),
                _ => {}
            }
        }
        out
    }

    /// Does a chain start with this work's CompleteTask?
    ///
    /// Original: `FUN_1400fc5d0`.
    pub fn holds_task(&self, work: i32) -> Option<i32> {
        self.chains
            .iter()
            .find_map(|c| match c.first().map(|t| &t.action) {
                Some(Action::CompleteTask { work: w, param }) if *w == work => Some(*param),
                _ => None,
            })
    }

    // +0x10
    /// Link `action` into the top chain (the original's of the top action).
    pub fn attach(&mut self, action: Action) {
        if let Some(top) = self.chains.last_mut() {
            top.push(Task {
                action,
                started: false,
            });
        }
    }

    /// The top chain's `HuntAnimal`, if any: its lodge and its `done` flag.
    pub fn hunt_watch(&mut self) -> Option<(usize, &mut bool)> {
        self.chains
            .last_mut()?
            .iter_mut()
            .filter(|t| t.started)
            .find_map(|t| match &mut t.action {
                Action::HuntAnimal { lodge, done } => Some((*lodge, done)),
                _ => None,
            })
    }

    // +0x30
    /// The scheduled notify: the running chain's started actions get slot; MoveTo's sets whether it
    /// follows.
    ///
    /// Original: `FUN_14010e950`, `FUN_1400ca450`, `FUN_1400cde40`.
    pub fn notify_moves(&mut self, following: bool) {
        let Some(chain) = self.chains.last_mut() else {
            return;
        };
        for t in chain.iter_mut().filter(|t| t.started) {
            if let Action::MoveTo(m) = &mut t.action {
                m.following = following;
            }
        }
    }

    /// Set the actions to run when the stack is empty.
    pub fn set_on_empty(&mut self, actions: Vec<Action>) {
        self.on_empty = actions
            .into_iter()
            .map(|action| Task {
                action,
                started: false,
            })
            .collect();
    }

    /// Stop the current top's started actions, then push a chain.
    ///
    /// Original: `FUN_1400ca9b0`.
    pub fn push(&mut self, ch: &mut Character, group: &AnimationGroup, action: Action) {
        if let Some(top) = self.chains.last_mut() {
            for t in top.iter_mut().filter(|t| t.started) {
                t.started = false;
                t.action.stop(ch, group);
            }
        }
        self.chains.push(vec![Task {
            action,
            started: false,
        }]);
    }

    /// `FUN_1400ca120`.
    pub(super) fn run(
        &mut self,
        ch: &mut Character,
        world: &mut World,
        group: &AnimationGroup,
        dt: f32,
    ) {
        let dt = ticks(dt);
        while let Some(top) = self.chains.last_mut() {
            let mut status = Status::Running;
            if !top.first().is_some_and(|t| t.started) {
                for t in top.iter_mut() {
                    t.started = true;
                    status = t.action.start(ch, world, group);
                    if status != Status::Running || !world.pushes.is_empty() {
                        break;
                    }
                }
            }
            // An action's start pushed a chain (`ActionStack::Push`): the top's started
            // actions stop and the new chain runs first.
            if !world.pushes.is_empty() {
                for action in std::mem::take(&mut world.pushes) {
                    self.push(ch, group, action);
                }
                if status == Status::Failed {
                    self.clear(ch, group, world.storages);
                }
                continue;
            }
            let mut ran = false;
            if status == Status::Running {
                for t in top.iter_mut() {
                    status = t.action.update(ch, world, group, dt);
                    if status != Status::Running {
                        break;
                    }
                }
                ran = status == Status::Running;
            }
            match status {
                Status::Done => {
                    let mut chain = self.chains.pop().unwrap_or_default();
                    for t in chain.iter_mut().filter(|t| t.started) {
                        t.started = false;
                        t.action.stop(ch, group);
                    }
                }
                // `FUN_1400ca8d0` also drops the "on empty" chain.
                Status::Failed => self.clear(ch, group, world.storages),
                Status::Running => {}
            }
            if ran {
                break;
            }
        }
        if self.chains.is_empty() && !self.on_empty.is_empty() {
            for t in self.on_empty.iter_mut() {
                t.started = true;
                if t.action.start(ch, world, group) != Status::Running {
                    break;
                }
            }
            self.on_empty.clear();
        }
    }
}
