//! Saving a game that was not loaded from a save: the state starts empty and the writers fill
//! it (docs/SAVE-WRITING.md, "New games").
//!
//! The loop's fields are the New Game settings (`FUN_14009af60`). The entity manager starts
//! with no entities: its five bucket lists empty, the quotas `[50, 15, 8, 5, 2]` (`+0xa8`), the
//! cursors 0. The terrain is the generated map's (`FUN_1400b59a0`'s block, version 6). The
//! gameplay map's cells have no description (`u32::MAX`), and the town and managers are at
//! their defaults, with the lists the writers fill entry by entry sized as the port's state.
//! Every entity is then created by [`StartingTown::export_into`] (no saved ids).

use banished_formats::menu_scene::{MapComponent, SceneCamera};
use banished_formats::savegame::{self, Fields, GameMapCell, GameState, Value};
use banished_formats::savegame_schema::{default_block, ACHIEVEMENTS, TOWN};

use super::Refs;
use crate::world::WorldParams;

/// The game serializer's version (`FUN_14009af60`, 3 in every current save).
const VERSION: u32 = 3;
/// The scheduler's per-bucket quotas (`EntityManager +0xa8`).
const QUOTAS: [i32; 5] = [50, 15, 8, 5, 2];

/// What the skeleton needs besides the settings.
pub struct NewGame<'a> {
    pub params: &'a WorldParams,
    /// `Game +0xd0`: 0 with disasters, `0xf0` without.
    pub flags: u32,
    /// The terrain block.
    pub map: MapComponent,
    /// The weather effects of the climate, the Town Info series, the stock slots and the
    /// storage capacity records: lists the writers fill entry by entry.
    pub effects: usize,
    pub history: usize,
    pub stock: usize,
    pub capacity: usize,
}

/// The camera's transform for its pivot, yaw, pitch and distance: the eye at
/// `target − direction(yaw, pitch) × distance` and the rotation `z(yaw) · x(−pitch)`
/// (inferred from the saved cameras; the controller recomputes both every frame).
pub fn camera(target: [f32; 2], yaw: f32, pitch: f32, distance: f32) -> SceneCamera {
    let (y, p) = (yaw.to_radians(), pitch.to_radians());
    let direction = crate::scene::from_yaw_pitch(yaw, pitch);
    let target3 = glam::Vec3::new(target[0], target[1], 0.0);
    let eye = target3 - direction * distance;
    let q = glam::Quat::from_rotation_z(y) * glam::Quat::from_rotation_x(-p);
    SceneCamera {
        rotation: [q.x, q.y, q.z, q.w],
        position: eye.to_array(),
        target: [target[0], target[1], 0.0],
        floats: [yaw, pitch, distance],
    }
}

/// A new game's state before the writers run.
pub fn state(n: NewGame) -> GameState {
    let p = n.params;
    let mut g = GameState {
        version: VERSION,
        ..Default::default()
    };
    let mut refs = Refs::new(&g);
    g.climate = refs.id(&mut g, &p.climate_ref);
    g.start_condition = refs.id(&mut g, &p.start_condition_ref);
    g.terrain = refs.id(&mut g, &format!(r"Game\Terrain\{}.rsc:resource", p.terrain));
    g.seed = p.seed;
    g.size = p.size as i32;
    g.flags = n.flags;
    g.scenario = p.scenario.map_or(-1, |s| s as i32);
    g.town_name = p.town_name.clone();
    // The game's Random: its 624 words (the loop writes its own).
    g.random_state = vec![0; 0x270];
    g.buckets = vec![Vec::new(); 5];
    g.bucket_values = [QUOTAS, [0; 5]];
    g.camera = Some(camera([0.0, 0.0], 0.0, -40.0, 25.0));
    // The terrain block names its materials by reference (`Terrain\Terrain.rsc:resource`).
    let mut map = n.map;
    for m in [
        &mut map.terrain_material,
        &mut map.river_material,
        &mut map.lake_material,
    ]
    .into_iter()
    .flatten()
    {
        if !m.contains(':') {
            m.push_str(":resource");
        }
        let id = refs.id(&mut g, m);
        *m = g.references[&id].clone();
    }
    let size = map.dims[1];
    g.terrain_map = Some(map);
    g.map.size = size;
    g.map.cells = vec![
        GameMapCell {
            description: u32::MAX,
            ..Default::default()
        };
        (size * size).max(0) as usize
    ];
    g.achievements = default_block(&[ACHIEVEMENTS], 0);
    g.town = default_block(&[TOWN], 0);
    g.managers = default_block(&savegame::managers(VERSION), 0);
    let empty = |k: usize, f: &dyn Fn() -> Fields| Value::List((0..k).map(|_| f()).collect());
    g.managers.set(
        "environment.effects",
        empty(n.effects, &|| Fields(vec![("active", Value::Bool(false))])),
    );
    let t = &mut g.town;
    t.set(
        "town.history",
        empty(n.history, &|| {
            Fields(vec![
                ("count", Value::I32(0)),
                ("values", Value::U32s(Vec::new())),
            ])
        }),
    );
    t.set(
        "town.stock",
        empty(n.stock, &|| {
            Fields(vec![
                ("count", Value::I32(0)),
                ("previous", Value::I32(0)),
                ("limit", Value::I32(0)),
                ("used", Value::I32(0)),
                ("made", Value::I32(0)),
                ("cooldown", Value::I64(0)),
            ])
        }),
    );
    t.set(
        "town.capacity_records",
        empty(n.capacity, &|| {
            Fields(vec![
                ("used", Value::I32(0)),
                ("capacity", Value::I32(0)),
                ("cooldown", Value::I64(0)),
            ])
        }),
    );
    g
}

/// One achievement's instance to save: its reference and, per goal, the goal's reference, its
/// instance class, its value (`+0x10`: bit 0 met, bit 1 failed) and a TimeGoal's time.
pub struct AchievementSave<'a> {
    pub reference: &'a str,
    pub goals: Vec<(&'a str, &'static str, i32, i64)>,
}

/// The game's achievement instances (`AchievementManager::Persist`): one per achievement of
/// the list, in its order, each with its goals' instances (`GoalInstance`, `TimeGoalInstance`
/// or `FinishTutorialInstance`, whose tutorial is none outside the tutorials).
pub fn achievements(g: &mut GameState, list: &[AchievementSave]) {
    let mut refs = Refs::new(g);
    let mut instances = Vec::new();
    for a in list {
        let mut goals = Vec::new();
        for &(reference, class, value, time) in &a.goals {
            let goal_ref = refs.id(g, reference);
            let Some(id) = g.new_object(class) else {
                continue;
            };
            if let Some(o) = g.objects.get_mut(&id) {
                o.fields.set("goal.goal", Value::Ext(goal_ref));
                o.fields.set_i32("goal.value", value);
                if class == "TimeGoalInstance" {
                    o.fields.set_time("timer.time", time);
                }
            }
            goals.push(Fields(vec![("goal", Value::Ptr(Some(id)))]));
        }
        let achievement = refs.id(g, a.reference);
        let Some(id) = g.new_object("AchievementInstance") else {
            continue;
        };
        if let Some(o) = g.objects.get_mut(&id) {
            o.fields
                .set("instance.achievement", Value::Ext(achievement));
            o.fields.set("instance.goals.goals", Value::List(goals));
        }
        instances.push(Fields(vec![("instance", Value::Ptr(Some(id)))]));
    }
    g.achievements
        .set("achievements.instances", Value::List(instances));
}
