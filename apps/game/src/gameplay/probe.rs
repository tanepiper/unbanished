//! World queries for tests and scripted runs (not in the original): the town's numbers, its
//! buildings, citizens, animals and stock, as JSON. They only read the town; nothing here
//! changes the game. See [`crate::probe`] for the command language.

use super::GameplayLoop;
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

/// The Status Bar's stock slots, in order (`unbanished_game::work::stock::STOCK_SLOTS`; the last
/// ten are the custom slots that mods may use).
const STOCK: [&str; 11] = [
    "wood", "stone", "iron", "firewood", "tools", "food", "herbs", "clothes", "alcohol", "coal",
    "textile",
];

/// The last part of a resource reference, without its folder or `:object`:
/// `Template\Buildings\WoodHouse.rsc:WoodHouse` → `WoodHouse`.
fn short(reference: &str) -> String {
    let file = reference.split(':').next().unwrap_or(reference);
    let name = file.rsplit(['\\', '/']).next().unwrap_or(file);
    name.trim_end_matches(".rsc").to_string()
}

/// Counts by name, kept in name order so that the JSON is the same every time.
fn counts(names: impl Iterator<Item = String>, filter: Option<&str>) -> Value {
    let mut map: BTreeMap<String, u32> = BTreeMap::new();
    for n in names {
        if filter.is_none_or(|f| n.to_ascii_lowercase().contains(&f.to_ascii_lowercase())) {
            *map.entry(n).or_default() += 1;
        }
    }
    json!({ "total": map.values().sum::<u32>(), "by_name": map })
}

impl GameplayLoop {
    /// Answer a world query: `town`, `buildings [FILTER]`, `citizens [FILTER]`,
    /// `animals [FILTER]` or `stock`. `None` when `what` is not a world query.
    pub(crate) fn world_query(
        &self,
        what: &str,
        filter: Option<&str>,
    ) -> Option<Result<Value, String>> {
        let Some(town) = self.town.as_ref() else {
            return matches!(
                what,
                "town" | "buildings" | "citizens" | "animals" | "stock"
            )
            .then(|| Err("no town is loaded".into()));
        };
        let v = self.status_values();
        let value = match what {
            "town" => json!({
                "name": v.town_name,
                "year": v.year,
                "month": v.month,
                "paused": self.paused,
                "population": v.adults + v.students + v.children,
                "adults": v.adults,
                "students": v.students,
                "children": v.children,
                "health": v.health,
                "happiness": v.happiness,
                "food": v.stock[5],
                "firewood": v.stock[3],
                "buildings": town.construction.sites.values()
                    .filter(|s| !s.destroyed && town.placed[s.entity].alive).count(),
                "animals": town.wildlife.animals.len(),
            }),
            "stock" => {
                let mut m = Map::new();
                for (name, n) in STOCK.iter().zip(v.stock) {
                    m.insert(name.to_string(), json!(n));
                }
                Value::Object(m)
            }
            "buildings" => {
                let sites = town
                    .construction
                    .sites
                    .values()
                    .filter(|s| !s.destroyed && town.placed[s.entity].alive);
                let (built, building): (Vec<_>, Vec<_>) = sites.partition(|s| s.complete);
                let name = |s: &&unbanished_game::work::construction::Site| {
                    short(&town.templates[town.placed[s.entity].template].reference)
                };
                json!({
                    "built": counts(built.iter().map(name), filter),
                    "under_construction": counts(building.iter().map(name), filter),
                })
            }
            "citizens" => {
                let professions = &town.construction.professions.list;
                let alive = || {
                    town.citizens
                        .iter()
                        .zip(&town.agents.states)
                        .filter(|(c, _)| c.alive)
                };
                let working = alive()
                    .filter(|(c, _)| c.is_adult(&town.rules) && c.school.is_none())
                    .map(|(_, s)| {
                        professions.get(s.profession).map_or("none".into(), |p| {
                            p.reference.rsplit(':').next().unwrap_or("none").to_string()
                        })
                    });
                json!({
                    "adults": v.adults,
                    "students": v.students,
                    "children": v.children,
                    "by_profession": counts(working, filter)["by_name"].clone(),
                })
            }
            "animals" => counts(
                town.wildlife
                    .animals
                    .values()
                    .map(|a| short(&town.templates[a.template].reference)),
                filter,
            ),
            _ => return None,
        };
        Some(Ok(value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_names_drop_folders_and_objects() {
        assert_eq!(
            short(r"Template\Buildings\WoodHouse.rsc:WoodHouse"),
            "WoodHouse"
        );
        assert_eq!(short("Game/Profession/Profession.rsc:Farmer"), "Profession");
        assert_eq!(short("Deer"), "Deer");
    }

    #[test]
    fn counts_are_sorted_and_filtered() {
        let names = ["b", "a", "b", "c"].into_iter().map(String::from);
        assert_eq!(
            counts(names, None),
            json!({ "total": 4, "by_name": { "a": 1, "b": 2, "c": 1 } })
        );
        let names = ["Deer", "Bear", "deer"].into_iter().map(String::from);
        assert_eq!(counts(names, Some("DEER"))["total"], json!(2));
    }
}
