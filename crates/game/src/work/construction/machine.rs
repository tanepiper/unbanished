//! The state machine that runs a site's controllers: the current state of each, messages to them,
//! and one step of the machine.

use super::*;

// StateMachineComponent +0x48; +0; +8; +9; +10; +0xb
/// The runner's fields (`StateMachineComponent`): the current interface, state, its index, pending
/// state, pending transition.
#[derive(Debug, Clone, Default)]
pub struct Machine {
    /// The building's controllers, in component order.
    pub controllers: Vec<Controller>,
    /// The controller that handles the current state.
    pub current: usize,
    /// The current state.
    pub state: i8,
    /// The index of the current state in its controller's table.
    pub index: i8,
    /// The state to enter next (-1: none).
    pub pending: i8,
    /// The transition requested (-1: none).
    pub transition: i8,
}

/// Run a building's state machine for one visit: handle the pending state change, then the
/// scheduled or per-iteration update of the current state.
///
/// Original: `FUN_1400aeb30(machine, time, scheduled, update)`.
pub fn run(host: &mut impl Callbacks, jobs: &mut Jobs, site: usize, scheduled: bool, update: bool) {
    let mut update = update;
    loop {
        while host.machine(site).pending >= 0 {
            let m = host.machine(site);
            if m.state >= 0 {
                // The exit chain, from the current interface back.
                let (state, mut node, mut index) = (m.state, m.current, m.index as i32);
                loop {
                    let m = host.machine(site);
                    let f = if index >= 0 {
                        m.controllers[node].table()[index as usize].exit
                    } else {
                        C::None
                    };
                    if index >= 0 && host.call(jobs, site, f) {
                        break;
                    }
                    let Some(prev) = node.checked_sub(1) else {
                        break;
                    };
                    node = prev;
                    index = host.machine(site).controllers[node]
                        .find(state)
                        .map_or(-1, |i| i as i32);
                }
                let m = host.machine(site);
                if m.transition >= 0 && m.index >= 0 {
                    let f = m.controllers[m.current].table()[m.index as usize].transitions
                        [m.transition as usize]
                        .1;
                    host.call(jobs, site, f);
                }
            }
            let m = host.machine(site);
            m.state = m.pending;
            let last = m.controllers.len() - 1;
            let Some((node, index)) = m.find_from(last, m.state) else {
                m.pending = -1;
                m.transition = -1;
                m.index = -1;
                break;
            };
            m.current = node;
            m.index = index as i8;
            m.pending = -1;
            m.transition = -1;
            let state = m.state;
            // The enter chain.
            let (mut node, mut index) = (node, index as i32);
            loop {
                let f = if index >= 0 {
                    host.machine(site).controllers[node].table()[index as usize].enter
                } else {
                    C::None
                };
                if index >= 0 && host.call(jobs, site, f) {
                    break;
                }
                let Some(prev) = node.checked_sub(1) else {
                    break;
                };
                node = prev;
                index = host.machine(site).controllers[node]
                    .find(state)
                    .map_or(-1, |i| i as i32);
            }
        }
        if update && host.machine(site).state >= 0 {
            update = false;
            if scheduled {
                let m = host.machine(site);
                let (state, mut node, mut index) = (m.state, m.current, m.index as i32);
                loop {
                    let f = if index >= 0 {
                        host.machine(site).controllers[node].table()[index as usize].update
                    } else {
                        C::None
                    };
                    if index >= 0 && host.call(jobs, site, f) {
                        break;
                    }
                    let Some(prev) = node.checked_sub(1) else {
                        break;
                    };
                    node = prev;
                    index = host.machine(site).controllers[node]
                        .find(state)
                        .map_or(-1, |i| i as i32);
                }
            }
            // The bit-4 update slot (`+0x10`) is null for every ported controller.
        }
        if host.machine(site).pending < 0 {
            return;
        }
    }
}

impl Machine {
    /// Pending = the first controller's initial state.
    ///
    /// Original: `FUN_140130f00`, `FUN_140130b70`.
    pub fn new(controllers: Vec<Controller>) -> Machine {
        let pending = controllers.first().map_or(-1, |c| c.initial());
        Machine {
            current: controllers.len().saturating_sub(1),
            controllers,
            state: -1,
            index: -1,
            pending,
            transition: -1,
        }
    }

    /// The last controller (from `node`, going back) that has `state`.
    pub(super) fn find_from(&self, mut node: usize, state: i8) -> Option<(usize, usize)> {
        loop {
            if let Some(i) = self.controllers[node].find(state) {
                return Some((node, i));
            }
            node = node.checked_sub(1)?;
        }
    }

    /// Look `msg` up in the current state's transitions.
    ///
    /// Original: `FUN_1400aef90`.
    pub fn request(&mut self, msg: i8) {
        if self.index < 0 {
            return;
        }
        let def = &self.controllers[self.current].table()[self.index as usize];
        if let Some(t) = def.transitions.iter().position(|t| t.0 == msg) {
            self.transition = t as i8;
            self.pending = msg;
        }
    }
}
