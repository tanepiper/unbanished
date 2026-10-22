//! The schemas of citizens and of the components of each kind of building.

use super::*;

pub(super) static SCHEMAS: &[(&str, &[Op])] = &[
    // `FUN_140119630` (version 3).
    (
        "CitizenComponent",
        &[
            BASE,
            Block(
                "citizen",
                3,
                &[
                    Str("name"),
                    I32("sex"),
                    F32("age"),
                    F32("lifespan"),
                    F32("hunger"),
                    F32("cold"),
                    F32("reproduction"),
                    F32("health"),
                    U32("health_events"),
                    Time("health_timer"),
                    Time("disease_clock"),
                    Ext("disease"),
                    Time("disease_left"),
                    Time("spread"),
                    F32("happiness"),
                    U32("flags"),
                    Time("happiness_timer"),
                    I32("food_count"),
                    I32sBy("foods", "food_count"),
                    Ext("profession"),
                    I32("tool_uses"),
                    I32("value_1d8"),
                    If(VersionAtLeast(1), &[I32("tool_status")]),
                    I32("clothing"),
                    Time("clothing_remaining"),
                    If(VersionAtLeast(2), &[I32("clothing_status")]),
                    U32("supply_activity"),
                    I32("job"),
                    Ptr("home"),
                    I32("home_slot"),
                    Ptr("spouse"),
                    Ptr("guardian"),
                    Ptr("ring_head"),
                    Ptr("ring_next"),
                    Ptr("ring_prev"),
                    Ptr("workplace"),
                    Ext("accident"),
                    Ptr("school"),
                    I32("education"),
                    If(VersionAtLeast(3), &[Time("school_time")]),
                    Bool("schooled"),
                    Ptr("doctor"),
                    Ptr("priest"),
                    Ptr("character"),
                    Ptr("status_icon"),
                    Ext("citizen_description"),
                    I32("appearance"),
                ],
            ),
        ],
    ),
    // `FUN_140114e30` (version 5).
    (
        "CitizenAIComponent",
        &[
            BASE,
            Block(
                "ai",
                5,
                &[
                    Ptr("citizen"),
                    I32("queued_activity"),
                    If(VersionBelow(5), &[Times("cooldowns", 14)]),
                    If(VersionAtLeast(5), &[Times("cooldowns", 16)]),
                    If(
                        VersionAtLeast(1),
                        &[Bool("urgent_eating"), Bool("urgent_warming")],
                    ),
                    If(VersionAtLeast(2), &[I32("current_activity")]),
                    If(VersionAtLeast(3), &[Time("since_loiter")]),
                    If(VersionAtLeast(4), &[Time("carrying_time")]),
                ],
            ),
        ],
    ),
    // `FUN_14012f640`: per family slot, its citizens; the fuel time (`+0x48`).
    (
        "ResidenceComponent",
        &[
            BASE,
            CONTROLLER,
            Block(
                "residence",
                0,
                &[
                    List("families", &[List("members", &[Ptr("citizen")])]),
                    Time("fuel"),
                ],
            ),
        ],
    ),
    // `FUN_1401292e0`.
    (
        "LivestockComponent",
        &[
            BASE,
            Block("livestock", 0, &[Ptr("pasture"), Time("timer")]),
        ],
    ),
    // `FUN_14012d410`: the kind (`+0x28`), the animals, `+0x40`, `+0x44`.
    (
        "PastureComponent",
        &[
            BASE,
            CONTROLLER,
            Block(
                "pasture",
                0,
                &[
                    Ext("kind"),
                    List("animals", &[Ptr("animal")]),
                    I32("value_40"),
                    I32("value_44"),
                ],
            ),
        ],
    ),
    // `FUN_140059a90`: this year's and last year's (material, amount), the year.
    (
        "TrackerComponent",
        &[
            BASE,
            Block(
                "tracker",
                0,
                &[
                    List("this_year", &[Ext("material"), I32("amount")]),
                    List("last_year", &[Ext("material"), I32("amount")]),
                    I32("year"),
                ],
            ),
        ],
    ),
    // `FUN_140049600`.
    (
        "DiseaseComponent",
        &[
            BASE,
            Block(
                "disease",
                1,
                &[
                    Ext("disease"),
                    I32("value_48"),
                    Time("time_28"),
                    Time("time_30"),
                    Time("time_38"),
                    Time("time_40"),
                ],
            ),
        ],
    ),
    // `FUN_14004b3e0`: registered (`+0x28`).
    (
        "HappinessComponent",
        &[
            BASE,
            CONTROLLER,
            Block("happiness", 0, &[Bool("registered")]),
        ],
    ),
    // `FUN_14005d900`.
    // RESOURCE_PRODUCE, then the time since the last hunt.
    (
        "HuntComponent",
        &[RESOURCE_PRODUCE, Block("hunt", 0, &[Time("time_50")])],
    ),
    // The graves in use.
    (
        "CemeteryComponent",
        &[
            BASE,
            CONTROLLER,
            Block("cemetery", 0, &[I32("count"), I32sBy("graves", "count")]),
        ],
    ),
    (
        "BoatComponent",
        &[
            BASE,
            CONTROLLER,
            Block(
                "boat",
                0,
                &[
                    Ptr("post"),
                    F32s("dock", 3),
                    F32("dock_yaw"),
                    Ptr("model"),
                    Ptr("character"),
                    F32s("target", 3),
                    I32("index"),
                    List("route", &[F32s("point", 3)]),
                ],
            ),
        ],
    ),
    // `FUN_140120340`; the cells by `FUN_140034020`.
    (
        "CropFieldComponent",
        &[
            BASE,
            CONTROLLER,
            Block(
                "field",
                0,
                &[
                    Bool("flag_48"),
                    Bool("flag_49"),
                    Ext("crop"),
                    List(
                        "cells",
                        &[
                            I32("x"),
                            I32("y"),
                            Ptr("entity"),
                            Ptr("resource"),
                            Bool("flag"),
                        ],
                    ),
                ],
            ),
        ],
    ),
    // `FUN_140052740`; the trees by `FUN_14000a230`.
    (
        "OrchardComponent",
        &[
            BASE,
            CONTROLLER,
            Block(
                "orchard",
                0,
                &[
                    Bool("flag_48"),
                    I32("value_4c"),
                    List(
                        "trees",
                        &[
                            I32("x"),
                            I32("y"),
                            Ptr("entity"),
                            Ptr("resource"),
                            Ptr("fruit"),
                            Bool("flag"),
                        ],
                    ),
                    Ext("tree"),
                ],
            ),
        ],
    ),
    // `FUN_14005c6b0`.
    (
        "TradeComponent",
        &[
            BASE,
            CONTROLLER,
            Block(
                "trade",
                0,
                &[
                    List("stock", &[Ext("material"), I32("value_4"), I32("value_8")]),
                    I32("value_2c"),
                    // `FUN_14000ebe0`: a map of description to count.
                    List("requests", &[Ext("material"), I32("count")]),
                    List(
                        "merchants",
                        &[
                            Str("name"),
                            I32("count"),
                            I32sBy("materials", "count"),
                            I32("value_18"),
                        ],
                    ),
                    List("orders", &[U32("value_0"), I32("value_8"), I32("value_4")]),
                    I32("value_28"),
                    Time("time_78"),
                    Bool("flag_71"),
                    Ptr("merchant"),
                    Bool("flag_70"),
                ],
            ),
        ],
    ),
    // `FUN_140124520`.
    (
        "FishingComponent",
        &[
            RESOURCE_PRODUCE,
            Block("fishing", 0, &[Bool("flag_50"), F32("value_54")]),
        ],
    ),
    // `FUN_140124e30`: whether to plant and whether to cut.
    (
        "ForesterComponent",
        &[
            RESOURCE_PRODUCE,
            Block("forester", 0, &[Bool("plant"), Bool("cut")]),
        ],
    ),
    // `FUN_14004bb60`.
    (
        "HerbalistComponent",
        &[RESOURCE_PRODUCE, Block("herbalist", 0, &[])],
    ),
];
