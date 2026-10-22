//! The schemas of tutorials, small building components and the raw-material actions.

use super::*;

pub(super) static SCHEMAS: &[(&str, &[Op])] = &[
    // `FUN_1400e69f0`: the tutorial and its step.
    (
        "TutorialInstance",
        &[Block("tutorial", 0, &[Ext("tutorial"), I32("step")])],
    ),
    ("WellComponent", &[BASE, CONTROLLER, Block("well", 0, &[])]),
    // `FUN_14004b810`.
    (
        "HealerComponent",
        &[BASE, CONTROLLER, Block("healer", 0, &[])],
    ),
    // `FUN_14011dab0`.
    (
        "ClearResourceComponent",
        &[BASE, CONTROLLER, Block("clear", 0, &[])],
    ),
    // `FUN_140044d70`: the students, patients or worshippers.
    (
        "AttendanceComponent",
        &[
            BASE,
            CONTROLLER,
            Block("attendance", 0, &[List("visitors", &[Ptr("citizen")])]),
        ],
    ),
    // `FUN_140045330`: on (`+0x28`), its pile cells.
    (
        "AutoPickupComponent",
        &[
            BASE,
            Block(
                "pickup",
                0,
                &[Bool("on"), I32("cell_count"), I32sBy("cells", "cell_count")],
            ),
        ],
    ),
    // `FUN_140124a30`.
    (
        "FloorComponent",
        &[BASE, Block("floor", 0, &[Bool("active")])],
    ),
    // `FUN_14004a9d0`.
    (
        "FruitComponent",
        &[
            BASE,
            Block(
                "fruit",
                0,
                &[Ptr("orchard"), F32("value_30"), Bool("flag_34")],
            ),
        ],
    ),
    // `FUN_14004fbc0`.
    ("MineComponent", &[BASE, Block("mine", 0, &[])]),
    // `FUN_14012ad50`: the parts and the stages (`+0x58`).
    (
        "MultiObjectComponent",
        &[
            BASE,
            CONTROLLER,
            Block("multi", 0, &[List("parts", &[Ptr("part")]), I32("stages")]),
        ],
    ),
    // `FUN_140055b90`.
    ("QuarryComponent", &[BASE, Block("quarry", 0, &[])]),
    // `FUN_14012e800` (version 2).
    (
        "RawMaterialComponent",
        &[
            BASE,
            Block(
                "raw",
                2,
                &[
                    I32("count"),
                    Time("age"),
                    If(VersionAtLeast(2), &[Ext("profession")]),
                ],
            ),
        ],
    ),
    // `FUN_14011e2d0`: the selector (`+0x50`), the recipes' weights, productions (`+0x68`).
    (
        "ConsumeProduceComponent",
        &[
            RESOURCE_PRODUCE,
            Block(
                "consume_produce",
                1,
                &[
                    I32("selector"),
                    I32("weight_count"),
                    F32sBy("weights", "weight_count"),
                    I32("produced"),
                ],
            ),
        ],
    ),
    // `FUN_1400cab30`.
    (
        "AnimateNaturalResource",
        &[
            ACTION,
            Block("animate", 0, &[Ptr("entity"), I32("clip"), Bool("flag_2c")]),
        ],
    ),
    // `FUN_1400cbd60`.
    (
        "CreateNaturalResource",
        &[
            ACTION,
            Block(
                "create_natural",
                1,
                &[
                    Ext("template"),
                    I32("cell"),
                    If(VersionAtLeast(1), &[Ptr("occupant")]),
                ],
            ),
        ],
    ),
    // `FUN_1400cc400` (version 3).
    (
        "CreateRawMaterial",
        &[
            ACTION,
            Block(
                "create_raw",
                3,
                &[
                    Ext("template"),
                    Ptr("citizen"),
                    I32("cell"),
                    F32("factor"),
                    Bool("flag_50"),
                    I32("batch"),
                    Bool("flag_51"),
                    If(VersionAtLeast(1), &[Ptr("workplace")]),
                    If(VersionAtLeast(2), &[U32("value_48")]),
                    If(VersionAtLeast(3), &[Ptr("value_38")]),
                ],
            ),
        ],
    ),
    // `FUN_1400ccfe0`.
    (
        "DropOffRawMaterial",
        &[
            ACTION,
            Block(
                "drop_off",
                0,
                &[
                    Ptr("citizen"),
                    Ptr("storage"),
                    Ext("material"),
                    Bool("redirect"),
                    I32("count"),
                ],
            ),
        ],
    ),
    // `FUN_1400ceeb0`.
    (
        "PickupRawMaterial",
        &[ACTION, Block("pickup", 0, &[Ptr("citizen"), Ptr("entity")])],
    ),
    // `FUN_1400cf5a0`.
    (
        "PickupStoredRawMaterial",
        &[
            ACTION,
            Block(
                "pickup_stored",
                0,
                &[
                    Ptr("citizen"),
                    Ptr("storage"),
                    List("items", &[Ext("material"), I32("count")]),
                    Bool("flag_40"),
                ],
            ),
        ],
    ),
    // `FUN_1400d0120`.
    (
        "RemoveNaturalResource",
        &[
            ACTION,
            Block(
                "remove",
                0,
                &[Ptr("citizen"), Ptr("entity"), Bool("flag_30")],
            ),
        ],
    ),
];
