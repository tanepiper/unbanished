//! The schemas of entities and their general components: map, model, nature, animals, storage, building.

use super::*;

pub(super) static SCHEMAS: &[(&str, &[Op])] = &[
    // `Achievement::AchievementInstance::Persist` (Runtime `0x18009efa0`, version 1): its
    // achievement, then its goal list's instances.
    (
        "AchievementInstance",
        &[Block(
            "instance",
            1,
            &[
                Ext("achievement"),
                Block("goals", 0, &[List("goals", &[Ptr("goal")])]),
            ],
        )],
    ),
    // `Achievement::GoalInstance::Persist` (`0x18009fe80`): its goal and its count (`+0x10`).
    ("GoalInstance", &[GOAL_INSTANCE]),
    // `FUN_1400e0200`: then its time (`+0x20`, `System::Time::Serialize`: a block).
    (
        "TimeGoalInstance",
        &[GOAL_INSTANCE, Block("timer", 0, &[Time("time")])],
    ),
    // `FUN_1400dd0c0`: then its tutorial (`+0x20`).
    ("FinishTutorialInstance", &[GOAL_INSTANCE, Ptr("tutorial")]),
    // `FUN_1400c8610`.
    (
        "ComponentEntity",
        &[Block(
            "entity",
            0,
            &[
                Ext("template"),
                List("components", &[Ptr("component")]),
                U32("flags"),
            ],
        )],
    ),
    // `FUN_1401298b0`.
    (
        "MapComponent",
        &[
            BASE,
            Block(
                "map",
                0,
                &[I16("x"), I16("y"), I16("w"), I16("h"), I32("rotation")],
            ),
        ],
    ),
    // `FUN_14012a040`.
    (
        "ModelComponent",
        &[
            BASE,
            Block(
                "model",
                0,
                &[
                    I16("variant"),
                    I16("mesh"),
                    F32s("rotation", 4),
                    F32s("position", 3),
                    F32("yaw"),
                    MESH_INSTANCE,
                    Bool("picking"),
                ],
            ),
        ],
    ),
    // `FUN_14012b1c0`.
    (
        "NaturalResourceComponent",
        &[
            BASE,
            Block(
                "natural",
                0,
                &[F32("value_28"), Bool("flag_2c"), Bool("flag_2d")],
            ),
        ],
    ),
    // `FUN_140125c90`.
    (
        "GrowthComponent",
        &[
            BASE,
            Block(
                "growth",
                0,
                &[
                    Ptr("natural"),
                    Ptr("model"),
                    F32("growth"),
                    F32("maximum"),
                    Time("timer"),
                ],
            ),
        ],
    ),
    // `FUN_140056840`.
    (
        "SpawnComponent",
        &[BASE, Block("spawn", 0, &[List("timers", &[Time("timer")])])],
    ),
    // `FUN_140043890`.
    (
        "AmbientEmitterComponent",
        &[
            BASE,
            Block(
                "emitter",
                1,
                &[
                    F32s("position", 3),
                    List("slots", &[Ext("effect"), U32("handle"), F32("volume")]),
                ],
            ),
        ],
    ),
    // `FUN_140044140`.
    (
        "AmbientEnvironmentComponent",
        &[
            BASE,
            Block(
                "environment",
                0,
                &[
                    F32s("position", 3),
                    I32("value_28"),
                    I32("value_2c"),
                    I32("value_30"),
                    I32("value_34"),
                ],
            ),
        ],
    ),
    // `FUN_14010c310`.
    (
        "AnimalComponent",
        &[
            BASE,
            Block(
                "animal",
                1,
                &[
                    Time("timer"),
                    I32("home"),
                    F32("age"),
                    F32("life"),
                    Bool("male"),
                    Time("breed"),
                    I16("x"),
                    I16("y"),
                    I16("w"),
                    I16("h"),
                    If(VersionBelow(1), &[Ptr("population")]),
                ],
            ),
        ],
    ),
    // `FUN_140127460`.
    (
        "HerdComponent",
        &[
            BASE,
            Block(
                "herd",
                0,
                &[
                    List("animals", &[Ptr("animal")]),
                    Bool("moving"),
                    Time("rest"),
                    Time("regrow"),
                    I32("target"),
                    Ptr("population"),
                    I32("index"),
                ],
            ),
        ],
    ),
    // `FUN_140054e00`.
    (
        "PopulationComponent",
        &[
            BASE,
            Block(
                "population",
                0,
                &[
                    Ext("description"),
                    I32("width"),
                    I32("herd_width"),
                    List("cells", &[I32("a"), I32("c"), I32("b")]),
                    List("herds", &[Ptr("herd")]),
                    Time("clock"),
                    I32("cursor"),
                    I32("value_6c"),
                ],
            ),
        ],
    ),
    // `FUN_1401337d0`.
    (
        "WildAnimalComponent",
        &[BASE, Block("wild", 0, &[Ptr("herd")])],
    ),
    // `FUN_1401357c0`.
    ("ZoneComponent", &[BASE, Block("zone", 1, &[])]),
    // `FUN_140120cf0`.
    (
        "DecalComponent",
        &[
            BASE,
            Block(
                "decal",
                1,
                &[F32("alpha"), If(VersionAtLeast(1), &[I32("material")])],
            ),
        ],
    ),
    // `FUN_140133be0`.
    (
        "WorkComponent",
        &[
            BASE,
            Block(
                "work",
                3,
                &[
                    I32("work_id"),
                    If(VersionBelow(3), &[Bool("old")]),
                    If(VersionAtLeast(1), &[I32("most_workers")]),
                    If(VersionAtLeast(2), &[I32("kept_priority")]),
                ],
            ),
        ],
    ),
    // `FUN_140130e10`.
    (
        "StateMachineComponent",
        &[
            BASE,
            Block(
                "machine",
                0,
                &[I8("state"), I8("index"), I8("pending"), I8("transition")],
            ),
        ],
    ),
    // `FUN_14011d390`.
    (
        "ClearAreaComponent",
        &[BASE, CONTROLLER, Block("clear", 0, &[I32("total")])],
    ),
    // `FUN_140123020`.
    (
        "DestroyComponent",
        &[
            BASE,
            CONTROLLER,
            Block(
                "destroy",
                0,
                &[
                    I32("count"),
                    I32("target"),
                    Bool("refund"),
                    Ext("rebuild"),
                    Bool("damaged"),
                    Time("time"),
                ],
            ),
        ],
    ),
    // `FUN_14012e080`.
    (
        "PickingComponent",
        &[BASE, Block("picking", 0, &[Ptr("entity")])],
    ),
    // `FUN_1401314c0`: the status bits (`+0xf0`).
    (
        "StatusIconComponent",
        &[BASE, Block("status", 0, &[U32("bits")])],
    ),
    // `FUN_14004d1e0`.
    ("HighlightComponent", &[BASE, Block("highlight", 0, &[])]),
    // `FUN_140131c00`: items (count, reserved, raw material), `+0x38`, capacity `+0x3c`.
    (
        "StorageComponent",
        &[
            BASE,
            Block(
                "storage",
                1,
                &[
                    List("items", &[I32("count"), I32("reserved"), Ext("material")]),
                    I32("capacity"),
                    U32("accepts"),
                ],
            ),
        ],
    ),
    // `FUN_140132770`.
    (
        "StorageLocationComponent",
        &[BASE, CONTROLLER, Block("location", 0, &[])],
    ),
    // `FUN_140128810`: the MultiObject parts' InteractComponents.
    (
        "InteractComponent",
        &[
            BASE,
            Block("interact", 0, &[List("children", &[Ptr("child")])]),
        ],
    ),
    // `FUN_14004a3f0`.
    (
        "FireComponent",
        &[
            BASE,
            Block(
                "fire",
                1,
                &[
                    Time("burn"),
                    Time("spread"),
                    I32("buckets"),
                    If(VersionAtLeast(1), &[I32("value_2c")]),
                ],
            ),
        ],
    ),
    // `FUN_140053bd0`: per slot, running.
    (
        "ParticleComponent",
        &[BASE, Block("particle", 0, &[List("slots", &[Bool("on")])])],
    ),
    // `FUN_14010b0d0`.
    (
        "AbsorbResourceComponent",
        &[BASE, CONTROLLER, Block("absorb", 0, &[])],
    ),
    // `FUN_14010e250`: work done (`+0x48`), stages (`+0x4c`), `+0x50`, then the models and
    // picking components of its MultiObject parts.
    (
        "BuildComponent",
        &[
            BASE,
            CONTROLLER,
            Block(
                "build",
                1,
                &[
                    I32("progress"),
                    I32("stages"),
                    Bool("flag_50"),
                    List("models", &[Ptr("model")]),
                    List("pickings", &[Ptr("picking")]),
                ],
            ),
        ],
    ),
    // `FUN_1400572d0`.
    ("StorageDisplayComponent", &[BASE, Block("display", 0, &[])]),
    // `FUN_1401331d0`: per page, a byte.
    (
        "UIComponent",
        &[BASE, Block("ui", 0, &[List("pages", &[I8("page")])])],
    ),
    // `FUN_1401345f0`: workers, enabled (`+0x50`), profession (`+0x48`, from version 1).
    (
        "WorkPlaceComponent",
        &[
            BASE,
            CONTROLLER,
            Block(
                "workplace",
                1,
                &[
                    List("workers", &[Ptr("worker")]),
                    Bool("enabled"),
                    If(VersionAtLeast(1), &[Ext("profession")]),
                ],
            ),
        ],
    ),
];
