//! The serializers of the objects a save holds, as schemas (see [`crate::savegame`]). Each
//! entry names the Application function it follows; field names give the in-memory offset
//! where no better name is known.

use std::collections::HashMap;
use std::sync::OnceLock;

use crate::crc::type_id;

mod actions;
mod events;
mod kinds;
mod managers;
mod objects;
mod resources;
mod work;

pub use managers::*;

/// A version test against the enclosing block's version.
#[derive(Debug, Clone, Copy)]
pub enum Cond {
    /// The block's version is at least this.
    VersionAtLeast(u32),
    /// The block's version is below this.
    VersionBelow(u32),
    /// An earlier `Bool` of the same fields is true.
    Flag(&'static str),
}

/// One step of a serializer.
#[derive(Debug, Clone, Copy)]
pub enum Op {
    /// A named bool.
    Bool(&'static str),
    /// A named `i8`.
    I8(&'static str),
    /// A named `i16`.
    I16(&'static str),
    /// A named `u16`.
    U16(&'static str),
    /// A named `i32`.
    I32(&'static str),
    /// A named `u32`.
    U32(&'static str),
    /// `System::Time`: an `i64` of ticks.
    Time(&'static str),
    /// A named `f32`.
    F32(&'static str),
    /// A fixed number of `System::Time`s.
    Times(&'static str, usize),
    /// A fixed number of `f32`s.
    F32s(&'static str, usize),
    /// A fixed number of `u32`s.
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
    /// Steps read only when the condition holds.
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

// Runtime FUN_18005d610, one 0x38-byte entry each: the group +0x00, the clip +0x08, the speed
// +0x10, the clock +0x18, the blend time +0x20 and what is left of it +0x28, the shared instance
// +0x0c, and +0x30.
/// One layer of an `AnimationState` (newest first): the animation group, the clip, the speed, the
/// clip's clock, the blend time and the blend time left while a newer layer covers it, the shared
/// instance an instanced clip follows (−1 for none), and whether the layer has been updated once.
pub const ANIMATION_STATE: &[Op] = &[
    Ext("animation"),
    I32("clip"),
    F32("speed"),
    Time("elapsed"),
    Time("blend"),
    Time("left"),
    I32("instance"),
    Bool("started"),
];

/// `Graphics::MeshInstance::Serialize` (`0x180101f10`): `AnimationState` entries
/// (`FUN_18005d610`), its flag, then the material references.
const MESH_INSTANCE: Op = Sub(&[
    List("animations", ANIMATION_STATE),
    Bool("animating"),
    List("materials", &[Ext("material")]),
]);

const GOAL_INSTANCE: Op = Block("goal", 0, &[Ext("goal"), I32("value")]);

/// The object schemas, in groups (one per child module), in the table's order.
static SCHEMAS: &[&[(&str, &[Op])]] = &[
    objects::SCHEMAS,
    actions::SCHEMAS,
    kinds::SCHEMAS,
    work::SCHEMAS,
    events::SCHEMAS,
    resources::SCHEMAS,
];

/// The schema for a type id: its name and steps.
/// An entity list (`FUN_140008150` and the inline copies): a count, then the pointers.
const fn ptr_list(name: &'static str) -> Op {
    List(name, &[Ptr("entity")])
}

/// The class name and the schema of an object type, by type id.
pub fn schema_for(ty: u32) -> Option<(&'static str, &'static [Op])> {
    static BY_ID: OnceLock<HashMap<u32, (&'static str, &'static [Op])>> = OnceLock::new();
    BY_ID
        .get_or_init(|| {
            SCHEMAS
                .iter()
                .flat_map(|g| g.iter())
                .map(|&(name, ops)| (type_id(name), (name, ops)))
                .collect()
        })
        .get(&ty)
        .copied()
}

/// A new object's fields for type `name`: every step's default (zero, false, none, empty), in
/// the schema's order, each block at its version. The writer fills them in.
pub fn default_fields(name: &str) -> Option<crate::savegame::Fields> {
    let (_, ops) = schema_for(type_id(name))?;
    let mut out = crate::savegame::Fields::default();
    defaults_into(ops, 0, &mut out);
    Some(out)
}

/// A block's fields at their defaults (the town, the managers), as [`default_fields`] makes
/// an object's.
pub fn default_block(ops: &[Op], version: u32) -> crate::savegame::Fields {
    let mut out = crate::savegame::Fields::default();
    defaults_into(ops, version, &mut out);
    out
}

fn defaults_into(ops: &[Op], version: u32, out: &mut crate::savegame::Fields) {
    use crate::savegame::{Fields, Value};
    for op in ops {
        let (name, value) = match *op {
            Bool(n) => (n, Value::Bool(false)),
            I8(n) => (n, Value::I8(0)),
            I16(n) => (n, Value::I16(0)),
            U16(n) => (n, Value::U16(0)),
            I32(n) => (n, Value::I32(0)),
            U32(n) => (n, Value::U32(0)),
            Time(n) => (n, Value::I64(0)),
            F32(n) => (n, Value::F32(0.0)),
            Times(n, k) => (n, Value::I64s(vec![0; k])),
            F32s(n, k) => (n, Value::F32s(vec![0.0; k])),
            U32s(n, k) => (n, Value::U32s(vec![0; k])),
            I32sBy(n, _) => (n, Value::U32s(Vec::new())),
            F32sBy(n, _) => (n, Value::F32s(Vec::new())),
            Str(n) | WStr(n) => (n, Value::Str(String::new())),
            Ext(n) => (n, Value::Ext(u32::MAX)),
            Ptr(n) => (n, Value::Ptr(None)),
            List(n, _) | Rest(n, _) => (n, Value::List(Vec::new())),
            Block(n, v, inner) => {
                let mut f = Fields::default();
                defaults_into(inner, v, &mut f);
                (n, Value::Block(v, f))
            }
            Sub(inner) => {
                defaults_into(inner, version, out);
                continue;
            }
            If(cond, inner) => {
                let take = match cond {
                    VersionAtLeast(v) => version >= v,
                    VersionBelow(v) => version < v,
                    Flag(_) => false,
                };
                if take {
                    defaults_into(inner, version, out);
                }
                continue;
            }
        };
        out.0.push((name, value));
    }
}

/// Every object type the schemas cover.
pub fn type_names() -> impl Iterator<Item = &'static str> {
    SCHEMAS.iter().flat_map(|g| g.iter()).map(|&(name, _)| name)
}
