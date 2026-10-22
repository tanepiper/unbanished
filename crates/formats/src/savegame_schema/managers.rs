//! The schemas of the town's managers: achievements, town, jobs, tools, calendar, event log, environment, minimap.

use super::*;

// +0x88
/// The game's achievement instances.
///
/// Original: `Achievement::AchievementManager::Persist`, Runtime `0x18009f4a0`.
pub const ACHIEVEMENTS: Op = Block("achievements", 0, &[List("instances", &[Ptr("instance")])]);

// Runtime FUN_18005d130: per 0x30-byte entry the group +0x00, the clip +0x08, the users +0x0c,
// the instances (+0x20, FUN_18005cd90, 0x48 bytes each) and the nodes (+0x10, FUN_18005cbf0).
// An instance: its AnimationState (FUN_18005d610), a flag, the transform pairs (FUN_18005cd00),
// a vector, then the pose (FUN_18005a2f0).
/// `Graphics::AnimationManager`: the shared instances of instanced clips. Each entry is one
/// group's clip with its number of users; each of its instances is an animation state of its own
/// with its pose, which the characters playing that clip follow.
///
/// Original: `AnimationManager::Serialize`, Runtime `0x1800fea20`.
pub const ANIMATIONS: Op = Block(
    "animations",
    0,
    &[List(
        "entries",
        &[
            Ext("group"),
            I32("clip"),
            I32("users"),
            List(
                "instances",
                &[
                    List("states", super::ANIMATION_STATE),
                    Bool("flag"),
                    List(
                        "pairs",
                        &[
                            F32s("rotation", 4),
                            F32s("translation", 3),
                            F32s("rotation_2", 4),
                            F32s("translation_2", 3),
                        ],
                    ),
                    F32s("vector", 3),
                    List("pose", &[F32s("rotation", 4), F32s("translation", 3)]),
                ],
            ),
            List(
                "nodes",
                &[
                    Str("name"),
                    I32("parent"),
                    F32s("rotation", 4),
                    F32s("translation", 3),
                    F32s("rotation_2", 4),
                    F32s("translation_2", 3),
                ],
            ),
        ],
    )],
);

// Game +0xf0
/// The town manager (version 6), `Game`: the registries of buildings and people by kind, the
/// professions' workers, the stock targets and the statistics.
///
/// Original: `FUN_140094c30`.
pub const TOWN: Op = Block(
    "town",
    6,
    &[
        ptr_list("storages"),
        ptr_list("markets"),
        ptr_list("trading_posts"),
        ptr_list("residences"),
        ptr_list("healers"),
        ptr_list("citizens"),
        // `FUN_140018450`: profession → workplaces.
        List("workplaces", &[Ext("profession"), ptr_list("entities")]),
        ptr_list("pastures"),
        ptr_list("cemeteries"),
        ptr_list("happiness"),
        ptr_list("attendance"),
        ptr_list("livestock"),
        ptr_list("herds"),
        If(VersionAtLeast(1), &[ptr_list("diseases")]),
        ptr_list("populations"),
        ptr_list("sites"),
        I32("known_material_count"),
        I32sBy("known_materials", "known_material_count"),
        I32("known_count"),
        I32sBy("known", "known_count"),
        Time("commute_time"),
        Time("time_198"),
        List("history", &[I32("count"), I32sBy("values", "count")]),
        If(
            VersionBelow(2),
            &[List("legacy_counts", &[Ext("description"), I32("count")])],
        ),
        // `FUN_1400186e0`: description → entities.
        If(
            VersionAtLeast(2),
            &[List("zones", &[Ext("description"), ptr_list("entities")])],
        ),
        ptr_list("wells"),
        List("water_sources", &[I32("a"), I32("b")]),
        If(
            VersionAtLeast(5),
            &[List(
                "values_140",
                &[I32("count"), I32sBy("values", "count")],
            )],
        ),
        If(
            VersionBelow(4),
            &[
                U32s("legacy_a", 13),
                U32s("legacy_b", 13),
                U32s("legacy_c", 13),
                Times("legacy_times", 13),
                U32s("legacy_d", 9),
                U32s("legacy_e", 9),
            ],
        ),
        If(
            VersionAtLeast(4),
            &[
                List(
                    "stock",
                    &[
                        I32("count"),
                        I32("previous"),
                        I32("limit"),
                        I32("used"),
                        I32("made"),
                        Time("cooldown"),
                    ],
                ),
                List(
                    "capacity_records",
                    &[I32("used"), I32("capacity"), Time("cooldown")],
                ),
            ],
        ),
        F32("visitor_risk"),
        Time("visitor_time"),
        If(VersionAtLeast(2), &[I32("trade_value")]),
        If(VersionAtLeast(3), &[I32("nomads_admitted")]),
    ],
);

// +0x30; +0x40..; +0x120; +0x00..+0x08; +0x10; Game +0x118
/// The job manager (version 3): the table from id to work, the 14 category sets (13 before version
/// 3), the work by profession, then the last id, the next priority and the pass. The priority
/// index is rebuilt on load from each work's priority.
///
/// Original: `FUN_1400fe710`, `FUN_14002df50`; the category sets by `FUN_14002d9c0`.
pub const JOBS: Op = Block(
    "jobs",
    3,
    &[
        List("works", &[I32("key"), Ptr("work")]),
        List("category_1", &[I32("key"), Ptr("work")]),
        List("category_2", &[I32("key"), Ptr("work")]),
        List("category_3", &[I32("key"), Ptr("work")]),
        List("category_4", &[I32("key"), Ptr("work")]),
        List("category_5", &[I32("key"), Ptr("work")]),
        List("category_6", &[I32("key"), Ptr("work")]),
        List("category_7", &[I32("key"), Ptr("work")]),
        List("category_8", &[I32("key"), Ptr("work")]),
        List("category_9", &[I32("key"), Ptr("work")]),
        List("category_10", &[I32("key"), Ptr("work")]),
        List("category_11", &[I32("key"), Ptr("work")]),
        List("category_12", &[I32("key"), Ptr("work")]),
        List("category_13", &[I32("key"), Ptr("work")]),
        If(
            VersionAtLeast(3),
            &[List("category_14", &[I32("key"), Ptr("work")])],
        ),
        List(
            "by_profession",
            &[Ext("profession"), List("work", &[I32("key"), Ptr("work")])],
        ),
        I32("next_id"),
        I32("next_priority"),
        U32("pass"),
    ],
);

/// The tool manager (version 2).
///
/// Original: `FUN_140107df0`.
pub const TOOLS: Op = Block(
    "tools",
    2,
    &[
        Bool("flag_30"),
        I32("value_34"),
        F32("value_3c"),
        F32("value_40"),
        F32("value_44"),
        // `FUN_14002ec40`.
        List(
            "descriptions",
            &[
                Ext("description"),
                I32("value_8"),
                I32("value_c"),
                Bool("flag_10"),
            ],
        ),
        // `FUN_14002edd0`.
        If(
            VersionAtLeast(1),
            &[List(
                "entities",
                &[
                    Ptr("entity"),
                    I32("value_8"),
                    I32("value_c"),
                    Bool("flag_10"),
                ],
            )],
        ),
        If(
            VersionAtLeast(2),
            &[List(
                "points",
                &[
                    F32s("position", 3),
                    F32("value_c"),
                    F32("value_10"),
                    F32("value_14"),
                    Bool("flag_18"),
                ],
            )],
        ),
    ],
);

/// The calendar block of the game state: a value and the time.
///
/// Original: written by `FUN_14009af60`.
pub const CALENDAR: Op = Block("calendar", 0, &[I32("value"), Time("time")]);

// Game +0x138
/// The event log (version 1), `Game`.
///
/// Original: `FUN_14009a790`.
pub const EVENT_LOG: Op = Block(
    "event_log",
    1,
    &[
        Bool("notify_0"),
        Bool("notify_1"),
        Bool("notify_2"),
        Bool("notify_3"),
        Bool("notify_4"),
        Bool("notify_5"),
        List(
            "entries",
            &[
                U32("color"),
                Str("text"),
                I32("kind"),
                I32("severity"),
                I32("cell"),
                I32("category"),
                I32("icon"),
            ],
        ),
    ],
);

/// The environment: climate and month descriptions, weather state, then one entry per weather
/// effect of the climate (the count is not written).
///
/// Original: `FUN_140099160`.
pub const ENVIRONMENT: Op = Block(
    "environment",
    0,
    &[
        Ext("climate"),
        Ext("month"),
        I32("state"),
        I32("level"),
        F32("temperature"),
        F32s("season", 4),
        F32("baseline"),
        F32("baseline_target"),
        F32("bias"),
        F32("bias_target"),
        I32("slice"),
        Rest(
            "effects",
            &[
                Bool("active"),
                If(Flag("active"), &[F32("direction"), F32("intensity")]),
            ],
        ),
    ],
);

/// The minimap: the two images' texels, then the image size.
///
/// Original: `FUN_1400aacc0`.
pub const MINIMAP: Op = Block(
    "minimap",
    0,
    &[
        I32("count_28"),
        I32sBy("image_28", "count_28"),
        I32("count_18"),
        I32sBy("image_18", "count_18"),
        I32("size"),
    ],
);
