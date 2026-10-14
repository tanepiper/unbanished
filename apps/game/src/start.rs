//! New Game start-condition setup (`FUN_1400a87d0`): start point, profile structures and natural
//! resource seeding, run on the game's shared random stream. See docs/RESOURCE-SEEDING.md and
//! START-CONDITION-PROFILES.md. Households, deer, crops, orchards and livestock come next.

use std::collections::HashMap;

use banished_formats::game::{
    component_description, create_placed_description, growth_description, map_description,
    map_generator, model_description, natural_resource_description, spawn_description,
    MapGenerator,
};
use banished_formats::pkg::Archive;
use banished_game::map::Map;
use banished_game::random::Random;
use banished_game::seeding::{Cells, Placed, Seeder, Template};
use banished_game::terrain::HeightGrid;

use crate::models::{placement_matrix, SceneModels};

/// `Template\X.rsc:resource` (profile lists omit the object name).
fn normalise(reference: &str) -> String {
    let r = reference.replace('/', "\\");
    if r.contains(':') {
        r
    } else {
        format!("{r}:resource")
    }
}

fn read(archive: &Archive, reference: &str) -> Option<Vec<u8>> {
    archive.read_ref(&reference.replace('\\', "/")).ok()
}

/// Load a template and (recursively) the templates it spawns.
fn load_template(archive: &Archive, reference: &str, table: &mut Vec<Template>) -> Option<usize> {
    let key = normalise(reference);
    if let Some(i) = table.iter().position(|t| t.reference == key) {
        return Some(i);
    }
    let desc = component_description(&read(archive, &key)?)
        .map_err(|e| log::warn!("{key}: {e}"))
        .ok()?;
    let mut t = Template {
        reference: key.clone(),
        map: None,
        model: None,
        natural: None,
        growth: None,
        spawn: Vec::new(),
        size: None,
    };
    let mut spawn = Vec::new();
    for d in desc.descriptions.iter().flatten() {
        let Some(data) = read(archive, d) else {
            continue;
        };
        let object = d.rsplit(':').next().unwrap_or("");
        match object {
            "map" => t.map = map_description(&data).ok(),
            "model" => t.model = model_description(&data).ok(),
            "naturalresource" => t.natural = natural_resource_description(&data).ok(),
            "growth" => t.growth = growth_description(&data).ok(),
            "spawn" => spawn = spawn_description(&data).unwrap_or_default(),
            "createplaced" => {
                t.size = create_placed_description(&data)
                    .ok()
                    .map(|c| (c.width, c.height))
            }
            _ => {}
        }
    }
    let index = table.len();
    table.push(t);
    for e in spawn {
        if let Some(child) = e
            .template
            .as_deref()
            .and_then(|c| load_template(archive, c, table))
        {
            table[index].spawn.push((child, e.lo, e.hi));
        }
    }
    Some(index)
}

pub struct StartingTown {
    pub start: (i32, i32),
    pub templates: Vec<Template>,
    pub placed: Vec<Placed>,
}

impl StartingTown {
    /// `FUN_1400a87d0` up to the seeding pass. `rng` is the game stream (`Random(seed)`).
    pub fn create(
        archive: &Archive,
        profile_ref: &str,
        map_size: usize,
        terrain: &HeightGrid,
        rng: &mut Random,
    ) -> Option<StartingTown> {
        let profile: MapGenerator = map_generator(&read(archive, profile_ref)?)
            .map_err(|e| log::warn!("{profile_ref}: {e}"))
            .ok()?;
        let map = Map::new(map_size, terrain);
        let radius = profile.start_radius;
        let start = map.find_start(radius);
        let mut templates = Vec::new();
        let structures: Vec<_> = profile
            .structures
            .iter()
            .map(|s| {
                (
                    s.clone(),
                    load_template(archive, &s.template, &mut templates),
                )
            })
            .collect();
        let list: Vec<usize> = profile
            .natural_resources
            .iter()
            .filter_map(|r| load_template(archive, r, &mut templates))
            .collect();
        let mut seeder = Seeder {
            templates: &templates,
            terrain,
            cells: Cells::new(map_size, map.types),
            rng,
            placed: Vec::new(),
        };
        seeder.place_structures(&structures, start.0, start.1);
        seeder.seed(&list, start.0, start.1, radius * radius);
        let placed = std::mem::take(&mut seeder.placed);
        log::info!(
            "start {:?}: {} natural resources placed",
            start,
            placed.len()
        );
        Some(StartingTown {
            start,
            templates,
            placed,
        })
    }

    /// Model instances of every placed entity, grouped by mesh.
    pub fn models(&self, archive: &Archive) -> SceneModels {
        let mut groups: HashMap<String, Vec<[f32; 16]>> = HashMap::new();
        for p in &self.placed {
            let Some(model) = &self.templates[p.template].model else {
                continue;
            };
            let Some(Some(mesh)) = model
                .variants
                .get(p.variant)
                .and_then(|v| v.meshes.get(p.mesh))
            else {
                continue;
            };
            groups
                .entry(mesh.clone())
                .or_default()
                .push(placement_matrix(
                    p.position,
                    [p.scale[0], p.scale[1], p.scale[2]],
                    p.yaw,
                ));
        }
        let mut list: Vec<_> = groups.into_iter().map(|(m, i)| (m, 0, i)).collect();
        list.sort_by(|a, b| a.0.cmp(&b.0));
        SceneModels::from_instances(archive, list)
    }
}
