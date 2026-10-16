//! The serializers of the objects a save holds, as schemas (see [`crate::savegame`]). Each
//! entry names the Application function it follows; field names give the in-memory offset
//! where no better name is known.

use std::collections::HashMap;
use std::sync::OnceLock;

use crate::crc::type_id;

/// A version test against the enclosing block's version.
#[derive(Debug, Clone, Copy)]
pub enum Cond {
    VersionAtLeast(u32),
    VersionBelow(u32),
    /// An earlier `Bool` of the same fields is true.
    Flag(&'static str),
}

/// One step of a serializer.
#[derive(Debug, Clone, Copy)]
pub enum Op {
    Bool(&'static str),
    I8(&'static str),
    I16(&'static str),
    U16(&'static str),
    I32(&'static str),
    U32(&'static str),
    /// `System::Time`: an `i64` of ticks.
    Time(&'static str),
    F32(&'static str),
    /// A fixed number of `System::Time`s.
    Times(&'static str, usize),
    F32s(&'static str, usize),
    U32s(&'static str, usize),
    /// `i32`s counted by an earlier field of the same fields.
    I32sBy(&'static str, &'static str),
    /// `f32`s counted by an earlier field.
    F32sBy(&'static str, &'static str),
    /// `System::String` with the 8-bit flag.
    Str(&'static str),
    /// A UTF-16 string.
    WStr(&'static str),
    /// An `ExternalPtr` in reference mode: the reference id.
    Ext(&'static str),
    /// An `InternalBasePtr`.
    Ptr(&'static str),
    /// An `i32` count, then the items.
    List(&'static str, &'static [Op]),
    /// Items repeated until the enclosing block ends (no count is written).
    Rest(&'static str, &'static [Op]),
    /// An `IO::Serializer` block written at this version.
    Block(&'static str, u32, &'static [Op]),
    /// Inline steps (shared fragments).
    Sub(&'static [Op]),
    If(Cond, &'static [Op]),
}

use Cond::*;
use Op::*;

/// `Component::Serialize` (`FUN_14011de80`): `Internal::Persist` writes nothing, then the
/// entity, flags (`+0x20`) and description (`+0x18`).
/// `Work::Serialize` (`FUN_1400fc8d0`, version 3): the entity and its WorkComponent, the
/// worker counts and the times.
const WORK: Op = Block(
    "work",
    3,
    &[
        Ptr("entity"),
        Ptr("component"),
        I32("id"),
        I32("cell"),
        If(VersionAtLeast(1), &[I32("priority")]),
        If(
            VersionAtLeast(3),
            &[Time("patience"), Time("patience_base"), I32("far_count")],
        ),
    ],
);

/// `AutoPickupWork`: Work, then an empty block.
const AUTO_PICKUP_WORK: Op = Sub(&[WORK, Block("auto_pickup", 0, &[])]);

/// `FUN_14002a010`: a set of citizens.
const CITIZENS: Op = List("citizens", &[Ptr("citizen")]);

/// `FUN_14002a740`: key → citizen.
const KEYED_CITIZENS: Op = List("keyed", &[I32("key"), Ptr("citizen")]);

const BASE: Op = Block(
    "base",
    0,
    &[Ptr("entity"), U32("flags"), Ext("description")],
);
/// `Action::Serialize`: started (`+0x18`).
const ACTION: Op = Block("action", 0, &[Bool("started")]);
/// `ControllerComponent`'s own (empty) block.
const CONTROLLER: Op = Block("controller", 0, &[]);
/// `ResourceProduceComponent::Serialize` (`FUN_140130970`): switched off (`+0x48`).
const RESOURCE_PRODUCE: Op = Sub(&[BASE, CONTROLLER, Block("produce", 0, &[Bool("disabled")])]);

/// `Graphics::MeshInstance::Serialize` (`0x180101f10`): `AnimationState` entries
/// (`FUN_18005d610`), its flag, then the material references.
const MESH_INSTANCE: Op = Sub(&[
    List(
        "animations",
        &[
            Ext("animation"),
            I32("index"),
            F32("speed"),
            Time("time0"),
            Time("time1"),
            Time("time2"),
            I32("flags"),
            Bool("looping"),
        ],
    ),
    Bool("animating"),
    List("materials", &[Ext("material")]),
]);

static SCHEMAS: &[(&str, &[Op])] = &[
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
                    Time("age"),
                    I32("value_28"),
                    F32("value_44"),
                    F32("value_48"),
                    Bool("flag_40"),
                    Time("time_50"),
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
                    Bool("flag_48"),
                    Time("time_50"),
                    Time("time_58"),
                    I32("value_4c"),
                    Ptr("population"),
                    I32("value_30"),
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
                    Ext("template"),
                    I32("value_30"),
                    I32("value_48"),
                    List("cells", &[I32("a"), I32("c"), I32("b")]),
                    List("herds", &[Ptr("herd")]),
                    Time("time_60"),
                    I32("value_68"),
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
                    If(VersionBelow(3), &[I32("old")]),
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
            Block("road", 0, &[I32("cell"), I8("kind"), I32("value_24")]),
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
                    Ptr("entity"),
                    I32("param"),
                    If(VersionAtLeast(1), &[I32("value_30")]),
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
            Block("livestock", 0, &[Ptr("pasture"), Time("time_30")]),
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
                    Ptr("ptr_28"),
                    F32s("position", 3),
                    F32("value_4c"),
                    Ptr("ptr_30"),
                    Ptr("ptr_38"),
                    F32s("value_50", 3),
                    I32("value_70"),
                    List("path", &[F32s("point", 3)]),
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
                    Ptr("entity"),
                    I32("value_44"),
                    F32("value_4c"),
                    Bool("flag_50"),
                    I32("value_40"),
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
                    Ptr("storage"),
                    Ptr("value_38"),
                    Ext("material"),
                    Bool("flag_20"),
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

/// The schema for a type id: its name and steps.
/// An entity list (`FUN_140008150` and the inline copies): a count, then the pointers.
const fn ptr_list(name: &'static str) -> Op {
    List(name, &[Ptr("entity")])
}

/// The town manager (`FUN_140094c30`, version 6), `Game +0xf0`: the registries of buildings
/// and people by kind, the professions' workers, the stock targets and the statistics.
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

/// The job manager (`FUN_1400fe710`, version 3), `Game +0x118`: the id → work table (`+0x30`)
/// and the 14 category sets (`+0x40..`, 13 before version 3) by `FUN_14002d9c0`, the work by
/// profession (`+0x120`, `FUN_14002df50`), then the last id, the next priority and the pass
/// (`+0x00..+0x08`). The priority index (`+0x10`) is rebuilt on load from each work's priority.
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

/// The tool manager (`FUN_140107df0`, version 2).
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

/// The calendar block of `FUN_14009af60`.
pub const CALENDAR: Op = Block("calendar", 0, &[I32("value"), Time("time")]);

/// The event log (`FUN_14009a790`, version 1), `Game +0x138`.
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

/// The environment (`FUN_140099160`): climate and month descriptions, weather state, then one
/// entry per weather effect of the climate (the count is not written).
pub const ENVIRONMENT: Op = Block(
    "environment",
    0,
    &[
        Ext("climate"),
        Ext("month"),
        I32("value_10"),
        I32("value_14"),
        F32("value_18"),
        F32s("vector_1c", 4),
        F32("value_2c"),
        F32("value_30"),
        F32("value_44"),
        F32("value_48"),
        I32("value_4c"),
        Rest(
            "effects",
            &[
                Bool("active"),
                If(Flag("active"), &[F32("value_c4"), F32("value_c0")]),
            ],
        ),
    ],
);

/// The minimap (`FUN_1400aacc0`): the two images' texels, then the image size.
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

pub fn schema_for(ty: u32) -> Option<(&'static str, &'static [Op])> {
    static BY_ID: OnceLock<HashMap<u32, (&'static str, &'static [Op])>> = OnceLock::new();
    BY_ID
        .get_or_init(|| {
            SCHEMAS
                .iter()
                .map(|&(name, ops)| (type_id(name), (name, ops)))
                .collect()
        })
        .get(&ty)
        .copied()
}
