//! The game-state stream of a save: the game loop's serializer (`FUN_14009af60`) over every
//! manager and every object. See docs/SAVE-GAME-STATE.md.
//!
//! Objects (`Object::Internal`) are written where they are first referenced
//! (`InternalBasePtr::Serialize`, Runtime `0x1800c29f0`): a present byte, the object's factory
//! index, its tag and its type id; the first time an index appears its serializer (vtable
//! `+0x18`) follows inline (`InternalFactory::PersistObject`, `0x1800c2780`).
//!
//! Each type's serializer is described by a schema ([`crate::savegame_schema`]) of fields,
//! blocks, lists and version conditions, read (and written) by one interpreter. Every block
//! must be consumed exactly.

use std::collections::{BTreeMap, BTreeSet};

use crate::crs::{Reader, Writer, BLOCK_END, BLOCK_START};
use crate::error::{format_err, Error, Result};
use crate::savegame_schema::{schema_for, Cond, Op, ACHIEVEMENTS, TOWN};

/// An object's factory index (`Internal +0x08`).
pub type ObjId = i32;

/// A decoded value.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Bool(bool),
    I8(i8),
    I16(i16),
    U16(u16),
    I32(i32),
    U32(u32),
    I64(i64),
    F32(f32),
    F32s(Vec<f32>),
    I64s(Vec<i64>),
    U32s(Vec<u32>),
    Str(String),
    /// An external reference id (`u32::MAX` for none).
    Ext(u32),
    Ptr(Option<ObjId>),
    List(Vec<Fields>),
    /// A nested block: its version and fields.
    Block(u32, Fields),
}

/// Named values in serialization order.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Fields(pub Vec<(&'static str, Value)>);

impl Fields {
    pub fn get(&self, name: &str) -> Option<&Value> {
        self.0.iter().find(|(n, _)| *n == name).map(|(_, v)| v)
    }

    /// A field inside nested blocks: `path` is `"block.field"`.
    pub fn at(&self, path: &str) -> Option<&Value> {
        let mut fields = self;
        let mut parts = path.split('.').peekable();
        while let Some(p) = parts.next() {
            let v = fields.get(p)?;
            if parts.peek().is_none() {
                return Some(v);
            }
            match v {
                Value::Block(_, f) => fields = f,
                _ => return None,
            }
        }
        None
    }

    /// [`Fields::at`] for writing.
    pub fn at_mut(&mut self, path: &str) -> Option<&mut Value> {
        let (head, rest) = match path.split_once('.') {
            Some((h, r)) => (h, Some(r)),
            None => (path, None),
        };
        let v = self
            .0
            .iter_mut()
            .find(|(n, _)| *n == head)
            .map(|(_, v)| v)?;
        match rest {
            None => Some(v),
            Some(rest) => match v {
                Value::Block(_, f) => f.at_mut(rest),
                _ => None,
            },
        }
    }

    /// Replace the value at `path`, keeping its kind (an `I32` stays an `I32`, a `Bool` that
    /// holds a raw byte takes the new truth value as 0 or 1 only if it changed). Returns whether
    /// the field exists.
    pub fn set(&mut self, path: &str, value: Value) -> bool {
        let Some(slot) = self.at_mut(path) else {
            return false;
        };
        // A raw non-0/1 bool byte stays as written while its truth value is unchanged.
        if let (Value::I8(b), Value::Bool(v)) = (&*slot, &value) {
            if (*b != 0) == *v {
                return true;
            }
        }
        *slot = value;
        true
    }

    pub fn set_i32(&mut self, path: &str, v: i32) -> bool {
        match self.at_mut(path) {
            Some(Value::I32(x)) => *x = v,
            Some(Value::U32(x)) | Some(Value::Ext(x)) => *x = v as u32,
            Some(Value::I16(x)) => *x = v as i16,
            Some(Value::I8(x)) => *x = v as i8,
            Some(Value::U16(x)) => *x = v as u16,
            _ => return false,
        }
        true
    }

    pub fn set_f32(&mut self, path: &str, v: f32) -> bool {
        match self.at_mut(path) {
            Some(Value::F32(x)) => *x = v,
            _ => return false,
        }
        true
    }

    pub fn set_time(&mut self, path: &str, v: i64) -> bool {
        match self.at_mut(path) {
            Some(Value::I64(x)) => *x = v,
            _ => return false,
        }
        true
    }

    pub fn set_bool(&mut self, path: &str, v: bool) -> bool {
        self.set(path, Value::Bool(v))
    }

    pub fn i32(&self, path: &str) -> Option<i32> {
        match self.at(path)? {
            Value::I32(v) => Some(*v),
            Value::I16(v) => Some(*v as i32),
            Value::I8(v) => Some(*v as i32),
            Value::U32(v) | Value::Ext(v) => Some(*v as i32),
            _ => None,
        }
    }

    pub fn f32(&self, path: &str) -> Option<f32> {
        match self.at(path)? {
            Value::F32(v) => Some(*v),
            _ => None,
        }
    }

    pub fn ptr(&self, path: &str) -> Option<ObjId> {
        match self.at(path)? {
            Value::Ptr(p) => *p,
            _ => None,
        }
    }
}

/// A decoded object.
#[derive(Debug, Clone)]
pub struct Object {
    pub index: ObjId,
    /// `Internal +0x0c`, drawn on the factory's own Random.
    pub tag: u32,
    pub type_id: u32,
    pub type_name: &'static str,
    pub fields: Fields,
}

/// The whole game state.
#[derive(Debug, Clone, Default)]
pub struct GameState {
    /// The external reference table (`ExternalFactory::SerializeReferenceTable`).
    pub references: BTreeMap<u32, String>,
    /// The loop's own fields (`Game +0x50`, `+0x40`, `+0x48`, `+0x30`, `+0x34`, `+0xd0`,
    /// `+0x15c`).
    pub climate: u32,
    pub start_condition: u32,
    pub terrain: u32,
    pub seed: u32,
    pub size: i32,
    pub flags: u32,
    pub scenario: i32,
    /// The game's Random (`System::Random::Serialize`): its index and 624 words.
    pub random_index: u32,
    pub random_state: Vec<u32>,
    pub town_name: String,
    /// `+0x158`, `+0x150`: the goal's state.
    pub goal_done: bool,
    pub goal: Option<ObjId>,
    /// The entity manager (`FUN_1400c9970`): its clock, the entities, the scheduler's five
    /// bucket lists (`FUN_140022500`), its two five-int arrays (`+0xa8`, `+0xbc`) and each
    /// entity's schedule (`+0x18`, `+0x1c` low 24 bits, `+0x1f`, the time `+0x20`).
    pub entity_time: i64,
    pub entities: Vec<Option<ObjId>>,
    pub buckets: Vec<Vec<Option<ObjId>>>,
    pub bucket_values: [[i32; 5]; 2],
    pub schedules: Vec<(i32, i32, i32, i64)>,
    /// The game loop serializer's version (3 in the samples).
    pub version: u32,
    /// The strings were written as UTF-16 (2015 builds) rather than 8-bit.
    pub wide_strings: bool,
    /// `InternalFactory::ResetPersistState`'s block, as written.
    pub persist_state: Vec<u8>,
    pub entity_manager_version: u32,
    /// `AchievementManager::Persist`: the achievement instances.
    pub achievements: Fields,
    /// Whole blocks (markers included) that are carried through undecoded; empty ones (a new
    /// game) are written as the engine's empty state ([`empty_animations`],
    /// [`empty_audio_scene`]).
    pub animations: Vec<u8>,
    pub audio_scene: Vec<u8>,
    /// The game camera (`FUN_14009dd90`).
    pub camera: Option<crate::menu_scene::SceneCamera>,
    /// The terrain (`FUN_1400b59a0`).
    pub terrain_map: Option<crate::menu_scene::MapComponent>,
    /// The town manager (`FUN_140094c30`).
    pub town: Fields,
    /// The gameplay map (`FUN_1400a45d0`).
    pub map: GameMap,
    /// The blocks after the map: `jobs`, `tools`, `calendar`, `event_log`, `environment` and
    /// `minimap` (see `savegame_schema`).
    pub managers: Fields,
    pub objects: BTreeMap<ObjId, Object>,
}

/// One gameplay map cell as `FUN_14001b570` writes it (48 bytes in memory).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GameMapCell {
    /// `+0x00`, `+0x08`.
    pub entities: [Option<ObjId>; 2],
    /// The description at `+0x10` (reference id, `u32::MAX` when unset).
    pub description: u32,
    /// Bytes `+0x1c..+0x25`.
    pub flags: [u8; 9],
    /// `+0x26`.
    pub value: u16,
    /// Bytes `+0x28..+0x2d`; `[1]` (`+0x29`) is the cell type.
    pub bytes: [u8; 6],
}

/// The gameplay map (`FUN_1400a45d0`, version 4): `size × size` cells and nine shorts (`+0x90`).
#[derive(Debug, Clone, Default)]
pub struct GameMap {
    pub size: i32,
    pub cells: Vec<GameMapCell>,
    pub trailer: [i16; 9],
}

struct Decoder<'a> {
    r: Reader<'a>,
    objects: BTreeMap<ObjId, Object>,
    defining: BTreeSet<ObjId>,
}

fn err<T>(at: usize, msg: impl Into<String>) -> Result<T> {
    Err(Error::Format(format!("{} at {at:#x}", msg.into())))
}

impl Decoder<'_> {
    fn ptr(&mut self) -> Result<Option<ObjId>> {
        if !self.r.bool()? {
            return Ok(None);
        }
        let at = self.r.pos();
        let index = self.r.i32()?;
        let tag = self.r.u32()?;
        let ty = self.r.u32()?;
        if self.objects.contains_key(&index) || self.defining.contains(&index) {
            return Ok(Some(index));
        }
        let Some((name, ops)) = schema_for(ty) else {
            return Err(Error::Unsupported(format!(
                "object type {ty:#010x} (index {index}) at {at:#x}"
            )));
        };
        self.defining.insert(index);
        let fields = self.ops(ops, 0).map_err(|e| match e {
            Error::Format(m) => Error::Format(format!("{m} [in {name} at {at:#x}]")),
            e => e,
        })?;
        self.defining.remove(&index);
        self.objects.insert(
            index,
            Object {
                index,
                tag,
                type_id: ty,
                type_name: name,
                fields,
            },
        );
        Ok(Some(index))
    }

    fn ptrs(&mut self) -> Result<Vec<Option<ObjId>>> {
        let n = self.r.count()?;
        (0..n).map(|_| self.ptr()).collect()
    }

    /// A block read by `f`, which must consume it.
    fn block<T>(&mut self, f: impl FnOnce(&mut Self, u32) -> Result<T>) -> Result<T> {
        let start = self.r.pos();
        if self.r.u32()? != BLOCK_START {
            return err(start, "expected a block");
        }
        let size = self.r.u32()? as usize;
        let version = self.r.u32()?;
        let end = self.r.pos() + size;
        if size < 4 || self.r.u32_at(end - 4) != Some(BLOCK_END) {
            return err(start, "block lacks its terminator");
        }
        let outer = self.r.limit(end - 4)?;
        let result = f(self, version);
        let unread = self.r.remaining();
        self.r.unlimit(outer);
        let value = result?;
        if unread != 0 {
            return err(
                start,
                format!("block (version {version}) has {unread} unread bytes"),
            );
        }
        self.r.set_pos(end);
        Ok(value)
    }

    /// `FUN_1400a45d0` with the cells of `FUN_14001b570`.
    fn game_map(&mut self) -> Result<GameMap> {
        self.block(|d, version| {
            if version < 3 {
                return err(d.r.pos(), "map versions below 3 are not supported");
            }
            let size = d.r.i32()?;
            let n = d.r.count()?;
            let mut cells = Vec::with_capacity(n);
            for _ in 0..n {
                let entities = [d.ptr()?, d.ptr()?];
                let description = d.r.u32()?;
                let flags = d.r.bytes(9)?.try_into().unwrap();
                let value = d.r.u16()?;
                let bytes = d.r.bytes(6)?.try_into().unwrap();
                cells.push(GameMapCell {
                    entities,
                    description,
                    flags,
                    value,
                    bytes,
                });
            }
            let mut trailer = [0; 9];
            for t in &mut trailer {
                *t = d.r.i16()?;
            }
            Ok(GameMap {
                size,
                cells,
                trailer,
            })
        })
    }

    /// One whole block's bytes, markers included.
    fn raw_block(&mut self) -> Result<Vec<u8>> {
        let start = self.r.pos();
        self.skip_block()?;
        Ok(self.r.slice(start, self.r.pos())?.to_vec())
    }

    fn skip_block(&mut self) -> Result<()> {
        self.block(|d, _| {
            d.r.skip_rest();
            Ok(())
        })
    }

    fn ops(&mut self, ops: &[Op], version: u32) -> Result<Fields> {
        let mut out = Fields::default();
        self.ops_into(ops, version, &mut out)?;
        Ok(out)
    }

    fn ops_into(&mut self, ops: &[Op], version: u32, out: &mut Fields) -> Result<()> {
        for op in ops {
            let (name, value) = match *op {
                // A byte other than 0 or 1 (an uninitialised bool) is kept as written.
                Op::Bool(n) => match self.r.u8()? {
                    b @ (0 | 1) => (n, Value::Bool(b == 1)),
                    b => (n, Value::I8(b as i8)),
                },
                Op::I8(n) => (n, Value::I8(self.r.u8()? as i8)),
                Op::I16(n) => (n, Value::I16(self.r.i16()?)),
                Op::U16(n) => (n, Value::U16(self.r.u16()?)),
                Op::I32(n) => (n, Value::I32(self.r.i32()?)),
                Op::U32(n) => (n, Value::U32(self.r.u32()?)),
                Op::Time(n) => (n, Value::I64(self.r.i64()?)),
                Op::F32(n) => (n, Value::F32(self.r.f32()?)),
                Op::Times(n, k) => (
                    n,
                    Value::I64s((0..k).map(|_| self.r.i64()).collect::<Result<_>>()?),
                ),
                Op::F32s(n, k) => (
                    n,
                    Value::F32s((0..k).map(|_| self.r.f32()).collect::<Result<_>>()?),
                ),
                Op::U32s(n, k) => (
                    n,
                    Value::U32s((0..k).map(|_| self.r.u32()).collect::<Result<_>>()?),
                ),
                Op::I32sBy(n, by) => {
                    let k = out.i32(by).unwrap_or(0).max(0) as usize;
                    (
                        n,
                        Value::U32s((0..k).map(|_| self.r.u32()).collect::<Result<_>>()?),
                    )
                }
                Op::F32sBy(n, by) => {
                    let k = out.i32(by).unwrap_or(0).max(0) as usize;
                    (
                        n,
                        Value::F32s((0..k).map(|_| self.r.f32()).collect::<Result<_>>()?),
                    )
                }
                Op::Str(n) => (n, Value::Str(self.r.string()?)),
                Op::WStr(n) => (n, Value::Str(self.r.wstring()?)),
                Op::Ext(n) => (n, Value::Ext(self.r.u32()?)),
                Op::Ptr(n) => (n, Value::Ptr(self.ptr()?)),
                Op::List(n, item) => {
                    let count = self.r.count()?;
                    let mut items = Vec::with_capacity(count.min(1 << 16));
                    for _ in 0..count {
                        items.push(self.ops(item, version)?);
                    }
                    (n, Value::List(items))
                }
                Op::Rest(n, item) => {
                    let mut items = Vec::new();
                    while self.r.remaining() > 0 {
                        items.push(self.ops(item, version)?);
                    }
                    (n, Value::List(items))
                }
                Op::Block(n, _, inner) => {
                    let (v, f) = self.block(|d, v| Ok((v, d.ops(inner, v)?)))?;
                    (n, Value::Block(v, f))
                }
                Op::Sub(inner) => {
                    self.ops_into(inner, version, out)?;
                    continue;
                }
                Op::If(cond, inner) => {
                    let take = match cond {
                        Cond::VersionAtLeast(v) => version >= v,
                        Cond::VersionBelow(v) => version < v,
                        Cond::Flag(f) => truthy(out.get(f)),
                    };
                    if take {
                        self.ops_into(inner, version, out)?;
                    }
                    continue;
                }
            };
            out.0.push((name, value));
        }
        Ok(())
    }
}

impl GameState {
    /// A new object of type `name` with default fields ([`crate::savegame_schema::default_fields`])
    /// and the next free factory index. The tag (`Internal +0x0c`, drawn on the factory's own
    /// Random in the original) is the index here; returns its id.
    pub fn new_object(&mut self, name: &'static str) -> Option<ObjId> {
        let fields = crate::savegame_schema::default_fields(name)?;
        let index = self.objects.keys().next_back().map_or(1, |&k| k + 1);
        self.objects.insert(
            index,
            Object {
                index,
                tag: index as u32,
                type_id: crate::crc::type_id(name),
                type_name: name,
                fields,
            },
        );
        Some(index)
    }

    /// Decode an expanded game-state stream (`crate::save::Save::state`).
    pub fn decode(state: &[u8]) -> Result<GameState> {
        let mut d = Decoder {
            r: Reader::new(state),
            objects: BTreeMap::new(),
            defining: BTreeSet::new(),
        };
        let mut g = GameState::default();
        d.block(|d, version| {
            g.version = version;
            g.references = d.block(|d, _| {
                let n = d.r.count()?;
                (0..n)
                    .map(|_| Ok((d.r.u32()?, d.r.string()?)))
                    .collect::<Result<BTreeMap<_, _>>>()
            })?;
            d.r.set_reference_ids(std::sync::Arc::new(g.references.clone()));
            g.persist_state = d.raw_block()?;
            g.climate = d.r.u32()?;
            g.start_condition = d.r.u32()?;
            g.terrain = d.r.u32()?;
            g.seed = d.r.u32()?;
            g.size = d.r.i32()?;
            if version > 0 {
                g.flags = d.r.u32()?;
            }
            if version > 2 {
                g.scenario = d.r.i32()?;
            }
            g.random_index = d.r.u32()?;
            g.random_state = (0..0x270).map(|_| d.r.u32()).collect::<Result<_>>()?;
            g.town_name = d.r.string()?;
            g.goal_done = d.r.bool()?;
            g.goal = d.ptr()?;
            // Kept as written: `AchievementManager::Persist`, `AnimationManager::Serialize` and
            // `Audio::Scene::Serialize` (engine side, not yet decoded).
            d.ops_into(&[ACHIEVEMENTS], 0, &mut g.achievements)?;
            g.animations = d.raw_block()?;
            g.audio_scene = d.raw_block()?;
            g.camera = Some(crate::menu_scene::read_camera(&mut d.r)?);
            d.block(|d, version| {
                g.entity_manager_version = version;
                g.entity_time = d.r.i64()?;
                g.entities = d.ptrs()?;
                g.buckets = (0..5).map(|_| d.ptrs()).collect::<Result<_>>()?;
                for k in 0..2 {
                    for v in 0..5 {
                        g.bucket_values[k][v] = d.r.i32()?;
                    }
                }
                g.schedules = (0..g.entities.len())
                    .map(|_| Ok((d.r.i32()?, d.r.i32()?, d.r.i32()?, d.r.i64()?)))
                    .collect::<Result<_>>()?;
                Ok(())
            })?;
            g.terrain_map = Some(crate::menu_scene::read_map_component(&mut d.r)?);
            d.ops_into(&[TOWN], 0, &mut g.town)?;
            g.map = d.game_map()?;
            d.ops_into(&managers(version), 0, &mut g.managers)?;
            Ok(())
        })?;
        g.objects = d.objects;
        g.wide_strings = d.r.wide_seen;
        Ok(g)
    }

    /// The writing side of [`GameState::decode`]: the same stream, with each object written
    /// inline where it is first referenced, in the order the decoder met them.
    pub fn encode(&self) -> Result<Vec<u8>> {
        let mut e = Encoder {
            w: Writer::new(),
            objects: &self.objects,
            written: BTreeSet::new(),
        };
        e.w.wide_strings = self.wide_strings;
        let at = e.w.begin_block(self.version);
        let refs = e.w.begin_block(0);
        e.w.count(self.references.len());
        for (&id, path) in &self.references {
            e.w.u32(id);
            e.w.string(path);
        }
        e.w.end_block(refs);
        e.w.set_reference_ids(&self.references);
        // `InternalFactory::ResetPersistState`: the number of object slots, which every index
        // must be below (raised when objects were added).
        let slots = self
            .objects
            .keys()
            .max()
            .map_or(0, |&m| m as u32 + 1)
            .max(persist_slots(&self.persist_state));
        let at_persist = e.w.begin_block(0);
        e.w.u32(slots);
        e.w.end_block(at_persist);
        e.w.u32(self.climate);
        e.w.u32(self.start_condition);
        e.w.u32(self.terrain);
        e.w.u32(self.seed);
        e.w.i32(self.size);
        if self.version > 0 {
            e.w.u32(self.flags);
        }
        if self.version > 2 {
            e.w.i32(self.scenario);
        }
        e.w.u32(self.random_index);
        self.random_state.iter().for_each(|&v| e.w.u32(v));
        e.w.string(&self.town_name);
        e.w.bool(self.goal_done);
        e.ptr(self.goal)?;
        e.ops(&[ACHIEVEMENTS], 0, &self.achievements)?;
        if self.animations.is_empty() {
            e.w.bytes(&empty_animations());
        } else {
            e.w.bytes(&self.animations);
        }
        if self.audio_scene.is_empty() {
            e.w.bytes(&empty_audio_scene());
        } else {
            e.w.bytes(&self.audio_scene);
        }
        let camera = self
            .camera
            .as_ref()
            .ok_or_else(|| Error::Format("no camera".into()))?;
        crate::menu_scene::write_camera(&mut e.w, camera)?;
        let em = e.w.begin_block(self.entity_manager_version);
        e.w.i64(self.entity_time);
        e.ptrs(&self.entities)?;
        for b in &self.buckets {
            e.ptrs(b)?;
        }
        self.bucket_values
            .iter()
            .flatten()
            .for_each(|&v| e.w.i32(v));
        for &(a, b, c, t) in &self.schedules {
            e.w.i32(a);
            e.w.i32(b);
            e.w.i32(c);
            e.w.i64(t);
        }
        e.w.end_block(em);
        let terrain = self
            .terrain_map
            .as_ref()
            .ok_or_else(|| Error::Format("no terrain".into()))?;
        crate::menu_scene::write_map_component(&mut e.w, terrain)?;
        e.ops(&[TOWN], 0, &self.town)?;
        e.game_map(&self.map)?;
        e.ops(&managers(self.version), 0, &self.managers)?;
        e.w.end_block(at);
        Ok(e.w.data)
    }
}

/// A `Bool` field as the game reads it: any non-zero byte is true.
fn truthy(v: Option<&Value>) -> bool {
    matches!(v, Some(Value::Bool(true))) || matches!(v, Some(Value::I8(b)) if *b != 0)
}

/// The blocks after the map; the event log from game-loop version 2.
/// The object slot count in a `ResetPersistState` block (marker, size, version, count).
fn persist_slots(block: &[u8]) -> u32 {
    block
        .get(12..16)
        .map_or(0, |b| u32::from_le_bytes(b.try_into().unwrap()))
}

/// `Graphics::AnimationManager::Serialize` (Runtime `0x1800fea20`) with no animated
/// instances: a count of 0.
pub fn empty_animations() -> Vec<u8> {
    let mut w = Writer::new();
    let at = w.begin_block(0);
    w.u32(0);
    w.end_block(at);
    w.data
}

/// `Audio::Scene::Serialize` (Runtime `0x1800e28a0`) with nothing playing: the 4096 sound
/// slots (`FUN_180053110`: position, a value, the effect, a time, a value, the volume 1.0 and
/// no points), then the five bytes at `+0x30`, as every sample has them.
pub fn empty_audio_scene() -> Vec<u8> {
    let mut w = Writer::new();
    let at = w.begin_block(0);
    w.u32(0x1000);
    for _ in 0..0x1000 {
        (0..3).for_each(|_| w.f32(0.0));
        w.u32(0);
        w.u32(u32::MAX);
        w.i64(0);
        w.u32(0);
        w.f32(1.0);
        w.u32(0);
    }
    w.bytes(&[0, 0, 1, 1, 1]);
    w.end_block(at);
    w.data
}

/// The blocks after the gameplay map (`jobs`, `tools`, `calendar`, `event_log`, `environment`,
/// `minimap`) at serializer version `version`.
pub fn managers(version: u32) -> Vec<Op> {
    use crate::savegame_schema::{CALENDAR, ENVIRONMENT, EVENT_LOG, JOBS, MINIMAP, TOOLS};
    let mut ops = vec![JOBS, TOOLS, CALENDAR];
    if version > 1 {
        ops.push(EVENT_LOG);
    }
    ops.extend([ENVIRONMENT, MINIMAP]);
    ops
}

struct Encoder<'a> {
    w: Writer,
    objects: &'a BTreeMap<ObjId, Object>,
    written: BTreeSet<ObjId>,
}

impl Encoder<'_> {
    fn ptr(&mut self, p: Option<ObjId>) -> Result<()> {
        let Some(index) = p else {
            self.w.bool(false);
            return Ok(());
        };
        let o = self
            .objects
            .get(&index)
            .ok_or_else(|| Error::Format(format!("object {index} is not defined")))?;
        self.w.bool(true);
        self.w.i32(index);
        self.w.u32(o.tag);
        self.w.u32(o.type_id);
        if self.written.insert(index) {
            let (_, ops) = schema_for(o.type_id)
                .ok_or_else(|| Error::Unsupported(format!("object type {:#010x}", o.type_id)))?;
            self.ops(ops, 0, &o.fields)?;
        }
        Ok(())
    }

    fn ptrs(&mut self, ptrs: &[Option<ObjId>]) -> Result<()> {
        self.w.count(ptrs.len());
        ptrs.iter().try_for_each(|&p| self.ptr(p))
    }

    fn game_map(&mut self, map: &GameMap) -> Result<()> {
        let at = self.w.begin_block(4);
        self.w.i32(map.size);
        self.w.count(map.cells.len());
        for c in &map.cells {
            self.ptr(c.entities[0])?;
            self.ptr(c.entities[1])?;
            self.w.u32(c.description);
            self.w.bytes(&c.flags);
            self.w.u16(c.value);
            self.w.bytes(&c.bytes);
        }
        map.trailer.iter().for_each(|&t| self.w.i16(t));
        self.w.end_block(at);
        Ok(())
    }

    fn ops(&mut self, ops: &[Op], version: u32, fields: &Fields) -> Result<()> {
        let mut next = fields.0.iter();
        self.ops_from(ops, version, fields, &mut next)?;
        match next.next() {
            None => Ok(()),
            Some((n, _)) => format_err(format!("field {n} has no step to write it")),
        }
    }

    /// Write `ops` from the fields `next` yields (`fields` is the whole set, for conditions).
    fn ops_from<'f>(
        &mut self,
        ops: &[Op],
        version: u32,
        fields: &'f Fields,
        next: &mut std::slice::Iter<'f, (&'static str, Value)>,
    ) -> Result<()> {
        for op in ops {
            let name = match *op {
                Op::Sub(inner) => {
                    self.ops_from(inner, version, fields, next)?;
                    continue;
                }
                Op::If(cond, inner) => {
                    let take = match cond {
                        Cond::VersionAtLeast(v) => version >= v,
                        Cond::VersionBelow(v) => version < v,
                        Cond::Flag(f) => truthy(fields.get(f)),
                    };
                    if take {
                        self.ops_from(inner, version, fields, next)?;
                    }
                    continue;
                }
                Op::Bool(n) | Op::I8(n) | Op::I16(n) | Op::U16(n) | Op::I32(n) | Op::U32(n) => n,
                Op::Time(n) | Op::F32(n) | Op::Times(n, _) | Op::F32s(n, _) | Op::U32s(n, _) => n,
                Op::I32sBy(n, _) | Op::F32sBy(n, _) | Op::Str(n) | Op::WStr(n) | Op::Ext(n) => n,
                Op::Ptr(n) | Op::List(n, _) | Op::Rest(n, _) | Op::Block(n, _, _) => n,
            };
            let Some((field, value)) = next.next() else {
                return format_err(format!("missing field {name}"));
            };
            if *field != name {
                return format_err(format!("expected field {name}, found {field}"));
            }
            let w = &mut self.w;
            match (*op, value) {
                (Op::Bool(_), Value::Bool(v)) => w.bool(*v),
                (Op::Bool(_), Value::I8(v)) => w.u8(*v as u8),
                (Op::I8(_), Value::I8(v)) => w.u8(*v as u8),
                (Op::I16(_), Value::I16(v)) => w.i16(*v),
                (Op::U16(_), Value::U16(v)) => w.u16(*v),
                (Op::I32(_), Value::I32(v)) => w.i32(*v),
                (Op::U32(_), Value::U32(v)) | (Op::Ext(_), Value::Ext(v)) => w.u32(*v),
                (Op::Time(_), Value::I64(v)) => w.i64(*v),
                (Op::F32(_), Value::F32(v)) => w.f32(*v),
                (Op::Times(_, _), Value::I64s(v)) => v.iter().for_each(|&t| w.i64(t)),
                (Op::F32s(_, _) | Op::F32sBy(_, _), Value::F32s(v)) => {
                    v.iter().for_each(|&f| w.f32(f))
                }
                (Op::U32s(_, _) | Op::I32sBy(_, _), Value::U32s(v)) => {
                    v.iter().for_each(|&u| w.u32(u))
                }
                (Op::Str(_), Value::Str(v)) => w.string(v),
                (Op::WStr(_), Value::Str(v)) => w.wstring(v),
                (Op::Ptr(_), Value::Ptr(p)) => self.ptr(*p)?,
                (Op::List(_, item), Value::List(items)) => {
                    w.count(items.len());
                    for f in items {
                        self.ops(item, version, f)?;
                    }
                }
                (Op::Rest(_, item), Value::List(items)) => {
                    for f in items {
                        self.ops(item, version, f)?;
                    }
                }
                (Op::Block(_, _, inner), Value::Block(v, f)) => {
                    let at = w.begin_block(*v);
                    self.ops(inner, *v, f)?;
                    self.w.end_block(at);
                }
                (op, value) => return format_err(format!("{op:?} cannot write {value:?}")),
            }
        }
        Ok(())
    }
}
