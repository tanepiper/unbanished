//! Global game data loaded once at startup (`FUN_14009f280`, loader loop "Game/Global"): the New
//! Game selector lists and the profiles they reference.

use banished_formats::game::{
    climate_header, external_list, map_generator, terrain_generator, ClimateHeader, MapGenerator,
    TerrainGenerator,
};
use banished_formats::pkg::Archive;
use banished_formats::Result;

pub struct Choice<T> {
    /// Reference as stored in the list (e.g. `Game\Terrain\Valleys.rsc:resource`).
    pub reference: String,
    /// The reference's file stem (`Valleys`).
    pub name: String,
    /// String table and key of the display name (`UI::String(+0x20, +0x28)`).
    pub label: (Option<String>, String),
    /// Shown in the New Game selector (`+0x38`).
    pub listed: bool,
    pub data: T,
}

pub struct GlobalData {
    pub climates: Vec<Choice<ClimateHeader>>,
    pub start_conditions: Vec<Choice<MapGenerator>>,
    pub terrains: Vec<Choice<TerrainGenerator>>,
}

fn stem(reference: &str) -> String {
    let path = reference.split(':').next().unwrap_or(reference);
    let file = path.rsplit(['\\', '/']).next().unwrap_or(path);
    file.trim_end_matches(".rsc").to_string()
}

fn load_list<T>(
    archive: &Archive,
    list: &str,
    parse: impl Fn(&[u8]) -> Result<T>,
    label: impl Fn(&T) -> ((Option<String>, String), bool),
) -> Result<Vec<Choice<T>>> {
    external_list(&archive.read_ref(list)?)?
        .into_iter()
        .map(|reference| {
            let data = parse(&archive.read_ref(&reference)?)?;
            let (label, listed) = label(&data);
            Ok(Choice {
                name: stem(&reference),
                reference,
                label,
                listed,
                data,
            })
        })
        .collect()
}

impl GlobalData {
    pub fn load(archive: &Archive) -> Result<GlobalData> {
        Ok(GlobalData {
            climates: load_list(
                archive,
                "Game/Climate/ClimateList.rsc:resource",
                climate_header,
                |c| ((c.string_table.clone(), c.name.clone()), c.listed),
            )?,
            start_conditions: load_list(
                archive,
                "Game/StartCondition/StartConditions.rsc:resource",
                map_generator,
                |g| ((g.list.clone(), g.name.clone()), g.flag),
            )?,
            terrains: load_list(
                archive,
                "Game/Terrain/TerrainList.rsc:resource",
                terrain_generator,
                |t| ((t.string_table.clone(), t.name.clone()), t.flag),
            )?,
        })
    }
}
