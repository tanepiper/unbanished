//! The schemas of the work kinds (`Work` subclasses).

use super::*;

pub(super) static SCHEMAS: &[(&str, &[Op])] = &[
    // Work (`Work` subclasses; serializers named by their class block).
    // `FUN_1400f0e40`.
    (
        "ForesterWork",
        &[
            AUTO_PICKUP_WORK,
            Block("forester", 0, &[CITIZENS, I32("value_58")]),
        ],
    ),
    // `FUN_1400f4080`.
    (
        "HerbalistWork",
        &[
            AUTO_PICKUP_WORK,
            Block("herbalist", 0, &[CITIZENS, I32("value_58")]),
        ],
    ),
    // `FUN_1400f4ad0`.
    (
        "HuntWork",
        &[AUTO_PICKUP_WORK, Block("hunt", 0, &[CITIZENS])],
    ),
    // `FUN_1400f8130`.
    (
        "PastureWork",
        &[AUTO_PICKUP_WORK, Block("pasture", 0, &[CITIZENS])],
    ),
    // `FUN_1400e9e50`.
    (
        "BuildRoad",
        &[
            WORK,
            Block(
                "road",
                1,
                &[
                    U32("type"),
                    I32("cell"),
                    Ptr("builder"),
                    Ext("material"),
                    If(
                        VersionAtLeast(1),
                        &[I32("old_kind"), I32("old_built"), I32("old_type")],
                    ),
                ],
            ),
        ],
    ),
    // `FUN_1400ead20`.
    (
        "BuildStructure",
        &[
            WORK,
            Block(
                "build",
                0,
                &[
                    Bool("destroy"),
                    CITIZENS,
                    List("occupied", &[Bool("done")]),
                    I32("cursor"),
                ],
            ),
        ],
    ),
    // `FUN_1400ebc10`; `FUN_14002a570` pairs entities with citizens.
    (
        "ClearArea",
        &[
            WORK,
            Block(
                "clear",
                1,
                &[
                    List("assigned", &[Ptr("entity"), Ptr("citizen")]),
                    Ext("filter"),
                    Bool("clear_resources"),
                    If(
                        VersionAtLeast(1),
                        &[KEYED_CITIZENS, Bool("clear_cells"), Bool("clear_piles")],
                    ),
                ],
            ),
        ],
    ),
    // `FUN_1400ec820`.
    (
        "ClearStorage",
        &[WORK, Block("clear_storage", 0, &[CITIZENS])],
    ),
    // `FUN_1400ee4f0`.
    (
        "ConsumeProduce",
        &[
            WORK,
            Block(
                "consume",
                0,
                &[
                    CITIZENS,
                    List("points", &[I32("state")]),
                    Bool("full"),
                    I32("cycles"),
                    U32("type_50"),
                    I32("value_54"),
                ],
            ),
        ],
    ),
    // `FUN_1400eef20`.
    (
        "FireWork",
        &[WORK, Block("fire", 0, &[CITIZENS, I32("value_48")])],
    ),
    // `FUN_1400efde0`.
    (
        "FishingWork",
        &[
            WORK,
            Block(
                "fishing",
                0,
                &[CITIZENS, List("values", &[I32("value")]), I32("value_68")],
            ),
        ],
    ),
    // `FUN_1400f1a30`.
    (
        "GatherRawMaterials",
        &[
            WORK,
            Block(
                "gather",
                0,
                &[
                    List("stored", &[I32("value")]),
                    List("transit", &[I32("value")]),
                    CITIZENS,
                ],
            ),
        ],
    ),
    // `FUN_1400f5070`.
    (
        "LocationWork",
        &[WORK, Block("location", 0, &[Ptr("citizen")])],
    ),
    // `FUN_1400f7240`; from version 1 `FUN_14002bbf0`: citizen → (description → count).
    (
        "MarketWork",
        &[
            WORK,
            Block(
                "market",
                1,
                &[
                    If(VersionBelow(1), &[CITIZENS]),
                    If(
                        VersionAtLeast(1),
                        &[List(
                            "orders",
                            &[
                                Ptr("citizen"),
                                List("items", &[Ext("description"), I32("count")]),
                            ],
                        )],
                    ),
                ],
            ),
        ],
    ),
    // `FUN_1400f8f70`.
    (
        "Pickup",
        &[
            WORK,
            Block(
                "pickup",
                2,
                &[
                    I32("state"),
                    Ptr("claimer"),
                    If(VersionAtLeast(1), &[Ext("profession")]),
                ],
            ),
        ],
    ),
    // `FUN_1400f9a10`.
    (
        "PlantArea",
        &[
            WORK,
            Block(
                "plant",
                0,
                &[
                    KEYED_CITIZENS,
                    List("cells", &[Bool("done")]),
                    Ext("description"),
                ],
            ),
        ],
    ),
    // `FUN_1400fa420`.
    (
        "PlantOrchard",
        &[
            WORK,
            Block(
                "plant",
                0,
                &[
                    KEYED_CITIZENS,
                    List("cells", &[Bool("done")]),
                    Ext("description"),
                ],
            ),
        ],
    ),
    // `FUN_1400fadd0`.
    (
        "TendArea",
        &[WORK, Block("tend", 0, &[CITIZENS, I32("value_58")])],
    ),
    // `FUN_1400fc180`; `FUN_14002d200`: citizen → count and description.
    (
        "TradeWork",
        &[
            WORK,
            Block(
                "trade",
                0,
                &[List(
                    "orders",
                    &[Ptr("citizen"), I32("count"), Ext("description")],
                )],
            ),
        ],
    ),
    // `FUN_14004f900`.
];
