//! A site's attendance, piles and the work it offers: clearing, pickup, gathering and building.

use super::*;

// vtable 0x14015ee00
/// AttendanceComponent: who attends a school, chapel or doctor.
#[derive(Debug, Clone, PartialEq)]
pub struct Attendance {
    // +0x20
    /// AttendanceDescription.
    pub capacity: i32,
    // +0x28
    /// Attendees in arrival order.
    pub attendees: Vec<usize>,
    /// The workplace's profession (teacher, priest, doctor).
    pub profession: Option<usize>,
}

// vtable 0x14016a5a0
/// `RawMaterialComponent`: a loose pile.
///
/// Original: `FUN_14012e8d0`.
#[derive(Debug, Clone, PartialEq)]
pub struct Pile {
    /// The raw material (RawMaterialDescription of the pile's template).
    pub material: usize,
    // +0x28
    /// Units.
    pub count: i32,
    // +0x38
    /// Age; after 3600 s the pile is destroyed.
    ///
    /// Original: `FUN_14012e600`.
    pub age: i64,
    /// The Pickup work its WorkComponent registered.
    ///
    /// Original: `FUN_14012e6a0`.
    pub work: i32,
    /// The work's category mask.
    ///
    /// Original: `FUN_14012e950`.
    pub mask: u32,
    /// A priority to keep from the work at its cell, applied on activation.
    ///
    /// Original: `FUN_1400fe220`.
    pub priority: Option<i32>,
    /// The workplace whose AutoPickup gets the Pickup work once registered.
    pub auto_pickup: Option<usize>,
    // +0x48; +0x50
    /// The Pickup work's (0 free, 1 claimed, 2 picked up) and (claimer). Kept on the pile: one
    /// Pickup try claims several piles' works.
    pub state: i32,
    /// The citizen who claimed the pile, if any.
    pub claimer: Option<usize>,
}

// +0x48; +0x58; +0x68; +0x70..+0x72
/// `ClearArea` state: claimed resources (entity → citizen), claimed cells, template filter,
/// resources / cells / piles.
#[derive(Debug, Clone, Default)]
pub struct ClearWork {
    /// The claimed resources: entity → citizen.
    pub resources: BTreeMap<usize, usize>,
    /// The claimed cells: cell → citizen.
    pub cells: BTreeMap<i32, usize>,
    /// Only resources of this template are cleared, when set.
    pub filter: Option<String>,
    /// Whether natural resources are cleared.
    pub clear_resources: bool,
    /// Whether cells are cleared.
    pub clear_cells: bool,
    /// Whether piles are cleared.
    pub clear_piles: bool,
}

// +0x48; +0x50
/// `Pickup` work: its state lives on the [`Pile`] it belongs to.
#[derive(Debug, Clone, Default)]
pub struct PickupWork;

// +0x48; +0x58; +0x68
/// `GatherRawMaterials` state (delivered per requirement, in transit, workers).
#[derive(Debug, Clone, Default)]
pub struct Gather {
    /// The materials delivered, for each requirement.
    pub stored: Vec<i32>,
    /// The materials on their way, for each requirement.
    pub transit: Vec<i32>,
    /// The workers.
    pub workers: BTreeSet<usize>,
}

// +0x48; +0x4c; +0x50; +0x60
/// `BuildStructure` state (destroy flag, point cursor, occupied build points, workers).
#[derive(Debug, Clone, Default)]
pub struct BuildWork {
    /// Whether this is demolition instead of construction.
    pub destroy: bool,
    /// The next build point to try.
    pub cursor: i32,
    /// Which build points are taken.
    pub occupied: Vec<bool>,
    /// The workers.
    pub workers: BTreeSet<usize>,
}
