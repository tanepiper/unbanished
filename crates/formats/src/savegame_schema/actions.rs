//! The schemas of the character component and the actions a character runs.

use super::*;

pub(super) static SCHEMAS: &[(&str, &[Op])] = &[
    // `FUN_14010ef80`.
    (
        "CharacterComponent",
        &[
            BASE,
            Block(
                "character",
                0,
                &[
                    F32s("direction", 3),
                    F32("target_heading"),
                    F32("speed"),
                    Bool("flag_28"),
                    I32("stack_top"),
                    List("stack", &[Ptr("action")]),
                    Ptr("on_empty"),
                    Ptr("model"),
                    Ptr("picking"),
                ],
            ),
        ],
    ),
    // `FUN_1400ce390`.
    (
        "MoveTo",
        &[
            ACTION,
            Block(
                "move",
                1,
                &[
                    Ptr("owner"),
                    Ext("animations"),
                    I32("path_len"),
                    I32("path_index"),
                    I32sBy("path", "path_len"),
                    F32s("target", 3),
                    U32("flags"),
                    I32("walk_clip"),
                    I32("stand_clip"),
                    If(VersionBelow(1), &[Ptr("population")]),
                    If(VersionAtLeast(1), &[I32("cost_mode")]),
                    Bool("following"),
                    F32("speed_factor"),
                ],
            ),
        ],
    ),
    // `FUN_1400cea20`.
    (
        "PerformWork",
        &[
            ACTION,
            Block(
                "perform",
                0,
                &[
                    Ptr("owner"),
                    F32("facing"),
                    I32("clip"),
                    Time("duration"),
                    Time("elapsed"),
                    Bool("use_tool"),
                    I32("value_44"),
                ],
            ),
        ],
    ),
    // `FUN_140042de0`.
    (
        "PlayAnimation",
        &[
            ACTION,
            Block(
                "play",
                0,
                &[Ptr("owner"), I32("clip"), Time("duration"), Time("elapsed")],
            ),
        ],
    ),
    // `FUN_1400d04d0`.
    (
        "SendComponentMessage",
        &[
            ACTION,
            Block(
                "send",
                0,
                &[
                    Ptr("entity"),
                    Ptr("argument"),
                    U32("component_type"),
                    I32("message"),
                    U32("value_38"),
                    U32("value_3c"),
                ],
            ),
        ],
    ),
    // `FUN_1400430e0`.
    (
        "ShowTool",
        &[ACTION, Block("show", 0, &[Ptr("owner"), I32("tool")])],
    ),
    // `FUN_1400cb0b0`.
    // `FUN_1400cad50`: the road cell, its kind and a value.
    (
        "CompleteRoad",
        &[
            ACTION,
            Block("road", 0, &[I32("cell"), I8("built"), I32("ty")]),
        ],
    ),
    (
        "CompleteTask",
        &[
            ACTION,
            Block(
                "complete",
                1,
                &[
                    Ptr("citizen"),
                    Ptr("work_entity"),
                    I32("param"),
                    If(VersionAtLeast(1), &[I32("work")]),
                ],
            ),
        ],
    ),
    // `FUN_1400cb5f0`.
    (
        "ConsumeRawMaterial",
        &[
            ACTION,
            Block(
                "consume",
                2,
                &[
                    Ptr("entity"),
                    If(VersionBelow(1), &[Ext("material"), I32("count")]),
                    If(
                        VersionAtLeast(1),
                        &[List("items", &[Ext("material"), I32("count")])],
                    ),
                    If(VersionAtLeast(2), &[Bool("flag_38")]),
                ],
            ),
        ],
    ),
];
