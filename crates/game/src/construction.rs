//! Construction sites: the building state machine (StateMachineComponent with the ClearArea,
//! WorkPlace, Build and Residence controllers) and the construction works
//! (`GatherRawMaterials`, `BuildStructure`). See docs/JOBS.md.
//!
//! A placed building's controllers are linked in component order; a state is handled by the
//! last controller that defines it, and a callback returning false passes it to the previous
//! one (`FUN_1400aeb30`). The callbacks themselves live on [`crate::town::Sim`].

use std::collections::{BTreeMap, BTreeSet};

use crate::jobs::Jobs;
use crate::professions::{Command, Professions, Workforce};

/// The ControllerComponents the port has state tables for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Controller {
    /// `FUN_14011d4e0`.
    ClearArea,
    /// `FUN_140134870`.
    WorkPlace,
    /// `FUN_14010e560`.
    Build,
    /// `FUN_14012f820`.
    Residence,
}

/// A state callback (`DAT_1401dbca0` records: enter `+8`, scheduled update `+0x18`, exit
/// `+0x20`, transition functions).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Callback {
    None,
    /// `FUN_14011ce30`, `FUN_14011cdc0`, `FUN_14011cd60`.
    ClearEnter0,
    ClearUpdate0,
    ClearExit0,
    /// `FUN_14011cc40` (state 0x15 exit).
    ClearExit15,
    /// `FUN_14011ccf0`, `FUN_14011ccc0`.
    ClearEnter1,
    ClearEnter2,
    /// `FUN_140134310` → `FUN_1401349f0`; `FUN_1401342c0`; `FUN_140134230`.
    WorkPlaceEnter0,
    WorkPlaceEnter2,
    WorkPlaceEnter3,
    /// `FUN_14010d760`, `FUN_14010d6b0`.
    BuildEnter1,
    BuildUpdate1,
    /// `FUN_14010cfb0`, `FUN_14010cbb0`, `FUN_14010cad0`.
    BuildEnter8,
    BuildUpdate8,
    BuildExit8,
    /// `FUN_14010cab0` → `FUN_14010d4c0` (transition 8 → 2).
    BuildFinish,
    /// `FUN_14012f2b0`.
    ResidenceEnter2,
    /// Callbacks whose port is still missing (they pass the state on).
    Missing,
}

pub struct StateDef {
    pub id: i8,
    pub enter: Callback,
    pub update: Callback,
    pub exit: Callback,
    pub transitions: &'static [(i8, Callback)],
}

use Callback as C;

const CLEAR: &[StateDef] = &[
    StateDef {
        id: 0,
        enter: C::ClearEnter0,
        update: C::ClearUpdate0,
        exit: C::ClearExit0,
        transitions: &[(1, C::None), (3, C::None)],
    },
    StateDef {
        id: 0x15,
        enter: C::Missing,
        update: C::Missing,
        exit: C::ClearExit15,
        transitions: &[(1, C::None), (3, C::None)],
    },
    StateDef {
        id: 1,
        enter: C::ClearEnter1,
        update: C::None,
        exit: C::None,
        transitions: &[(2, C::None), (3, C::None)],
    },
    StateDef {
        id: 2,
        enter: C::ClearEnter2,
        update: C::None,
        exit: C::None,
        transitions: &[(3, C::None)],
    },
];

const WORKPLACE: &[StateDef] = &[
    StateDef {
        id: 0,
        enter: C::WorkPlaceEnter0,
        update: C::None,
        exit: C::None,
        transitions: &[(1, C::None), (3, C::None)],
    },
    StateDef {
        id: 2,
        enter: C::WorkPlaceEnter2,
        update: C::None,
        exit: C::None,
        transitions: &[(3, C::None)],
    },
    StateDef {
        id: 3,
        enter: C::WorkPlaceEnter3,
        update: C::None,
        exit: C::None,
        transitions: &[(6, C::None), (4, C::None), (5, C::None)],
    },
];

const BUILD: &[StateDef] = &[
    StateDef {
        id: 1,
        enter: C::BuildEnter1,
        update: C::BuildUpdate1,
        exit: C::None,
        transitions: &[(8, C::None), (3, C::None)],
    },
    StateDef {
        id: 8,
        enter: C::BuildEnter8,
        update: C::BuildUpdate8,
        exit: C::BuildExit8,
        transitions: &[(2, C::BuildFinish), (3, C::None)],
    },
    StateDef {
        id: 2,
        enter: C::None,
        update: C::None,
        exit: C::None,
        transitions: &[(3, C::None)],
    },
];

const RESIDENCE: &[StateDef] = &[
    StateDef {
        id: 2,
        enter: C::ResidenceEnter2,
        // Not ported: `FUN_14012ee40` (households moving in).
        update: C::Missing,
        exit: C::None,
        transitions: &[(3, C::None)],
    },
    StateDef {
        id: 3,
        enter: C::Missing,
        update: C::None,
        exit: C::None,
        transitions: &[(6, C::None), (4, C::None), (5, C::None)],
    },
];

impl Controller {
    pub fn table(self) -> &'static [StateDef] {
        match self {
            Controller::ClearArea => CLEAR,
            Controller::WorkPlace => WORKPLACE,
            Controller::Build => BUILD,
            Controller::Residence => RESIDENCE,
        }
    }

    /// Slot `+0x80`: the initial state the first controller gives (ClearArea `0`,
    /// `FUN_140003730`; Build `1`, `FUN_14002fc50`).
    pub fn initial(self) -> i8 {
        match self {
            Controller::Build => 1,
            _ => 0,
        }
    }

    /// Interface slot `+0`: the state's index.
    pub fn find(self, state: i8) -> Option<usize> {
        self.table().iter().position(|d| d.id == state)
    }
}

/// The runner's fields (`StateMachineComponent +0x48`): the current interface `+0`, state
/// `+8`, its index `+9`, pending state `+10`, pending transition `+0xb`.
#[derive(Debug, Clone, Default)]
pub struct Machine {
    pub controllers: Vec<Controller>,
    pub current: usize,
    pub state: i8,
    pub index: i8,
    pub pending: i8,
    pub transition: i8,
}

impl Machine {
    /// `FUN_140130f00` + setup `FUN_140130b70`: pending = the first controller's initial state.
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
    fn find_from(&self, mut node: usize, state: i8) -> Option<(usize, usize)> {
        loop {
            if let Some(i) = self.controllers[node].find(state) {
                return Some((node, i));
            }
            node = node.checked_sub(1)?;
        }
    }

    /// `FUN_1400aef90`: look `msg` up in the current state's transitions.
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

/// What a callback needs to run, borrowed from the town (see [`crate::town::Sim`]).
pub trait Callbacks {
    fn call(&mut self, jobs: &mut Jobs, site: usize, f: Callback) -> bool;
    fn machine(&mut self, site: usize) -> &mut Machine;
}

/// `FUN_1400aeb30(machine, time, scheduled, update)`.
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

/// The parts of a building's descriptions construction reads.
#[derive(Debug, Clone, Default)]
pub struct SiteTemplate {
    /// `Template\X.rsc:resource`.
    pub reference: String,
    /// The controllers in component order.
    pub controllers: Vec<Controller>,
    /// BuildDescription `+0x20`: `(count, raw material)`; `+0x30` work.
    pub materials: Vec<(i32, usize)>,
    pub work: i32,
    /// ClearAreaDescription: present, `+0x2c` flatten/stamp flag.
    pub clear: Option<banished_formats::game::ClearAreaDescription>,
    /// WorkDescription `+0x24` (most workers once built).
    pub max_workers: i32,
    /// WorkPlaceDescription `+0x20`.
    pub profession: Option<usize>,
    /// StorageDescription: accepted flags, capacity, per footprint cell.
    pub storage: Option<(u32, i32, bool)>,
    /// ModelDescription meshes per variant (construction stages, the last finished).
    pub meshes: Vec<usize>,
}

/// One building under construction (and, once built, its WorkComponent).
#[derive(Debug, Clone)]
pub struct Site {
    pub entity: usize,
    /// Index into [`Construction::templates`].
    pub template: usize,
    pub machine: Machine,
    /// BuildComponent `+0x48` progress, `+0x4c` stages.
    pub progress: i32,
    pub stages: i32,
    /// WorkComponent `+0x28` registered work id (−1) and `+0x2c` kept priority.
    pub work_id: i32,
    pub work_priority: Option<i32>,
    /// The building's StorageComponent (index into the town's storages).
    pub storage: usize,
    /// State 2 reached.
    pub complete: bool,
}

/// The job manager, professions, workforce and every construction site.
#[derive(Debug, Clone, Default)]
pub struct Construction {
    pub jobs: Jobs,
    pub professions: Professions,
    pub workforce: Workforce,
    pub templates: Vec<SiteTemplate>,
    /// Entity → site.
    pub sites: BTreeMap<usize, Site>,
    /// `Game +0xf0 → +0xd0`: sites in their gathering and building states.
    pub site_list: Vec<usize>,
    /// Sites placed since the last iteration (activated before the next one).
    pub placed: Vec<usize>,
    /// Player commands for the next iteration.
    pub commands: Vec<Command>,
}

/// `GatherRawMaterials` state (`+0x48` delivered per requirement, `+0x58` in transit, `+0x68`
/// workers).
#[derive(Debug, Clone, Default)]
pub struct Gather {
    pub stored: Vec<i32>,
    pub transit: Vec<i32>,
    pub workers: BTreeSet<usize>,
}

/// `BuildStructure` state (`+0x48` destroy flag, `+0x4c` point cursor, `+0x50` occupied build
/// points, `+0x60` workers).
#[derive(Debug, Clone, Default)]
pub struct BuildWork {
    pub destroy: bool,
    pub cursor: i32,
    pub occupied: Vec<bool>,
    pub workers: BTreeSet<usize>,
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Host {
        machine: Machine,
        log: Vec<Callback>,
        clear: bool,
    }

    impl Callbacks for Host {
        fn call(&mut self, _: &mut Jobs, _: usize, f: Callback) -> bool {
            if f == C::None {
                return false;
            }
            self.log.push(f);
            match f {
                C::ClearEnter0 | C::ClearUpdate0 => {
                    if self.clear {
                        self.machine.request(1);
                    }
                    f == C::ClearEnter0
                }
                C::BuildEnter1 => {
                    self.machine.request(8);
                    true
                }
                C::BuildEnter8 => {
                    self.machine.request(2);
                    true
                }
                C::WorkPlaceEnter0 | C::WorkPlaceEnter2 | C::ResidenceEnter2 => false,
                _ => true,
            }
        }
        fn machine(&mut self, _: usize) -> &mut Machine {
            &mut self.machine
        }
    }

    #[test]
    fn house_states_run_in_order_through_the_chain() {
        use Controller::*;
        let mut h = Host {
            machine: Machine::new(vec![ClearArea, WorkPlace, Build, Residence]),
            log: Vec::new(),
            clear: false,
        };
        let mut jobs = Jobs::default();
        run(&mut h, &mut jobs, 0, false, false);
        assert_eq!(h.log, vec![C::WorkPlaceEnter0, C::ClearEnter0]);
        assert_eq!(h.machine.state, 0);
        h.clear = true;
        h.log.clear();
        run(&mut h, &mut jobs, 0, true, true);
        assert_eq!(
            h.log,
            vec![
                C::ClearUpdate0,
                C::ClearExit0,
                C::BuildEnter1,
                C::BuildEnter8,
                C::BuildExit8,
                C::BuildFinish,
                C::ResidenceEnter2,
                C::WorkPlaceEnter2,
                C::ClearEnter2,
            ]
        );
        assert_eq!(h.machine.state, 2);
    }
}
