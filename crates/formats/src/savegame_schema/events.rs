//! The schemas of markets, immigration, harvest work, nomads and disasters.

use super::*;

pub(super) static SCHEMAS: &[(&str, &[Op])] = &[
    (
        "MarketLocationComponent",
        &[BASE, CONTROLLER, Block("market_location", 0, &[])],
    ),
    // `FUN_14004f430`.
    (
        "MarketComponent",
        &[RESOURCE_PRODUCE, Block("market", 0, &[])],
    ),
    // `FUN_14004ee10`: the town hall's nomads.
    (
        "ImmigrationComponent",
        &[
            BASE,
            CONTROLLER,
            Block(
                "immigration",
                0,
                &[
                    I32("value_50"),
                    List("nomads", &[Ptr("entity")]),
                    Time("time_48"),
                ],
            ),
        ],
    ),
    // `FUN_1400f2820`: crop harvest.
    (
        "HarvestArea",
        &[
            AUTO_PICKUP_WORK,
            Block(
                "harvest",
                0,
                &[KEYED_CITIZENS, List("cells", &[Bool("done")])],
            ),
        ],
    ),
    // `FUN_1400f3370`: fruit picking.
    (
        "HarvestOrchard",
        &[
            AUTO_PICKUP_WORK,
            Block(
                "harvest",
                0,
                &[KEYED_CITIZENS, List("cells", &[Bool("done")])],
            ),
        ],
    ),
    // `FUN_1400cd730`: the hunting lodge, the hunter, done (`+0x38`).
    (
        "HuntAnimal",
        &[
            ACTION,
            Block("hunt", 0, &[Ptr("lodge"), Ptr("hunter"), Bool("done")]),
        ],
    ),
    // `FUN_1400cfd40`.
    (
        "RemoveAnimal",
        &[ACTION, Block("remove", 0, &[Ptr("entity")])],
    ),
    // `FUN_1400505a0`: the town hall it walks to, its way, `+0x50`, its clock.
    (
        "NomadComponent",
        &[
            BASE,
            CONTROLLER,
            Block(
                "nomad",
                0,
                &[
                    Ptr("town_hall"),
                    F32s("from", 3),
                    F32s("to", 3),
                    I32("value_50"),
                    Time("time_48"),
                ],
            ),
        ],
    ),
    // `FUN_140057e60`: the heights kept, as points.
    (
        "TerraformComponent",
        &[
            BASE,
            Block("terraform", 0, &[List("points", &[F32s("point", 3)])]),
        ],
    ),
    // `FUN_140058240`: the time left (halved when read from version 1).
    (
        "TombstoneComponent",
        &[BASE, Block("tombstone", 2, &[Time("left")])],
    ),
    // `FUN_1400594e0`.
    (
        "TornadoComponent",
        &[
            BASE,
            Block(
                "tornado",
                0,
                &[
                    F32s("position", 3),
                    I32("value_50"),
                    List("path", &[F32s("point", 3)]),
                    Time("time_28"),
                    F32s("value_58", 3),
                ],
            ),
        ],
    ),
];
