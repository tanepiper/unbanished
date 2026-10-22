//! The game-state stream of a save: the game loop's serializer over every manager and every object.
//! See docs/SAVE-GAME-STATE.md.
//!
//! Objects (`Object::Internal`) are written where they are first referenced
//! (`InternalBasePtr::Serialize`): a present byte, the object's factory index, its tag and its type
//! id; the first time an index appears its serializer follows inline
//! (`InternalFactory::PersistObject`).
//!
//! Each type's serializer is described by a schema ([`crate::savegame_schema`]) of fields, blocks,
//! lists and version conditions, read (and written) by one interpreter. Every block must be
//! consumed exactly.
//!
//! Original: `FUN_14009af60`, Runtime `0x1800c29f0`, Runtime `0x1800c2780`.
mod decoder;
mod encoder;
mod fields;
use decoder::Decoder;
use encoder::Encoder;
pub use fields::{Fields, Value};

// vtable +0x18

use std::collections::{BTreeMap, BTreeSet};

use crate::crs::{Reader, Writer, BLOCK_END, BLOCK_START};
use crate::error::{format_err, Error, Result};
use crate::savegame_schema::{schema_for, Cond, Op, ACHIEVEMENTS, TOWN};

// Internal +0x08
/// An object's factory index (`Internal`).
pub type ObjId = i32;

/// A decoded object.
#[derive(Debug, Clone)]
pub struct Object {
    /// Its number in the stream.
    pub index: ObjId,
    // Internal +0x0c
    /// `Internal`, drawn on the factory's own Random.
    pub tag: u32,
    /// The type's id.
    pub type_id: u32,
    /// The type's name.
    pub type_name: &'static str,
    /// The decoded fields.
    pub fields: Fields,
}

/// The whole game state.
#[derive(Debug, Clone, Default)]
pub struct GameState {
    /// The external reference table (`ExternalFactory::SerializeReferenceTable`).
    pub references: BTreeMap<u32, String>,
    // +0x40; +0x48; +0x30; +0x34; +0xd0; +0x15c; Game +0x50
    /// The climate.
    pub climate: u32,
    /// The start condition.
    pub start_condition: u32,
    /// The terrain type.
    pub terrain: u32,
    /// The map seed.
    pub seed: u32,
    /// The map size.
    pub size: i32,
    /// The game's flags.
    pub flags: u32,
    /// The scenario.
    pub scenario: i32,
    /// The index of the game's Random (`System::Random::Serialize`).
    pub random_index: u32,
    /// The 624 words of the game's Random.
    pub random_state: Vec<u32>,
    /// The town's name.
    pub town_name: String,
    // +0x158, +0x150
    /// The goal's state.
    pub goal_done: bool,
    /// The goal, if any.
    pub goal: Option<ObjId>,
    // +0xa8; +0xbc; +0x18; +0x1f; +0x1c; +0x20
    /// The entity manager's clock.
    ///
    /// Original: `FUN_1400c9970`, `FUN_140022500`.
    pub entity_time: i64,
    /// The entities.
    pub entities: Vec<Option<ObjId>>,
    /// The scheduler's five bucket lists.
    pub buckets: Vec<Vec<Option<ObjId>>>,
    /// The scheduler's two five-int arrays.
    pub bucket_values: [[i32; 5]; 2],
    /// Each entity's schedule (the low 24 bits; the time).
    pub schedules: Vec<(i32, i32, i32, i64)>,
    /// The game loop serializer's version (3 in the samples).
    pub version: u32,
    /// The strings were written as UTF-16 (2015 builds) rather than 8-bit.
    pub wide_strings: bool,
    /// `InternalFactory::ResetPersistState`'s block, as written.
    pub persist_state: Vec<u8>,
    /// The entity manager block's version.
    pub entity_manager_version: u32,
    /// `AchievementManager::Persist`: the achievement instances.
    pub achievements: Fields,
    /// The animations: a whole block (markers included) carried through undecoded. An empty one
    /// (a new game) is written as the engine's empty state ([`empty_animations`]).
    pub animations: Vec<u8>,
    /// The sound scene: a whole block carried through undecoded. An empty one is written as the engine's empty state ([`empty_audio_scene`]).
    pub audio_scene: Vec<u8>,
    /// The game camera.
    ///
    /// Original: `FUN_14009dd90`.
    pub camera: Option<crate::menu_scene::SceneCamera>,
    /// The terrain.
    ///
    /// Original: `FUN_1400b59a0`.
    pub terrain_map: Option<crate::menu_scene::MapComponent>,
    /// The town manager.
    ///
    /// Original: `FUN_140094c30`.
    pub town: Fields,
    /// The gameplay map.
    ///
    /// Original: `FUN_1400a45d0`.
    pub map: GameMap,
    /// The blocks after the map: `jobs`, `tools`, `calendar`, `event_log`, `environment` and
    /// `minimap` (see `savegame_schema`).
    pub managers: Fields,
    /// Every object in the stream, by number.
    pub objects: BTreeMap<ObjId, Object>,
}

/// One cell of the gameplay map, as the game state stores it (48 bytes in memory).
///
/// Original: `FUN_14001b570`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GameMapCell {
    // +0x00; +0x08
    /// Not identified yet.
    pub entities: [Option<ObjId>; 2],
    // +0x10
    /// The description at (reference id, `u32::MAX` when unset).
    pub description: u32,
    // +0x1c..+0x25
    /// Bytes.
    pub flags: [u8; 9],
    // +0x26
    /// Not identified yet.
    pub value: u16,
    // +0x29; +0x28..+0x2d
    /// Bytes; `[1]` is the cell type.
    pub bytes: [u8; 6],
}

// +0x90
/// The gameplay map (version 4).
///
/// Original: `FUN_1400a45d0`.
#[derive(Debug, Clone, Default)]
pub struct GameMap {
    /// The map's size in cells.
    pub size: i32,
    /// The `size × size` cells.
    pub cells: Vec<GameMapCell>,
    /// Nine shorts after the cells; not identified yet.
    pub trailer: [i16; 9],
}

impl GameState {
    // Internal +0x0c
    /// A new object of type `name` with default fields ([`crate::savegame_schema::default_fields`])
    /// and the next free factory index. The tag (`Internal`, drawn on the factory's own Random in
    /// the original) is the index here; returns its id.
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

/// `Graphics::AnimationManager::Serialize` with no animated instances: a count of 0.
///
/// Original: Runtime `0x1800fea20`.
pub fn empty_animations() -> Vec<u8> {
    let mut w = Writer::new();
    let at = w.begin_block(0);
    w.u32(0);
    w.end_block(at);
    w.data
}

// Each slot is written by Runtime 0x180053110; the five bytes are the scene's +0x30.
/// The sound scene's block with nothing playing, as a new save writes it: 4,096 empty sound slots
/// (a position, a value, the effect, a time, a value, the volume 1.0 and no points), then the five
/// bytes that every sample save has.
///
/// Original: `Audio::Scene::Serialize`, Runtime `0x1800e28a0`.
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
