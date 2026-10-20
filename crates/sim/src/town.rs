//! New Game start-condition setup: start point, profile structures and natural resource seeding,
//! run on the game's shared random stream. See docs/RESOURCE-SEEDING.md and
//! START-CONDITION-PROFILES.md. Starting households and scheduled natural resources are active;
//! deer, crops, orchards and livestock come next.
//!
//! This file holds [`StartingTown`], creating and assembling it, and the small helpers other
//! modules use (`read`, `normalise`, `material_index`). The rest is split by topic:
//! - `templates`, `animals`, `citizens`, `professions`: what the game data describes;
//! - `placing`: placing structures and fenced areas on the map;
//! - `registry`: registering entities, ambience and markers;
//! - `frame`, `smooth`: the town's frame update and the phase after each iteration;
//! - `sites`: building sites;
//! - `minimap`: the minimap's overlay;
//! - [`map`]: the generated or saved map, with no drawing data;
//! - [`load`], [`save`], `bridge`, `tornado`: loading and saving games, bridges, tornadoes.
//!
//! The models and icons to draw are not here: the game app's `view::town` makes them.
//!
//! Original: `FUN_1400a87d0`.

mod animals;
mod citizens;
mod frame;
mod minimap;
mod placing;
mod professions;
mod registry;
mod sites;
mod smooth;
mod templates;
use animals::{livestock_kind, wildlife_kinds};
use citizens::{citizen_animations, citizen_rules, grid_start, households, starting_goods};
pub use citizens::{citizen_tool_nodes, CHILD_SCALE};
use templates::{load_template, professions};

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use unbanished_formats::game::{
    citizen_description, component_description, create_placed_description, growth_description,
    map_description, map_generator, model_description, natural_resource_description,
    particle_description, radius_description, raw_material_description, spawn_description,
    storage_description, storage_location_description, MapGenerator,
};
use unbanished_formats::names::NameDatabase;
use unbanished_formats::pkg::Archive;
use unbanished_game::behaviour::agents::{Agents, CitizenRulesAi};
use unbanished_game::behaviour::ai::Thresholds;
use unbanished_game::nature::seeding::{Cells, Placed, ScheduledComponent, Seeder, Template};
use unbanished_game::people::citizens::{Citizen, CitizenRules, Households};
use unbanished_game::random::Random;
use unbanished_game::scheduler::EntityScheduler;
use unbanished_game::terrain::HeightGrid;
use unbanished_game::work::stock::{distribute, RawMaterial, Storage, StorageSite};
use unbanished_game::world::map::Map;

use unbanished_game::work::construction::{Construction, Controller, Site, SiteTemplate};
use unbanished_game::work::professions::{Profession, Professions, WorkPlace};

mod bridge;
pub mod load;
pub mod map;
// The web build has no save path yet (no Save dialog, no file access).
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
pub mod save;
mod tornado;
pub mod world_params;
pub use bridge::BridgeSpec;

/// The raw material for `reference`, loading it into the town's list when new.
pub fn material_index(
    archive: &Archive,
    materials: &mut Vec<RawMaterial>,
    reference: &str,
) -> Option<usize> {
    let key = normalise(reference);
    if let Some(i) = materials.iter().position(|m| m.reference == key) {
        return Some(i);
    }
    let d = read(archive, &key.replace(":resource", ":rawmaterial"))
        .and_then(|d| raw_material_description(&d).ok())?;
    materials.push(RawMaterial {
        reference: key,
        size: d.size,
        flags: d.flags,
        tool_uses: d.value_54,
        preferred: d.flag_58,
        pile_spacing: d.value_4c,
        pile_amount: d.value_40,
    });
    Some(materials.len() - 1)
}

pub fn normalise(reference: &str) -> String {
    let r = reference.replace('/', "\\");
    if r.contains(':') {
        r
    } else {
        format!("{r}:resource")
    }
}

pub fn read(archive: &Archive, reference: &str) -> Option<Vec<u8>> {
    archive.read_ref(&reference.replace('\\', "/")).ok()
}

/// A template's component descriptions by their key (`X.rsc:key`).
fn description_refs(archive: &Archive, reference: &str) -> Vec<(String, String)> {
    let key = normalise(reference);
    read(archive, &key)
        .and_then(|d| component_description(&d).ok())
        .map(|desc| {
            desc.descriptions
                .iter()
                .flatten()
                .map(|d| (d.rsplit(':').next().unwrap_or("").to_string(), d.clone()))
                .collect()
        })
        .unwrap_or_default()
}

/// What [`StartingTown::assemble`] builds a town from.
struct TownParts {
    start: (i32, i32),
    citizens: Vec<Citizen>,
    rules: CitizenRules,
    materials: Vec<RawMaterial>,
    storages: Vec<Storage>,
    storage_sites: Vec<StorageSite>,
    templates: Vec<Template>,
    placed: Vec<Placed>,
    cells: Cells,
    scheduler: EntityScheduler<usize>,
    removed: Vec<usize>,
    animation_events: Vec<(usize, String)>,
    wildlife: unbanished_game::nature::wildlife::Wildlife,
    known: Vec<usize>,
    known_materials: Vec<usize>,
    water: unbanished_game::disasters::fire::Water,
}

pub struct StartingTown {
    /// The citizen template (`Template\\Citizen…:resource`), as the town was made with it; the
    /// view reads its models and icons from it.
    pub citizen: String,
    /// The laborer, child and student professions (`start/professions.rs`).
    pub archive_professions: Vec<(String, unbanished_formats::game::Profession)>,
    pub start: (i32, i32),
    pub citizens: Vec<Citizen>,
    pub rules: CitizenRules,
    pub materials: Vec<RawMaterial>,
    pub storages: Vec<Storage>,
    pub storage_sites: Vec<StorageSite>,
    pub templates: Vec<Template>,
    pub placed: Vec<Placed>,
    pub cells: Cells,
    pub scheduler: EntityScheduler<usize>,
    /// The last bucket pass's visits (`EntityScheduler::take_visits`), for the agents.
    pub visits: Vec<(usize, i64)>,
    /// Each citizen's entity in the scheduler (bucket 1); `None` once dead.
    ///
    /// Original: `FUN_1400c9d70`.
    pub citizen_handles: Vec<Option<usize>>,
    /// The map ambience's region entities in the scheduler (bucket 2) for a new game, in
    /// region order (a saved game's are among `SaveIds::foreign`).
    pub ambience_handles: Vec<usize>,
    pub removed: Vec<usize>,
    pub animation_events: Vec<(usize, String)>,
    pub citizen_model: Option<unbanished_formats::game::ModelDescription>,
    /// The nomad templates' NomadDescription materials (men, women).
    pub nomad_materials: HashMap<usize, [Option<String>; 2]>,
    /// Each variant's animation group (`Models\Citizen\CitizenAnims.rsc`).
    pub citizen_anims: Vec<Option<Arc<unbanished_formats::types::AnimationGroup>>>,
    /// The citizens' CharacterComponent and CitizenAIComponent (docs/CITIZEN-AI.md).
    pub agents: Agents,
    /// Wild animals, their herds and populations (docs/WILDLIFE-POPULATION.md).
    pub wildlife: unbanished_game::nature::wildlife::Wildlife,
    // PlacementTool +0x68
    /// The placement tool's ghost (`PlacementTool`): mesh and transform.
    pub ghost: Option<(String, [f32; 16])>,
    /// The job manager, professions, workplaces and construction sites (docs/JOBS.md).
    pub construction: Construction,
    /// `Game/Names/Male` and `Female`, for births.
    ///
    /// Original: `FUN_1400aa130`.
    pub names: Option<(NameDatabase, NameDatabase)>,
    /// Each StorageDisplay's cells on the ground, by rows (presentation only).
    pub display_cells: BTreeMap<usize, Vec<[f32; 3]>>,
    /// For a town loaded from a save: the saved object each entity and citizen came from, so
    /// saving writes the town back into them (docs/SAVE-WRITING.md).
    pub save_ids: Option<SaveIds>,
}

/// The saved objects behind a loaded town: per placed entity its ComponentEntity, per citizen
/// its CitizenComponent.
#[derive(Debug, Clone, Default)]
pub struct SaveIds {
    pub entities: Vec<Option<unbanished_formats::savegame::ObjId>>,
    pub citizens: Vec<unbanished_formats::savegame::ObjId>,
    /// Saved entities the town does not model (the map ambience, skipped templates): their
    /// scheduler handle and ComponentEntity.
    pub foreign: Vec<(usize, unbanished_formats::savegame::ObjId)>,
}

impl StartingTown {
    /// A New Game's town, up to the seeding pass. `rng` is the game stream (`Random(seed)`).
    ///
    /// Original: `FUN_1400a87d0`.
    pub fn create(
        archive: &Archive,
        profile_ref: &str,
        map_size: usize,
        terrain: &HeightGrid,
        rng: &mut Random,
    ) -> Option<StartingTown> {
        // A new game: no shared animation instances (`AnimationManager`).
        unbanished_game::behaviour::animation::shared::reset();
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
        let mut cells = Cells::new(map_size, map.types);
        // The map's setup files the shore cells as water sources (`FUN_1400a43f0`).
        let water = unbanished_game::disasters::fire::Water::new(&cells);
        let mut placed = Vec::new();
        let mut removed = Vec::new();
        let mut animation_events = Vec::new();
        let mut seeder = Seeder {
            templates: &templates,
            terrain,
            cells: &mut cells,
            rng,
            placed: &mut placed,
            removed: &mut removed,
            animation_events: &mut animation_events,
        };
        seeder.place_structures(&structures, start.0, start.1);
        let structure_count = seeder.placed.len();
        seeder.seed(&list, start.0, start.1, radius * radius);
        drop(seeder);
        let mut scheduler = EntityScheduler::default();
        for (id, p) in placed.iter().enumerate() {
            scheduler.register(id, p.bucket);
        }
        // `FUN_1400a62a0`: the starting households around the start cell centre.
        let (citizens, rules) = households(
            archive,
            &profile,
            &grid_start(map_size, terrain, start),
            rng,
        );
        // `FUN_1400a5eb0`: the wildlife populations (deer) and their herds.
        let mut wildlife = unbanished_game::nature::wildlife::Wildlife::default();
        // The households' CharacterComponents activate as they are made, before any animal
        // (`FUN_14010ee90`): first in the characters' tree.
        let radius = read(
            archive,
            &normalise(&profile.citizen).replace(":resource", ":character"),
        )
        .and_then(|d| unbanished_formats::game::character_description(&d).ok())
        .unwrap_or_default()
        .radius;
        for (i, c) in citizens.iter().enumerate() {
            wildlife.tree.insert(
                unbanished_game::behaviour::character_tree::Who::Citizen(i),
                c.position,
                radius,
            );
        }
        let populations =
            wildlife_kinds(archive, &profile.populations, &mut templates, &mut wildlife);
        for t in populations {
            unbanished_game::nature::wildlife::Wild {
                wildlife: &mut wildlife,
                placed: &mut placed,
                templates: &templates,
                cells: &cells,
                terrain,
                rng,
                scheduler: &mut scheduler,
            }
            .start_population(t);
        }
        let (materials, storages, storage_sites) =
            starting_goods(archive, &profile, &placed, &templates);
        // `FUN_140097330`: the profile's raw materials are the first known.
        let known_materials: Vec<usize> = (0..materials.len()).collect();
        // `FUN_1400a87d0`'s end: random crops, orchard trees and livestock become known
        // (`FUN_140096ef0`, once each).
        let mut known = Vec::new();
        for (list, picks) in [
            (&profile.crops, profile.crop_picks),
            (&profile.orchard_trees, profile.tree_picks),
        ] {
            for _ in 0..picks {
                let r = rng.rand_below(list.len() as u32) as usize;
                if let Some(t) = list
                    .get(r)
                    .and_then(|t| load_template(archive, t, &mut templates))
                {
                    if !known.contains(&t) {
                        known.push(t);
                    }
                }
            }
        }
        // Every livestock kind can be kept; the picks bring their starting animals
        // (`FUN_1400a5fe0`) and become known.
        for t in &profile.livestock {
            livestock_kind(archive, t, &mut templates, &mut wildlife);
        }
        let at = grid_start(map_size, terrain, start);
        let [dx, dy, spread] = profile.livestock_floats;
        for _ in 0..profile.livestock_picks {
            let r = rng.rand_below(profile.livestock.len() as u32) as usize;
            if let Some(t) = profile
                .livestock
                .get(r)
                .and_then(|t| livestock_kind(archive, t, &mut templates, &mut wildlife))
            {
                unbanished_game::nature::wildlife::Wild {
                    wildlife: &mut wildlife,
                    placed: &mut placed,
                    templates: &templates,
                    cells: &cells,
                    terrain,
                    rng,
                    scheduler: &mut scheduler,
                }
                .start_livestock(
                    t,
                    profile.ints[0],
                    [at[0] + dx, at[1] + dy, at[2]],
                    spread,
                );
                if !known.contains(&t) {
                    known.push(t);
                }
            }
        }
        log::info!(
            "start {:?}: {} natural resources placed",
            start,
            placed.len()
        );
        let mut town = Self::assemble(
            archive,
            &profile.citizen,
            TownParts {
                start,
                citizens,
                rules,
                materials,
                storages,
                storage_sites,
                templates,
                placed,
                cells,
                scheduler,
                removed,
                animation_events,
                wildlife,
                known,
                known_materials,
                water,
            },
        );
        town.build_structures(archive, structure_count);
        town.place_displays(terrain);
        Some(town)
    }

    /// The setup New Game and loading share: the citizen description's rules, models and
    /// tools, the professions, the name databases and the agents.
    fn assemble(archive: &Archive, citizen: &str, parts: TownParts) -> StartingTown {
        let TownParts {
            start,
            citizens,
            rules,
            materials,
            storages,
            storage_sites,
            templates,
            placed,
            cells,
            scheduler,
            removed,
            animation_events,
            wildlife,
            known,
            known_materials,
            water,
        } = parts;
        let citizen_model = read(archive, r"Template\Citizen.rsc:model")
            .and_then(|d| unbanished_formats::game::model_description(&d).ok());
        let citizen_ref = normalise(citizen);
        let character = read(archive, &citizen_ref.replace(":resource", ":character"))
            .and_then(|d| unbanished_formats::game::character_description(&d).ok())
            .unwrap_or_default();
        let ai_rules = read(archive, &citizen_ref.replace(":resource", ":citizen"))
            .and_then(|d| citizen_description(&d).ok())
            .map(|c| CitizenRulesAi {
                thresholds: Thresholds {
                    adult_age: c.f(0x44),
                    health_low: c.f(0x74),
                    happy_low: c.f(0xb4),
                    happy_unhappy: c.f(0xb8),
                    hungry: c.f(0xc8),
                    hungry_urgent: c.f(0xcc),
                    cold: c.f(0xe8),
                    cold_urgent: c.f(0xec),
                },
                health: c.f(0x6c),
                happiness: c.f(0xa4),
                loiter_range: c.i(0xbc),
                clothing_duration: c.f(0x130) * 300.0,
                tool_grade: c.i(0x110),
                work_near: c.i(0xfc),
                work_far: c.i(0x100),
                work_minutes: c.i(0x104),
                work_ok: c.i(0x108),
                work_cold: c.f(0x10c),
                no_tool_factor: c.f(0x114),
                education: c.i(0x118),
                water_distance: c.i(0xf8),
                health_max: c.f(0x70),
                hunger_limit: c.f(0xd0),
                cold_limit: c.f(0xf0),
                profession_allows_idle: crate::mods::profession_list(archive)
                    .first()
                    .cloned()
                    .and_then(|r| read(archive, &r))
                    .and_then(|d| unbanished_formats::game::profession(&d).ok())
                    .is_some_and(|p| p.flag_4c),
            })
            .unwrap_or_default();
        let carried = read(archive, &citizen_ref.replace(":resource", ":storage"))
            .and_then(|d| storage_description(&d).ok())
            .map(|d| Storage {
                capacity: d.capacity,
                accepts: d.accepts,
                items: Vec::new(),
            })
            .unwrap_or_default();
        let mut agents = Agents::new(&citizens, &cells, character, ai_rules, carried, |c| {
            if c.is_adult(&rules) {
                1.0
            } else {
                CHILD_SCALE
            }
        });
        // The citizens already in the characters' tree keep their slots.
        for i in 0..citizens.len() {
            if let Some(s) = wildlife
                .tree
                .slot_of(unbanished_game::behaviour::character_tree::Who::Citizen(i))
            {
                agents.set_tree_slot(i, s);
            }
        }
        if let Some(d) = read(archive, &citizen_ref.replace(":resource", ":citizen"))
            .and_then(|d| citizen_description(&d).ok())
        {
            agents.life = unbanished_game::people::life::LifeRules::new(&d);
        }
        agents.diseases = read(archive, "Game/Disease/Diseases.rsc:resource")
            .and_then(|d| unbanished_formats::game::external_list(&d).ok())
            .unwrap_or_default()
            .iter()
            .filter_map(|r| read(archive, &normalise(r)))
            .filter_map(|d| unbanished_formats::game::disease(&d).ok())
            .collect();
        agents.carry = read(archive, "Game/Profession/Profession.rsc:laborer")
            .and_then(|d| unbanished_formats::game::profession(&d).ok())
            .map_or(0, |p| p.value_50);
        let names = ["Male", "Female"].map(|n| {
            archive
                .read(&format!("Game/Names/{n}_resource.crs"))
                .ok()
                .and_then(|d| NameDatabase::read(&d).ok())
        });
        let names = match names {
            [Some(m), Some(f)] => Some((m, f)),
            _ => None,
        };
        let archive_professions = ["laborer", "child", "student"]
            .iter()
            .filter_map(|k| {
                read(archive, &format!("Game/Profession/Profession.rsc:{k}"))
                    .and_then(|d| unbanished_formats::game::profession(&d).ok())
                    .map(|p| (k.to_string(), p))
            })
            .collect();
        let mut town = StartingTown {
            citizen: citizen.to_string(),
            archive_professions,
            start,
            citizens,
            rules,
            materials,
            storages,
            storage_sites,
            templates,
            placed,
            cells,
            scheduler,
            visits: Vec::new(),
            citizen_handles: Vec::new(),
            ambience_handles: Vec::new(),
            removed,
            animation_events,
            citizen_anims: citizen_model
                .as_ref()
                .map(|m| citizen_animations(archive, m))
                .unwrap_or_default(),
            citizen_model,
            nomad_materials: HashMap::new(),
            agents,
            wildlife,
            ghost: None,
            construction: Construction {
                professions: professions(archive),
                known,
                known_materials,
                water,
                names: names.clone().map(std::sync::Arc::new),
                ..Default::default()
            },
            names,
            display_cells: BTreeMap::new(),
            save_ids: None,
        };
        town.resolve_piles(archive);
        town.schedule_citizens();
        // CitizenDescription `+0x20/+0x28/+0x30`: the teacher's, priest's and doctor's
        // professions.
        if let Some(d) = read(archive, &citizen_ref.replace(":resource", ":citizen"))
            .and_then(|d| citizen_description(&d).ok())
        {
            let list = &town.construction.professions.list;
            town.agents.life.services = d.refs.clone().map(|r| {
                let r = normalise(&r?);
                list.iter().position(|p| normalise(&p.reference) == r)
            });
        }
        town
    }

    /// The start structures (`FUN_1400a7490`) are built at once: the original creates them
    /// with `Game +0xd0` bit 0 set, so their ClearArea clears at once (`FUN_14011d230`), the
    /// BuildComponent skips gathering (`FUN_14010d760`) and finishes on entering its build
    /// state (`FUN_14010cfb0`). Each becomes a site that enters state 2 on its activation, its
    /// map stamped and its zone taken, keeping the storage its starting goods went to.
    fn build_structures(&mut self, archive: &Archive, count: usize) {
        for e in 0..count {
            if !self.placed[e].alive {
                continue;
            }
            let before = self.storages.len();
            self.add_site(archive, e);
            let Some(site) = self.construction.sites.get_mut(&e) else {
                continue;
            };
            if let Some(s) = self.storage_sites.iter().find(|s| s.entity == e) {
                if site.storage == before && self.storages.len() == before + 1 {
                    self.storages.pop();
                }
                site.storage = s.storage;
            }
            let t = &self.construction.templates[site.template];
            site.stages = 1;
            site.progress = t.work;
            site.complete = true;
            // Activation runs the machine from state 2.
            site.machine.pending = 2;
            let map = self.templates[self.placed[e].template].map.clone();
            let p = &self.placed[e];
            let rect = unbanished_game::world::placement::Rect {
                x0: p.cell.0,
                y0: p.cell.1,
                x1: p.cell.0 + p.footprint.0 - 1,
                y1: p.cell.1 + p.footprint.1 - 1,
            };
            let degrees = p.yaw as i32;
            if let Some(map) = map {
                let rules = unbanished_game::world::placement::PlacementRules {
                    mask: 0,
                    masks: Vec::new(),
                    mask_chars: String::new(),
                    footprint: map.footprint.clone(),
                    width: 0,
                    height: 0,
                    cell_type: map.cell_type as u8,
                };
                if !map.no_stamp {
                    unbanished_game::world::placement::stamp(
                        &mut self.cells,
                        &rules,
                        rect,
                        degrees,
                        e,
                    );
                }
                unbanished_game::world::placement::zone(&mut self.cells, &rules, rect, degrees, e);
            }
        }
    }

    /// The trader boat template (`Template\\TraderBoat.rsc`): its CharacterDescription and
    /// its model's animation group.
    pub(in crate::town) fn load_boat(
        &mut self,
        archive: &Archive,
        reference: &str,
    ) -> Option<usize> {
        let template = self.load_template(archive, reference)?;
        let file = normalise(reference).replace(":resource", ":character");
        let character = read(archive, &file)
            .and_then(|d| unbanished_formats::game::character_description(&d).ok())?;
        if let Some(Some(g)) = self.templates[template].animations.first() {
            self.wildlife.groups.insert((template, 0), g.clone());
        }
        self.wildlife.boat_kinds.insert(
            template,
            unbanished_game::commerce::boat::BoatKind { character },
        );
        Some(template)
    }

    /// Every loaded template with a ValueDescription (`crate::mods::templates`, `FUN_140059fd0`),
    /// its raw material registered.
    pub(in crate::town) fn load_trade_goods(&mut self, archive: &Archive) {
        if !self.construction.trade_goods.is_empty() {
            return;
        }
        let list = crate::mods::templates(archive);
        for r in &list {
            let r = normalise(r);
            if !r.starts_with("Template") || !r.ends_with(":resource") {
                continue;
            }
            let Some(value) = read(archive, &r.replace(":resource", ":value"))
                .and_then(|d| unbanished_formats::game::value_description(&d).ok())
            else {
                continue;
            };
            let Some(template) = self.load_template(archive, &r) else {
                continue;
            };
            let material = read(archive, &r.replace(":resource", ":rawmaterial"))
                .is_some()
                .then(|| material_index(archive, &mut self.materials, &r))
                .flatten();
            self.construction
                .trade_goods
                .push(unbanished_game::commerce::trade::TradeGood {
                    template,
                    material,
                    value,
                });
        }
    }
}
