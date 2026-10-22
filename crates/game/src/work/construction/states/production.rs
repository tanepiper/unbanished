//! The state tables of the producing and service controllers: fields, orchards, markets, storage, pastures and resources.

use super::*;

pub(super) const CROP_FIELD: &[StateDef] = &[
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

pub(super) const ORCHARD: &[StateDef] = &[
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

pub(super) const MARKET_LOCATION: &[StateDef] = &[
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

pub(super) const CEMETERY: &[StateDef] = &[
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

pub(super) const ABSORB_RESOURCE: &[StateDef] = &[StateDef {
    id: 0,
    enter: C::None,
    update: C::AbsorbUpdate0,
    exit: C::None,
    transitions: &[],
}];

pub(super) const STORAGE_LOCATION: &[StateDef] = &[
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

pub(super) const PASTURE: &[StateDef] = &[
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

pub(super) const HAPPINESS: &[StateDef] = &[
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

pub(super) const CLEAR_RESOURCE: &[StateDef] = &[StateDef {
    id: 2,
    enter: C::ClearResourceEnter2,
    update: C::None,
    exit: C::None,
    transitions: &[(3, C::None)],
}];

pub(super) const RESOURCE_PRODUCE: &[StateDef] = &[
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
