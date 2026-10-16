//! A loaded save (`.sav`): the decoded game state (`banished_formats::savegame`) and the lookups
//! the gameplay loop builds its world and town from. See docs/SAVE-LOADING.md.

use banished_formats::save::{Save, SaveMetadata};
use banished_formats::savegame::{Fields, GameState, ObjId, Object, Value};
use banished_formats::Result;

use crate::world::WorldParams;

pub struct SavedGame {
    pub state: GameState,
    pub metadata: SaveMetadata,
}

/// `Game\Climate\Fair.rsc:resource` → `Fair`.
fn stem(path: &str) -> String {
    let file = path.rsplit(['\\', '/']).next().unwrap_or(path);
    file.split('.').next().unwrap_or(file).to_string()
}

impl SavedGame {
    /// The web build has no load path yet (no Load dialog, no file access).
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    pub fn parse(bytes: &[u8]) -> Result<SavedGame> {
        let save = Save::parse(bytes)?;
        Ok(SavedGame {
            state: GameState::decode(&save.state)?,
            metadata: save.metadata()?,
        })
    }

    /// An external reference id's path (`u32::MAX` is none).
    pub fn reference(&self, id: u32) -> Option<&str> {
        self.state.references.get(&id).map(String::as_str)
    }

    /// An `Ext` field's path.
    pub fn ext(&self, fields: &Fields, path: &str) -> Option<&str> {
        match fields.at(path)? {
            Value::Ext(id) => self.reference(*id),
            _ => None,
        }
    }

    /// The New Game settings the save was started with (`FUN_14009af60`'s loop fields).
    pub fn params(&self) -> WorldParams {
        let g = &self.state;
        let path = |id| self.reference(id).unwrap_or_default().to_string();
        WorldParams {
            town_name: g.town_name.clone(),
            terrain: stem(&path(g.terrain)),
            climate: stem(&path(g.climate)),
            climate_ref: path(g.climate),
            start_condition: stem(&path(g.start_condition)),
            start_condition_ref: path(g.start_condition),
            size: g.size.max(0) as usize,
            seed: g.seed,
            // `Game +0xd0`: 0 with disasters, `0xf0` without (`FUN_14009d530`).
            disasters: g.flags & 0xf0 == 0,
            scenario: (g.scenario >= 0).then_some(g.scenario as usize),
        }
    }

    /// The stock manager's slots (`+0x170`) and capacity records (`+0x180`) as saved in the
    /// town manager's block: counts, previous counts, limits, cooldowns.
    pub fn stock_warnings(&self) -> banished_game::events::StockWarnings {
        let mut w = banished_game::events::StockWarnings::default();
        let town = &self.state.town;
        if let Some(Value::List(slots)) = town.at("town.stock") {
            for (i, s) in slots.iter().enumerate().take(w.count.len()) {
                w.count[i] = s.i32("count").unwrap_or(0);
                w.previous[i] = s.i32("previous").unwrap_or(0);
                w.limits[i] = s.i32("limit").unwrap_or(w.limits[i]);
                if let Some(Value::I64(t)) = s.get("cooldown") {
                    w.cooldown[i] = *t;
                }
            }
        }
        if let Some(Value::List(records)) = town.at("town.capacity_records") {
            for (r, s) in w.capacity.iter_mut().zip(records) {
                let cooldown = match s.get("cooldown") {
                    Some(Value::I64(t)) => *t,
                    _ => 0,
                };
                *r = (
                    s.i32("used").unwrap_or(0),
                    s.i32("capacity").unwrap_or(0),
                    cooldown,
                );
            }
        }
        w
    }

    /// The stock slots' used and made counters since the last statistics sample.
    pub fn stock_counters(&self) -> banished_game::tracker::StockCounters {
        let mut c = banished_game::tracker::StockCounters::default();
        if let Some(Value::List(slots)) = self.state.town.at("town.stock") {
            for (i, s) in slots.iter().enumerate().take(c.used.len()) {
                c.used[i] = s.i32("used").unwrap_or(0);
                c.made[i] = s.i32("made").unwrap_or(0);
            }
        }
        c
    }

    /// The environment controller's saved state onto `env` (`FUN_140099160`).
    pub fn restore_environment(&self, env: &mut banished_game::environment::Environment) {
        let m = &self.state.managers;
        let f = |k: &str| m.f32(&format!("environment.{k}")).unwrap_or(0.0);
        let i = |k: &str| m.i32(&format!("environment.{k}")).unwrap_or(0);
        let season = match m.at("environment.season") {
            Some(Value::F32s(v)) if v.len() == 4 => banished_game::environment::SeasonValues {
                snow: v[0],
                autumn: v[1],
                leaf_loss: v[2],
                rain: v[3],
            },
            _ => Default::default(),
        };
        let mut effects = [None; 2];
        if let Some(Value::List(items)) = m.at("environment.effects") {
            for (slot, e) in effects.iter_mut().zip(items) {
                if matches!(e.get("active"), Some(Value::Bool(true))) {
                    *slot = Some(banished_game::environment::WeatherEffect {
                        intensity: e.f32("intensity").unwrap_or(0.0),
                        direction: e.f32("direction").unwrap_or(0.0),
                    });
                }
            }
        }
        env.restore(
            i("state"),
            i("level"),
            f("temperature"),
            season,
            (f("baseline"), f("baseline_target")),
            (f("bias"), f("bias_target")),
            i("slice"),
            effects,
        );
    }

    /// The event log (`FUN_14009a790`): the notify flags and every entry with its text.
    pub fn event_log(&self) -> banished_game::events::EventLog {
        let m = &self.state.managers;
        let mut log = banished_game::events::EventLog::default();
        for (k, n) in log.notify.iter_mut().enumerate() {
            if let Some(Value::Bool(b)) = m.at(&format!("event_log.notify_{k}")) {
                *n = *b;
            }
        }
        if let Some(Value::List(entries)) = m.at("event_log.entries") {
            log.entries = entries
                .iter()
                .map(|e| banished_game::events::EventEntry {
                    kind: e.i32("kind").unwrap_or(0),
                    severity: e.i32("severity").unwrap_or(0),
                    color: e.i32("color").unwrap_or(0) as u32,
                    month: 0,
                    year: 0,
                    args: Default::default(),
                    cell: e.i32("cell").unwrap_or(-1),
                    category: e.i32("category").unwrap_or(0),
                    icon: e.i32("icon").unwrap_or(-1),
                    text: match e.get("text") {
                        Some(Value::Str(t)) => Some(t.clone()),
                        _ => None,
                    },
                })
                .collect();
        }
        log
    }

    /// The Town Info history (StockManager `+0x150`) and the time to the next sample
    /// (`+0x198`).
    pub fn statistics(&self) -> banished_game::tracker::Statistics {
        let town = &self.state.town;
        let mut s = banished_game::tracker::Statistics::default();
        if let Some(Value::List(series)) = town.at("town.history") {
            for (dst, src) in s.series.iter_mut().zip(series) {
                if let Some(Value::U32s(v)) = src.get("values") {
                    *dst = v.iter().map(|&x| x as i32).collect();
                }
            }
        }
        if let Some(Value::I64(t)) = town.at("town.time_198") {
            s.timer = *t;
        }
        s
    }

    pub fn object(&self, id: ObjId) -> Option<&Object> {
        self.state.objects.get(&id)
    }

    /// The live entities in the entity manager's order.
    pub fn entities(&self) -> impl Iterator<Item = (ObjId, &Object)> {
        self.state
            .entities
            .iter()
            .flatten()
            .filter_map(|&e| Some((e, self.object(e)?)))
    }

    /// An entity's components, in its template's order.
    pub fn components(&self, entity: &Object) -> Vec<&Object> {
        match entity.fields.at("entity.components") {
            Some(Value::List(items)) => items
                .iter()
                .filter_map(|c| self.object(c.ptr("component")?))
                .collect(),
            _ => Vec::new(),
        }
    }

    /// An entity's component of one type.
    pub fn component<'a>(&'a self, entity: &'a Object, type_name: &str) -> Option<&'a Object> {
        self.components(entity)
            .into_iter()
            .find(|c| c.type_name == type_name)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn stems() {
        assert_eq!(super::stem(r"Game\Climate\Fair.rsc:resource"), "Fair");
        assert_eq!(super::stem("Game/Terrain/Valleys.rsc"), "Valleys");
    }
}
