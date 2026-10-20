//! The settings of a new game: the town name, terrain, climate, start, map size, seed,
//! disasters and scenario (the New Game dialog's choices).

pub struct WorldParams {
    pub town_name: String,
    pub terrain: String,
    pub climate: String,
    /// The selected `ClimateGenerator` (`Game\Climate\Fair.rsc:resource`).
    pub climate_ref: String,
    pub start_condition: String,
    /// The selected `MapGenerator` (`Game\\StartCondition\\Medium.rsc:resource`).
    pub start_condition_ref: String,
    /// 0 Small, 1 Medium, 2 Large.
    pub size: usize,
    pub seed: u32,
    // Game +0xd0
    /// The Disasters combo's first item (on): `Game` starts 0, else `0xf0`.
    ///
    /// Original: `FUN_14009d530`.
    pub disasters: bool,
    /// The scenario played (an index into `Game\Scenario\ScenarioList.rsc`).
    ///
    /// Original: `FUN_1400b9a10`.
    pub scenario: Option<usize>,
}

impl Default for WorldParams {
    fn default() -> Self {
        WorldParams {
            town_name: "New Settlement".into(),
            terrain: "Valleys".into(),
            climate: "Fair".into(),
            climate_ref: r"Game\Climate\Fair.rsc:resource".into(),
            start_condition: "Medium".into(),
            start_condition_ref: String::new(),
            size: 0,
            seed: 290,
            disasters: true,
            scenario: None,
        }
    }
}
