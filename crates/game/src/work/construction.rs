//! Construction sites: the building state machine (StateMachineComponent with the ClearArea,
//! WorkPlace, Build and Residence controllers) and the construction works (`GatherRawMaterials`,
//! `BuildStructure`). See docs/JOBS.md.
//!
//! A placed building's controllers are linked in component order; a state is handled by the last
//! controller that defines it, and a callback returning false passes it to the previous one. The
//! callbacks themselves live on [`crate::town::Sim`].
//!
//! Original: `FUN_1400aeb30`.

use std::collections::{BTreeMap, BTreeSet};

use crate::work::jobs::Jobs;
use crate::work::professions::{Command, Professions, Workforce};

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
    /// ClearResourceComponent (`FUN_140032150` / `FUN_140032270` table): the removal marker.
    ClearResource,
    /// ResourceProduceComponent (`FUN_140130700` table), here ConsumeProduceComponent.
    ResourceProduce,
    /// AttendanceComponent (`FUN_140044f90` table): school, chapel, doctor.
    Attendance,
    /// HealerComponent (`FUN_14004b980` table): the herbalist.
    Healer,
    /// HappinessComponent (`FUN_14004b570` table).
    Happiness,
    /// CropFieldComponent (`FUN_140120620` table).
    CropField,
    /// OrchardComponent (`FUN_140052a40` table).
    Orchard,
    /// PastureComponent (`FUN_14012d660` table).
    Pasture,
    /// AbsorbResourceComponent (`FUN_14002f5d0` table): a storage yard takes in the piles on
    /// its footprint.
    AbsorbResource,
    /// StorageLocationComponent (`FUN_1401328e0` table): a town storage.
    StorageLocation,
    /// CemeteryComponent (`FUN_140048100` table).
    Cemetery,
    /// MarketLocationComponent (`FUN_14004fa70` table): a town market.
    MarketLocation,
    /// DestroyComponent (`FUN_1401231d0` table): demolition, damage and rubble.
    Destroy,
    /// WellComponent (`FUN_14005da70` table): a water source for firefighters.
    Well,
    /// ImmigrationComponent (`FUN_14004f020` table): the town hall's nomads.
    Immigration,
    /// TradeComponent (`FUN_14005cfb0` table): the trading post's merchants.
    Trade,
    /// MultiObjectComponent (table `FUN_14012b000`): a bridge's or a pasture's parts.
    MultiObject,
}

// exit +0x20; DAT_1401dbca0; +8; +0x18
/// A state callback (records: enter, scheduled update, transition functions).
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
    /// `FUN_14011da30`: the marker destroys its entity once its cell is clear.
    ClearResourceEnter2,
    /// `FUN_140130680`: built → producing (0xd), or idle (0xe) when switched off.
    ProduceEnter2,
    /// `FUN_140130460` (transition 2 → 0xd): the output points' cells become empty ground.
    ProduceClearPoints,
    /// `FUN_140130330`: register the production work and enable the workplace (0xe when
    /// switched off).
    ProduceEnterD,
    /// `FUN_1401301f0`: remove the work and disable the workplace.
    ProduceEnterE,
    /// `FUN_140130660`: state 0xd's scheduled update → slot `+0xa8` (the dock's water).
    ProduceUpdateD,
    /// `FUN_140130440`: transition 0xd → 3 → slot `+0xa0` (the dock leaves the map layer).
    ProduceLeaveD,
    /// `FUN_1400445f0`: register the attendance, its LocationWork, enable the workplace.
    AttendanceEnter2,
    /// `FUN_140044590`: status icon 2 when every attendance of the kind is full.
    AttendanceExit2,
    /// `FUN_140044530`: unregister, send the attendees away, disable the workplace.
    AttendanceEnter3,
    /// `FUN_14004b790` / `FUN_14004b750`: register / unregister the healer.
    HealerEnter2,
    HealerEnter3,
    /// `FUN_14004b350` (states 2 and 0xd enter), `FUN_14004b300` (their scheduled update),
    /// `FUN_14004b2a0` (state 3 enter).
    HappinessEnter,
    HappinessUpdate,
    HappinessLeave,
    /// CropField: `FUN_14011fdf0` / `FUN_14011fd30` (state 2), `FUN_14011f7e0` /
    /// `FUN_14011f6b0` (9), `FUN_14011f5d0` / `FUN_14011f440` / `FUN_14011f4a0` (10),
    /// `FUN_14011f330` / `FUN_14011f250` (11).
    CropEnter2,
    CropUpdate2,
    CropEnter9,
    CropUpdate9,
    CropEnter10,
    CropExit10,
    CropUpdate10,
    CropEnter11,
    CropUpdate11,
    /// `FUN_14004f880` / `FUN_14004f840`: the market joins / leaves the town's (`+0x10`).
    MarketEnter2,
    MarketEnter3,
    /// `FUN_140047920` / `FUN_1400478e0`: the cemetery joins / leaves the town's.
    CemeteryEnter2,
    CemeteryEnter3,
    /// `FUN_14010ae20`: AbsorbResource's state 0 update.
    AbsorbUpdate0,
    /// `FUN_140132700` / `FUN_1401326c0`: join / leave the town's storages.
    StorageEnter2,
    StorageEnter3,
    /// Pasture: `FUN_14012c590` / `FUN_14012bcb0` (state 2), `FUN_14012c310` (3).
    PastureEnter2,
    PastureUpdate2,
    PastureEnter3,
    /// Orchard: `FUN_140051e90` / `FUN_140051e20` (2), `FUN_1400519d0` / `FUN_1400518f0` (9),
    /// `FUN_140051790` / `FUN_140051560` (10), `FUN_140051500` / `FUN_140051310` (exits of 10
    /// and 12), `FUN_140051440` / `FUN_140051370` (12), `FUN_140051160` / `FUN_140050fb0`
    /// (11), `FUN_140050e40` / `FUN_140050d40` / `FUN_140050d90` (0x14).
    OrchardEnter2,
    OrchardUpdate2,
    OrchardEnter9,
    OrchardUpdate9,
    OrchardEnter10,
    OrchardUpdate10,
    OrchardExitWork,
    OrchardEnter12,
    OrchardUpdate12,
    OrchardEnter11,
    OrchardUpdate11,
    OrchardEnter14,
    OrchardExit14,
    OrchardUpdate14,
    /// `FUN_14012edf0`: the families move out and the residence leaves the town's.
    ResidenceEnter3,
    /// Destroy: `FUN_140122c80` (3), `FUN_1401224a0` / `FUN_140122390` (4), `FUN_140122140` /
    /// `FUN_1401215f0` (5), `FUN_1401229a0` / `FUN_140122960` / `FUN_1401228d0` (6).
    DestroyEnter3,
    DestroyEnter4,
    DestroyUpdate4,
    DestroyEnter5,
    DestroyUpdate5,
    DestroyEnter6,
    DestroyUpdate6,
    DestroyExit6,
    /// `FUN_14005d880` / `FUN_14005d840`: the well joins / leaves the town's water sources.
    WellEnter2,
    WellEnter3,
    /// Immigration: `FUN_14004dbf0` / `FUN_14004db80` (2), `FUN_14004db00` / `FUN_14004da30`
    /// (0xf), `FUN_14004d9f0` / `FUN_14004d9d0` (0x10), `FUN_14004d810` / `FUN_14004d760`
    /// (0x11), `FUN_14004d660` (0x10 and 0x11 → 3).
    ImmigrationEnter2,
    ImmigrationUpdate2,
    ImmigrationEnterF,
    ImmigrationUpdateF,
    ImmigrationEnter10,
    ImmigrationUpdate10,
    ImmigrationEnter11,
    ImmigrationUpdate11,
    ImmigrationLeave,
    /// Trade: `FUN_14005b530` / `FUN_14005b3c0` (2), `FUN_14005b3b0` (0x12), `FUN_14005b160` /
    /// `FUN_14005b030` / `FUN_14005b0e0` (0x13), `FUN_14005bac0` (→ 3).
    TradeEnter2,
    TradeUpdate2,
    TradeEnter12,
    TradeEnter13,
    TradeExit13,
    TradeUpdate13,
    TradeLeave,
    /// `FUN_14012aa30`: activate the parts' MapComponents and floors.
    MultiObjectEnter2,
    /// Callbacks whose port is still missing (they pass the state on).
    Missing,
}

/// One state of a building's state machine: its callbacks and its transitions.
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
    // `FUN_14011ccb0` / `FUN_14011cca0` are thunks of state 0's enter and update.
    StateDef {
        id: 0x15,
        enter: C::ClearEnter0,
        update: C::ClearUpdate0,
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
        // `FUN_14012ee40` runs in `Agents::update` (`life::update_home`).
        update: C::None,
        exit: C::None,
        transitions: &[(3, C::None)],
    },
    StateDef {
        id: 3,
        enter: C::ResidenceEnter3,
        update: C::None,
        exit: C::None,
        transitions: &[(6, C::None), (4, C::None), (5, C::None)],
    },
];

/// The state messages (`FUN_140122860` in 6, `FUN_140122770` / `FUN_140122670` in 7,
/// `FUN_140121190` in 4 and 5) are [`crate::town::Sim::destroy_message`].
const DESTROY: &[StateDef] = &[
    StateDef {
        id: 3,
        enter: C::DestroyEnter3,
        update: C::None,
        exit: C::None,
        transitions: &[(4, C::None), (6, C::None)],
    },
    StateDef {
        id: 6,
        enter: C::DestroyEnter6,
        update: C::DestroyUpdate6,
        exit: C::DestroyExit6,
        transitions: &[(7, C::None)],
    },
    StateDef {
        id: 7,
        enter: C::None,
        update: C::None,
        exit: C::None,
        transitions: &[(0, C::None), (8, C::None), (5, C::None)],
    },
    StateDef {
        id: 4,
        enter: C::DestroyEnter4,
        update: C::DestroyUpdate4,
        exit: C::None,
        transitions: &[(5, C::None), (2, C::None)],
    },
    StateDef {
        id: 5,
        enter: C::DestroyEnter5,
        update: C::DestroyUpdate5,
        exit: C::None,
        transitions: &[(0, C::None), (2, C::None), (8, C::None), (7, C::None)],
    },
];

const WELL: &[StateDef] = &[
    StateDef {
        id: 2,
        enter: C::WellEnter2,
        update: C::None,
        exit: C::None,
        transitions: &[(3, C::None)],
    },
    StateDef {
        id: 3,
        enter: C::WellEnter3,
        update: C::None,
        exit: C::None,
        transitions: &[(6, C::None), (4, C::None), (5, C::None)],
    },
];

/// The state messages (6 in 0x10, 7 and 8 in 0x11) are
/// [`crate::town::Sim::immigration_message`].
const IMMIGRATION: &[StateDef] = &[
    StateDef {
        id: 2,
        enter: C::ImmigrationEnter2,
        update: C::ImmigrationUpdate2,
        exit: C::None,
        transitions: &[(3, C::None), (0xf, C::None)],
    },
    StateDef {
        id: 0xf,
        enter: C::ImmigrationEnterF,
        update: C::ImmigrationUpdateF,
        exit: C::None,
        transitions: &[(3, C::None), (0x10, C::None), (2, C::None)],
    },
    StateDef {
        id: 0x10,
        enter: C::ImmigrationEnter10,
        update: C::ImmigrationUpdate10,
        exit: C::None,
        transitions: &[(3, C::ImmigrationLeave), (0x11, C::None)],
    },
    StateDef {
        id: 0x11,
        enter: C::ImmigrationEnter11,
        update: C::ImmigrationUpdate11,
        exit: C::None,
        transitions: &[(3, C::ImmigrationLeave), (2, C::None)],
    },
];

/// The state messages (9 and 0xb in 0x12, 0xa in 0x13) are
/// [`crate::town::Sim::trade_message`].
/// `FUN_14012b000` (`FUN_140036a50`, `FUN_140036b70`): state 2 only.
const MULTI_OBJECT: &[StateDef] = &[StateDef {
    id: 2,
    enter: C::MultiObjectEnter2,
    update: C::None,
    exit: C::None,
    transitions: &[(3, C::None)],
}];

const TRADE: &[StateDef] = &[
    StateDef {
        id: 2,
        enter: C::TradeEnter2,
        update: C::TradeUpdate2,
        exit: C::None,
        transitions: &[(3, C::TradeLeave), (0x12, C::None)],
    },
    StateDef {
        id: 0x12,
        enter: C::TradeEnter12,
        update: C::None,
        exit: C::None,
        transitions: &[(3, C::TradeLeave), (0x13, C::None), (2, C::None)],
    },
    StateDef {
        id: 0x13,
        enter: C::TradeEnter13,
        update: C::TradeUpdate13,
        exit: C::TradeExit13,
        transitions: &[(3, C::TradeLeave), (2, C::None)],
    },
];

const ATTENDANCE: &[StateDef] = &[
    StateDef {
        id: 2,
        enter: C::AttendanceEnter2,
        update: C::None,
        exit: C::AttendanceExit2,
        transitions: &[(3, C::None)],
    },
    StateDef {
        id: 3,
        enter: C::AttendanceEnter3,
        update: C::None,
        exit: C::None,
        transitions: &[(6, C::None), (4, C::None), (5, C::None)],
    },
];

const HEALER: &[StateDef] = &[
    StateDef {
        id: 2,
        enter: C::HealerEnter2,
        update: C::None,
        exit: C::None,
        transitions: &[(3, C::None), (0xd, C::None)],
    },
    StateDef {
        id: 3,
        enter: C::HealerEnter3,
        update: C::None,
        exit: C::None,
        transitions: &[(6, C::None), (4, C::None), (5, C::None)],
    },
];

const CROP_FIELD: &[StateDef] = &[
    StateDef {
        id: 2,
        enter: C::CropEnter2,
        update: C::CropUpdate2,
        exit: C::None,
        transitions: &[(3, C::None), (9, C::None)],
    },
    StateDef {
        id: 9,
        enter: C::CropEnter9,
        update: C::CropUpdate9,
        exit: C::None,
        transitions: &[(3, C::None), (0xa, C::None)],
    },
    StateDef {
        id: 0xa,
        enter: C::CropEnter10,
        update: C::CropUpdate10,
        exit: C::CropExit10,
        transitions: &[(3, C::None), (0x15, C::None), (0xb, C::None)],
    },
    StateDef {
        id: 0xb,
        enter: C::CropEnter11,
        update: C::CropUpdate11,
        exit: C::None,
        transitions: &[(3, C::None), (0x15, C::None)],
    },
];

const ORCHARD: &[StateDef] = &[
    StateDef {
        id: 2,
        enter: C::OrchardEnter2,
        update: C::OrchardUpdate2,
        exit: C::None,
        transitions: &[(3, C::None), (9, C::None)],
    },
    StateDef {
        id: 9,
        enter: C::OrchardEnter9,
        update: C::OrchardUpdate9,
        exit: C::None,
        transitions: &[(3, C::None), (0xa, C::None), (0x14, C::None)],
    },
    StateDef {
        id: 0xa,
        enter: C::OrchardEnter10,
        update: C::OrchardUpdate10,
        exit: C::OrchardExitWork,
        transitions: &[
            (3, C::None),
            (0xc, C::None),
            (0xb, C::None),
            (0x14, C::None),
        ],
    },
    StateDef {
        id: 0xc,
        enter: C::OrchardEnter12,
        update: C::OrchardUpdate12,
        exit: C::OrchardExitWork,
        transitions: &[
            (3, C::None),
            (0xa, C::None),
            (0xb, C::None),
            (0x14, C::None),
        ],
    },
    StateDef {
        id: 0xb,
        enter: C::OrchardEnter11,
        update: C::OrchardUpdate11,
        exit: C::None,
        transitions: &[(3, C::None), (0xa, C::None), (0x14, C::None)],
    },
    StateDef {
        id: 0x14,
        enter: C::OrchardEnter14,
        update: C::OrchardUpdate14,
        exit: C::OrchardExit14,
        transitions: &[(3, C::None), (9, C::None)],
    },
];

const MARKET_LOCATION: &[StateDef] = &[
    StateDef {
        id: 2,
        enter: C::MarketEnter2,
        update: C::None,
        exit: C::None,
        transitions: &[(3, C::None)],
    },
    StateDef {
        id: 3,
        enter: C::MarketEnter3,
        update: C::None,
        exit: C::None,
        transitions: &[(6, C::None), (4, C::None), (5, C::None)],
    },
];

const CEMETERY: &[StateDef] = &[
    StateDef {
        id: 2,
        enter: C::CemeteryEnter2,
        update: C::None,
        exit: C::None,
        transitions: &[(3, C::None)],
    },
    StateDef {
        id: 3,
        enter: C::CemeteryEnter3,
        update: C::None,
        exit: C::None,
        transitions: &[(6, C::None), (4, C::None), (5, C::None)],
    },
];

const ABSORB_RESOURCE: &[StateDef] = &[StateDef {
    id: 0,
    enter: C::None,
    update: C::AbsorbUpdate0,
    exit: C::None,
    transitions: &[],
}];

const STORAGE_LOCATION: &[StateDef] = &[
    StateDef {
        id: 2,
        enter: C::StorageEnter2,
        update: C::None,
        exit: C::None,
        transitions: &[(3, C::None)],
    },
    StateDef {
        id: 3,
        enter: C::StorageEnter3,
        update: C::None,
        exit: C::None,
        transitions: &[(6, C::None), (4, C::None), (5, C::None)],
    },
];

const PASTURE: &[StateDef] = &[
    StateDef {
        id: 2,
        enter: C::PastureEnter2,
        update: C::PastureUpdate2,
        exit: C::None,
        transitions: &[(3, C::None)],
    },
    StateDef {
        id: 3,
        enter: C::PastureEnter3,
        update: C::None,
        exit: C::None,
        transitions: &[(6, C::None), (4, C::None), (5, C::None)],
    },
];

const HAPPINESS: &[StateDef] = &[
    StateDef {
        id: 2,
        enter: C::HappinessEnter,
        update: C::HappinessUpdate,
        exit: C::None,
        transitions: &[(3, C::None)],
    },
    StateDef {
        id: 0xd,
        enter: C::HappinessEnter,
        update: C::HappinessUpdate,
        exit: C::None,
        transitions: &[(3, C::None)],
    },
    StateDef {
        id: 3,
        enter: C::HappinessLeave,
        update: C::None,
        exit: C::None,
        transitions: &[(6, C::None), (4, C::None), (5, C::None)],
    },
];

const CLEAR_RESOURCE: &[StateDef] = &[StateDef {
    id: 2,
    enter: C::ClearResourceEnter2,
    update: C::None,
    exit: C::None,
    transitions: &[(3, C::None)],
}];

const RESOURCE_PRODUCE: &[StateDef] = &[
    StateDef {
        id: 2,
        enter: C::ProduceEnter2,
        update: C::None,
        exit: C::None,
        transitions: &[(0xd, C::ProduceClearPoints), (0xe, C::None), (3, C::None)],
    },
    StateDef {
        id: 0xd,
        enter: C::ProduceEnterD,
        update: C::ProduceUpdateD,
        exit: C::None,
        transitions: &[(0xe, C::None), (3, C::ProduceLeaveD)],
    },
    StateDef {
        id: 0xe,
        enter: C::ProduceEnterE,
        update: C::None,
        exit: C::None,
        transitions: &[(0xd, C::None), (3, C::None)],
    },
];

impl Controller {
    /// The states of a controller.
    pub fn table(self) -> &'static [StateDef] {
        match self {
            Controller::ClearArea => CLEAR,
            Controller::WorkPlace => WORKPLACE,
            Controller::Build => BUILD,
            Controller::Residence => RESIDENCE,
            Controller::ClearResource => CLEAR_RESOURCE,
            Controller::ResourceProduce => RESOURCE_PRODUCE,
            Controller::Attendance => ATTENDANCE,
            Controller::Healer => HEALER,
            Controller::Happiness => HAPPINESS,
            Controller::CropField => CROP_FIELD,
            Controller::Orchard => ORCHARD,
            Controller::Pasture => PASTURE,
            Controller::AbsorbResource => ABSORB_RESOURCE,
            Controller::StorageLocation => STORAGE_LOCATION,
            Controller::Cemetery => CEMETERY,
            Controller::MarketLocation => MARKET_LOCATION,
            Controller::Destroy => DESTROY,
            Controller::Well => WELL,
            Controller::Immigration => IMMIGRATION,
            Controller::Trade => TRADE,
            Controller::MultiObject => MULTI_OBJECT,
        }
    }

    // +0x80
    /// The initial state the first controller gives (ClearArea `0`, Build `1`).
    ///
    /// Original: `FUN_140003730`, `FUN_14002fc50`.
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

// StateMachineComponent +0x48; +0; +8; +9; +10; +0xb
/// The runner's fields (`StateMachineComponent`): the current interface, state, its index, pending
/// state, pending transition.
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
    fn find_from(&self, mut node: usize, state: i8) -> Option<(usize, usize)> {
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

/// What a callback needs to run, borrowed from the town (see [`crate::town::Sim`]).
pub trait Callbacks {
    /// Run a callback. Returns false to pass the state on to the previous controller.
    fn call(&mut self, jobs: &mut Jobs, site: usize, f: Callback) -> bool;
    /// A site's state machine.
    fn machine(&mut self, site: usize) -> &mut Machine;
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

/// The parts of a building's descriptions construction reads.
#[derive(Debug, Clone, Default)]
pub struct SiteTemplate {
    /// `Template\X.rsc:resource`.
    pub reference: String,
    /// The controllers in component order.
    pub controllers: Vec<Controller>,
    // +0x20; +0x30
    /// BuildDescription: `(count, raw material)`; work.
    pub materials: Vec<(i32, usize)>,
    pub work: i32,
    // +0x2c
    /// ClearAreaDescription: present, flatten/stamp flag.
    pub clear: Option<banished_formats::game::ClearAreaDescription>,
    // +0x24
    /// WorkDescription (most workers once built).
    pub max_workers: i32,
    // +0x34; +0x30; +0x24
    /// WorkDescription /: scale by the footprint area over this many cells.
    pub per_area: Option<i32>,
    // +0x20; +0x28; +0x2c
    /// WorkDescription: the player may change the most workers, between and (WorkPlaceUI).
    pub adjustable: bool,
    pub min_jobs: i32,
    pub max_jobs: i32,
    // +0x20
    /// WorkPlaceDescription.
    pub profession: Option<usize>,
    /// ConsumeProduceDescription, resolved.
    pub produce: Option<ProduceTemplate>,
    /// StorageDescription: accepted flags, capacity, per footprint cell.
    pub storage: Option<(u32, i32, bool)>,
    /// ModelDescription meshes per variant (construction stages, the last finished).
    pub meshes: Vec<usize>,
    /// ResidenceDescription.
    pub residence: Option<banished_formats::game::ResidenceDescription>,
    /// AttendanceDescription capacity.
    pub attendance: Option<i32>,
    /// HerbalistDescription (gatherer, herbalist), resolved.
    pub herbalist: Option<crate::production::herbalist::HerbalistTemplate>,
    // +0x28
    /// RadiusDescription.
    pub radius: Option<i32>,
    // +0x20
    /// AutoPickupDescription.
    pub auto_pickup: Option<bool>,
    /// ForesterDescription, resolved.
    pub forester: Option<crate::production::forester::ForesterTemplate>,
    /// HappinessDescription.
    pub happiness: Option<banished_formats::game::HappinessDescription>,
    /// FishingDescription, resolved.
    pub fishing: Option<crate::production::fishing::FishingTemplate>,
    /// HuntDescription.
    pub hunt: Option<banished_formats::game::HuntDescription>,
    /// CropFieldDescription.
    pub crop_field: Option<crate::production::farm::CropFieldTemplate>,
    /// OrchardDescription.
    pub orchard: Option<crate::production::orchard::OrchardTemplate>,
    // +0x20
    /// PastureDescription: the stock slot of its products' limit.
    pub pasture: Option<i32>,
    /// DiseaseDescription (fields, orchards, pastures).
    pub disease: Option<banished_formats::game::DiseaseDescription>,
    /// CemeteryDescription's tombstone template and its TombstoneDescription.
    pub cemetery: Option<crate::people::cemetery::CemeteryTemplate>,
    /// MarketDescription.
    pub market: Option<banished_formats::game::MarketDescription>,
    // ModelComponent +0x5c
    /// StatusIconDescription (presentation), and the finished model's height its icons stand on
    /// (`ModelComponent`).
    pub status_icon: Option<banished_formats::game::StatusIconDescription>,
    pub icon_height: Option<f32>,
    /// DestroyDescription, resolved.
    pub destroy: Option<crate::work::destroy::DestroyTemplate>,
    /// FireDescription (`Template\Common.rsc:fire*`).
    pub fire: Option<banished_formats::game::FireDescription>,
    /// ImmigrationDescription, resolved.
    pub immigration: Option<crate::people::immigration::ImmigrationTemplate>,
    /// TradeDescription, resolved.
    pub trade: Option<crate::commerce::trade::TradeTemplate>,
    // +0x68; +0x6c
    /// TradeDescription /: the visitors' disease risk trading brings, and its months.
    pub trade_risk: Option<(f32, i32)>,
    /// The pile template of each BuildDescription material (the refunds' piles).
    pub material_templates: Vec<Option<usize>>,
}

impl SiteTemplate {
    /// The default number of workers of a building of `footprint` cells.
    ///
    /// Original: `WorkComponent::MaxWorkers`, `FUN_140133a90`.
    pub fn default_max_workers(&self, footprint: (i32, i32)) -> i32 {
        let Some(area) = self.per_area else {
            return self.max_workers;
        };
        let n = self.max_workers;
        // `DAT_14017884c` = 1.0.
        let scaled = ((footprint.0 * footprint.1) as f32 / area as f32 * n as f32 + 1.0) as i32;
        if scaled <= n {
            scaled.max(1)
        } else {
            n
        }
    }
}

/// A resolved [`banished_formats::game::ConsumeProduceDescription`].
#[derive(Debug, Clone, Default)]
pub struct ProduceTemplate {
    pub recipes: Vec<Recipe>,
    // +0x40
    /// Work cycles per production.
    pub cycles: i32,
    // +0x50
    /// Work type (tool mesh, animation, walk).
    pub work_type: u32,
    // +0x38/+0x3c
    /// A message to the building's component of that type after a production.
    pub message: (u32, i32),
    // +0x44
    /// Seconds per cycle.
    pub duration: f32,
    // +0x48
    /// Productions before exhaustion (−1 never).
    pub lifetime: i32,
    // +0x4c
    /// Initial selector.
    pub selector: i32,
}

/// A production alternative: inputs (raw material, count), the output pile template and its
/// raw material, and the stock group the limit check reads.
#[derive(Debug, Clone, Default)]
pub struct Recipe {
    pub inputs: Vec<(usize, i32)>,
    pub output: usize,
    pub output_material: usize,
    pub group: i32,
}

// +0x48; +0x50; +0x58; +0x68
/// ConsumeProduceComponent state (switched off, selector, weights, productions so far).
#[derive(Debug, Clone, Default)]
pub struct ProduceState {
    pub disabled: bool,
    pub selector: i32,
    pub weights: Vec<f32>,
    pub produced: i32,
}

// +0x58; +0x68; +0x48; +0x4c
/// `ConsumeProduce` work state: the worker at each use point, each output point's state (0 free, 1
/// a production is under way, 2 holds output), every output point was taken, work cycles towards
/// the next production.
#[derive(Debug, Clone, Default)]
pub struct ProduceWork {
    pub workers: Vec<Option<usize>>,
    pub points: Vec<u8>,
    pub full: bool,
    pub cycles: i32,
}

// vtable 0x140169400; +0x30; +0x28; +0x2c
/// DecalComponent (0x38 bytes, serialized alpha, material): setup takes the description's alpha;
/// entity event 1 (the ClearArea's state 2) and the build's state 8 make it opaque; a collapse
/// switches to the DestroyDescription's material. Each change re-adds the map decal. See
/// docs/DECALS.md.
///
/// Original: `FUN_140120ca0`, `FUN_14011ccc0`, `FUN_14010cfb0`, `FUN_140122aa0`, `FUN_140120c30`.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Decal {
    // +0x30
    /// is 1.
    pub opaque: bool,
    // +0x28
    /// Index into the description's materials.
    pub material: i32,
}

/// One building under construction (and, once built, its WorkComponent).
#[derive(Debug, Clone)]
pub struct Site {
    pub entity: usize,
    /// Index into [`Construction::templates`].
    pub template: usize,
    pub machine: Machine,
    // +0x48; +0x4c
    /// BuildComponent progress, stages.
    pub progress: i32,
    pub stages: i32,
    // +0x28; +0x2c
    /// WorkComponent registered work id (−1) and kept priority.
    pub work_id: i32,
    pub work_priority: Option<i32>,
    /// The building's StorageComponent (index into the town's storages).
    pub storage: usize,
    /// State 2 reached.
    pub complete: bool,
    // +0x48
    /// ClearAreaComponent: objects on the footprint when clearing began.
    pub clear_total: i32,
    /// The entity was destroyed during a state callback; the site goes once the machine has run.
    ///
    /// Original: `FUN_1400c8370`.
    pub destroyed: bool,
    // +0x48
    /// ConsumeProduceComponent (ResourceProduceComponent's for the others).
    pub produce: Option<ProduceState>,
    /// AutoPickupComponent.
    pub auto_pickup: Option<crate::production::herbalist::AutoPickup>,
    /// ForesterComponent's toggles.
    pub forester: Option<crate::production::forester::Forester>,
    // +0x28
    /// HappinessComponent: registered.
    pub happiness_registered: bool,
    /// FishingComponent.
    pub fishing: Option<crate::production::fishing::Fishing>,
    // +0x50
    /// HuntComponent: the hunting time gathered.
    pub hunt_time: Option<i64>,
    /// CropFieldComponent.
    pub crop_field: Option<crate::production::farm::CropField>,
    /// OrchardComponent.
    pub orchard: Option<crate::production::orchard::Orchard>,
    // +0x48
    /// MultiObjectComponent: the entities placed with this one (a pasture's shelter and fences), in
    /// order.
    pub parts: Vec<usize>,
    // +0x58; +0x4c
    /// MultiObjectComponent: the stages its construction counts (a bridge's cells, a fenced area's
    /// border cells); `BuildComponent::EnterGather` takes it for.
    pub multi_stages: Option<i32>,
    /// DiseaseComponent.
    pub disease: Option<crate::disasters::blight::Blight>,
    // +0x48
    /// CemeteryComponent: the grave cells.
    ///
    /// Original: `FUN_140047b60`.
    pub graves: Vec<i32>,
    /// DestroyComponent.
    pub destroy: Option<crate::work::destroy::Destroy>,
    /// FireComponent.
    pub fire: Option<crate::disasters::fire::Fire>,
    /// ImmigrationComponent.
    pub immigration: Option<crate::people::immigration::Immigration>,
    /// TradeComponent.
    pub trade: Option<crate::commerce::trade::Trade>,
    /// The ResidenceComponent while the residence is out of the town's (its states 3 to 7).
    pub home: Option<crate::people::life::Home>,
    // +0x28; +0x30; +0x20
    /// UIComponent (bytes, the UIDescription's): the window's remembered settings
    /// (`DialogInstance::SaveSettings`), empty until it is first closed.
    pub ui_settings: Vec<u8>,
}

/// The job manager, professions, workforce and every construction site.
#[derive(Debug, Clone, Default)]
pub struct Construction {
    // +0x1b8; +0x190
    /// The town's commute optimisation (StockManager).
    pub commute: crate::behaviour::commute::Commute,
    pub jobs: Jobs,
    pub professions: Professions,
    pub workforce: Workforce,
    pub templates: Vec<SiteTemplate>,
    /// Entity → site.
    pub sites: BTreeMap<usize, Site>,
    // Game +0xf0 → +0xd0
    /// `Game →`: sites in their gathering and building states.
    pub site_list: Vec<usize>,
    /// Sites placed since the last iteration (activated before the next one).
    pub placed: Vec<usize>,
    /// Player commands for the next iteration.
    pub commands: Vec<Command>,
    /// Loose raw-material piles (RawMaterialComponent), by entity.
    pub piles: BTreeMap<usize, Pile>,
    /// NaturalResourceComponent flag `0x100000`: a removal sequence holds the resource.
    pub claimed_resources: BTreeSet<usize>,
    /// Piles created since the last iteration (registered with the entity manager and
    /// activated after the citizens' update).
    pub new_piles: Vec<usize>,
    /// Entities were created or removed (the models need refreshing).
    pub models_dirty: bool,
    /// Entities the player destroyed (RemovalTool cancel), before the next iteration.
    pub destroy: Vec<usize>,
    /// Finished residences' ResidenceComponents, by entity.
    pub homes: BTreeMap<usize, crate::people::life::Home>,
    // +0x60
    /// AttendanceComponents, by entity, and the town manager's registry.
    pub attendance: BTreeMap<usize, Attendance>,
    pub attendance_list: Vec<usize>,
    // +0x70
    /// The town manager's HealerComponents.
    pub healers: Vec<usize>,
    // +0x50
    /// The town manager's registered HappinessComponents, by entity.
    pub happiness: Vec<usize>,
    // Game +0x128 → +8; +0
    /// The calendar's seconds into the year, set before each iteration.
    pub calendar: f32,
    /// The calendar's year, set before each iteration.
    pub year: u32,
    /// FruitComponents of orchard trees, by entity.
    pub fruits: BTreeMap<usize, crate::production::orchard::Fruit>,
    // +0x2d
    /// NaturalResourceComponent set by a crop field: whether harvesting leaves a pile.
    pub keep_piles: BTreeMap<usize, bool>,
    /// Event-log entries the sites posted: type, severity, entity; the game formats them with the
    /// entity's toolbar name.
    ///
    /// Original: `FUN_14009a2d0`.
    pub site_events: Vec<(i32, i32, usize)>,
    // +0x00
    /// StorageLocationComponents joining (`true`) or leaving the town's storages, in order; the
    /// town applies them after the iteration's update.
    pub storage_changes: Vec<(usize, bool)>,
    // +0x40
    /// The town's cemeteries, in order.
    pub cemeteries: Vec<usize>,
    // +0x28
    /// TombstoneComponents and the time each has left.
    pub tombstones: BTreeMap<usize, i64>,
    /// Burials decided at a death (cemetery, grave cell), placed after the citizens' update.
    pub burials: Vec<(usize, i32)>,
    /// Entities a DestroyComponent puts in a torn-down building's place, created by the town after
    /// the update.
    ///
    /// Original: `FUN_1400c9d70`.
    pub rebuilds: Vec<crate::work::destroy::Rebuild>,
    /// The town's water sources (built with the map).
    ///
    /// Original: `FUN_1400a43f0`.
    pub water: crate::disasters::fire::Water,
    // Game +0xd0; FUN_14009d530 sets 0xf0 with disasters off, else 0.
    /// The game's flags (for example, disasters off).
    pub game_flags: u32,
    /// The sky's state is precipitation, set before each iteration.
    pub precipitation: bool,
    // +0xa8
    /// Town manager: the population, set before each update.
    pub population: i32,
    // +0x1a8; +0x1a0
    /// Town manager /: the visitors' disease risk and how long it lasts.
    pub visitors: (f32, i64),
    // +0x1b0
    /// Town manager: nomads taken in.
    pub nomads_admitted: i32,
    // +0x30
    /// Town manager: the trading posts.
    pub trading_posts: Vec<usize>,
    // +0x1ac
    /// Town manager: the value traded away.
    pub trade_value: i32,
    // +0xe8
    /// Map: the cells with a planned road, and their BuildRoad works.
    pub pending_roads: BTreeMap<i32, i32>,
    /// Tunnels to dig or fill back (TerraformComponent event 4 and removal), for the terrain's
    /// owner, and each terraformed entity's kept heights and cells.
    pub terraform_requests: Vec<crate::world::terraform::Terraform>,
    /// Requests to flatten the ground: cell rectangles (`x0, y0, x1, y1`, inclusive) whose
    /// cells' low corners go to height 0. They are applied to the terrain after the iteration.
    ///
    /// Original: `FUN_1400a35b0`.
    pub flatten_requests: Vec<[i32; 4]>,
    /// The tornadoes by entity, and the citizens they killed this iteration (death code 5; the
    /// citizens' pass applies it).
    pub tornadoes: BTreeMap<usize, crate::disasters::tornado::Tornado>,
    pub tornado_kills: Vec<usize>,
    /// TrackerComponents by entity, and the stock manager's made/used counters.
    pub trackers: BTreeMap<usize, crate::work::tracker::Tracker>,
    /// DecalComponents that left their setup state, by entity (the others show at their
    /// description's alpha with material 0).
    pub decals: BTreeMap<usize, Decal>,
    pub counters: crate::work::tracker::StockCounters,
    pub terraforms: BTreeMap<usize, crate::world::terraform::TerraformState>,
    /// Built road cells Remove Roads marked, for the town to place their markers (cell,
    /// marker template).
    pub road_markers: Vec<(i32, usize)>,
    /// The goods with a ValueDescription, in the factory's order (`GameResources`).
    pub trade_goods: Vec<crate::commerce::trade::TradeGood>,
    // +0x1e0
    /// MapComponent 's first river centre line, the trader boats' way.
    pub river: Vec<[f32; 2]>,
    /// `Game/Names/Male` and `Female`, for the merchants' names.
    pub names: Option<
        std::sync::Arc<(
            banished_formats::names::NameDatabase,
            banished_formats::names::NameDatabase,
        )>,
    >,
    /// The town manager's known crops, orchard trees and livestock, as placed-object templates.
    ///
    /// Original: `FUN_140096ef0`.
    pub known: Vec<usize>,
    // +0x110
    /// The stock manager's known raw materials: the start profile's, then each as its first pile is
    /// set up or it is traded for.
    ///
    /// Original: `FUN_140097330`.
    pub known_materials: Vec<usize>,
}

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
    pub claimer: Option<usize>,
}

// +0x48; +0x58; +0x68; +0x70..+0x72
/// `ClearArea` state: claimed resources (entity → citizen), claimed cells, template filter,
/// resources / cells / piles.
#[derive(Debug, Clone, Default)]
pub struct ClearWork {
    pub resources: BTreeMap<usize, usize>,
    pub cells: BTreeMap<i32, usize>,
    pub filter: Option<String>,
    pub clear_resources: bool,
    pub clear_cells: bool,
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
    pub stored: Vec<i32>,
    pub transit: Vec<i32>,
    pub workers: BTreeSet<usize>,
}

// +0x48; +0x4c; +0x50; +0x60
/// `BuildStructure` state (destroy flag, point cursor, occupied build points, workers).
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
