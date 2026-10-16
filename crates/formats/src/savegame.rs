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

use crate::crs::{Reader, BLOCK_END, BLOCK_START};
use crate::error::{Error, Result};
use crate::savegame_schema::{schema_for, Cond, Op};

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
    /// Whole blocks (markers included) that are carried through undecoded.
    pub achievements: Vec<u8>,
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
                Op::Bool(n) => (n, Value::Bool(self.r.bool()?)),
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
                        Cond::Flag(f) => matches!(out.get(f), Some(Value::Bool(true))),
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
    /// Decode an expanded game-state stream (`crate::save::Save::state`).
    pub fn decode(state: &[u8]) -> Result<GameState> {
        let mut d = Decoder {
            r: Reader::new(state),
            objects: BTreeMap::new(),
            defining: BTreeSet::new(),
        };
        let mut g = GameState::default();
        d.block(|d, _| {
            g.references = d.block(|d, _| {
                let n = d.r.count()?;
                (0..n)
                    .map(|_| Ok((d.r.u32()?, d.r.string()?)))
                    .collect::<Result<BTreeMap<_, _>>>()
            })?;
            d.r.set_reference_ids(std::sync::Arc::new(g.references.clone()));
            // `InternalFactory::ResetPersistState`'s block.
            d.skip_block()?;
            g.climate = d.r.u32()?;
            g.start_condition = d.r.u32()?;
            g.terrain = d.r.u32()?;
            g.seed = d.r.u32()?;
            g.size = d.r.i32()?;
            g.flags = d.r.u32()?;
            g.scenario = d.r.i32()?;
            g.random_index = d.r.u32()?;
            g.random_state = (0..0x270).map(|_| d.r.u32()).collect::<Result<_>>()?;
            g.town_name = d.r.string()?;
            g.goal_done = d.r.bool()?;
            g.goal = d.ptr()?;
            // Kept as written: `AchievementManager::Persist`, `AnimationManager::Serialize` and
            // `Audio::Scene::Serialize` (engine side, not yet decoded).
            g.achievements = d.raw_block()?;
            g.animations = d.raw_block()?;
            g.audio_scene = d.raw_block()?;
            g.camera = Some(crate::menu_scene::read_camera(&mut d.r)?);
            d.block(|d, _| {
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
            d.ops_into(&[crate::savegame_schema::TOWN], 0, &mut g.town)?;
            g.map = d.game_map()?;
            use crate::savegame_schema::{CALENDAR, ENVIRONMENT, EVENT_LOG, JOBS, MINIMAP, TOOLS};
            d.ops_into(
                &[JOBS, TOOLS, CALENDAR, EVENT_LOG, ENVIRONMENT, MINIMAP],
                0,
                &mut g.managers,
            )?;
            Ok(())
        })?;
        g.objects = d.objects;
        Ok(g)
    }
}
