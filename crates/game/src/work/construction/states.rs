//! The controllers' state tables (`StateDef`): for each controller, its states in order, the
//! callback each one runs, and where it goes next.

mod production;
use production::{
    ABSORB_RESOURCE, CEMETERY, CLEAR_RESOURCE, CROP_FIELD, HAPPINESS, MARKET_LOCATION, ORCHARD,
    PASTURE, RESOURCE_PRODUCE, STORAGE_LOCATION,
};

use super::*;

/// One state of a building's state machine: its callbacks and its transitions.
pub struct StateDef {
    /// The state's number.
    pub id: i8,
    /// The callback when the state is entered.
    pub enter: Callback,
    /// The callback at each scheduled update.
    pub update: Callback,
    /// The callback when the state is left.
    pub exit: Callback,
    /// The callbacks of transitions: the target state and its callback.
    pub transitions: &'static [(i8, Callback)],
}

pub(super) const CLEAR: &[StateDef] = &[
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

pub(super) const WORKPLACE: &[StateDef] = &[
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

pub(super) const BUILD: &[StateDef] = &[
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

pub(super) const RESIDENCE: &[StateDef] = &[
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
pub(super) const DESTROY: &[StateDef] = &[
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

pub(super) const WELL: &[StateDef] = &[
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
pub(super) const IMMIGRATION: &[StateDef] = &[
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
pub(super) const MULTI_OBJECT: &[StateDef] = &[StateDef {
    id: 2,
    enter: C::MultiObjectEnter2,
    update: C::None,
    exit: C::None,
    transitions: &[(3, C::None)],
}];

pub(super) const TRADE: &[StateDef] = &[
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

pub(super) const ATTENDANCE: &[StateDef] = &[
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

pub(super) const HEALER: &[StateDef] = &[
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
