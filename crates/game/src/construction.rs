//! Construction sites: the building state machine (StateMachineComponent with the
//! ClearArea, WorkPlace and Build controllers) and the construction works. See docs/JOBS.md.

use std::collections::BTreeSet;

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
