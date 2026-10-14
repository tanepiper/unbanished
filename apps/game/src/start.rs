//! New Game start-condition setup (`FUN_1400a87d0`): start point, profile structures and natural
//! resource seeding, run on the game's shared random stream. See docs/RESOURCE-SEEDING.md and
//! START-CONDITION-PROFILES.md. Starting households and scheduled natural resources are active;
//! deer, crops, orchards and livestock come next.

use std::collections::HashMap;
use std::sync::Arc;

use banished_formats::game::{
    citizen_description, component_description, create_placed_description, growth_description,
    map_description, map_generator, model_description, natural_resource_description,
    raw_material_description, spawn_description, storage_description, MapGenerator,
};
use banished_formats::names::NameDatabase;
use banished_formats::pkg::Archive;
use banished_game::citizens::{Citizen, CitizenRules, Households};
use banished_game::map::Map;
use banished_game::random::Random;
use banished_game::scheduler::EntityScheduler;
use banished_game::seeding::{Cells, Placed, Seeder, Template};
use banished_game::stock::{distribute, RawMaterial, Storage};
use banished_game::terrain::HeightGrid;

use crate::models::{placement_matrix, ModelInstance, SceneModels};

/// `Template\X.rsc:resource` (profile lists omit the object name).
/// `DAT_140178818`: children's model scale.
const CHILD_SCALE: f32 = 0.66;

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
        bucket: desc.bucket as usize,
        map: None,
        model: None,
        animations: Vec::new(),
        natural: None,
        growth: None,
        growth_reference: None,
        spawn: Vec::new(),
        size: None,
        storage: None,
        storage_location: false,
    };
    let mut spawn = Vec::new();
    for d in desc.descriptions.iter().flatten() {
        let Some(data) = read(archive, d) else {
            continue;
        };
        let object = d.rsplit(':').next().unwrap_or("");
        match object {
            "map" => t.map = map_description(&data).ok(),
            "model" => {
                t.model = model_description(&data).ok();
                let mut cache = HashMap::new();
                if let Some(model) = &t.model {
                    t.animations = model
                        .variants
                        .iter()
                        .map(|v| {
                            let reference = v.animations.as_deref()?;
                            cache
                                .entry(reference.to_owned())
                                .or_insert_with(|| {
                                    read(archive, reference)
                                        .and_then(|bytes| {
                                            banished_formats::types::animation_group(&bytes).ok()
                                        })
                                        .map(Arc::new)
                                })
                                .clone()
                        })
                        .collect();
                }
            }
            "naturalresource" => t.natural = natural_resource_description(&data).ok(),
            "growth" => {
                t.growth = growth_description(&data).ok();
                t.growth_reference = Some(normalise(d));
            }
            "spawn" => spawn = spawn_description(&data).unwrap_or_default(),
            "storage" => {
                t.storage = storage_description(&data)
                    .ok()
                    .map(|d| (d.accepts, d.capacity, d.flag_20))
            }
            "storagelocation" => t.storage_location = true,
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

/// `FUN_1400a87d0` → `FUN_140097330` / `FUN_1400a73d0`: the profile's raw materials become the
/// candidate list, then seven `(mask, cap)` passes of `FUN_140096b30` fill the town storages.
///
/// *Inferred:* the town storage list holds the structures with a `storagelocation` component in
/// creation order (barn, yard, cart); houses and citizens are not in it. Their capacity is
/// `capacity · (per-cell ? footprint area : 1)` as in `FUN_140131b10`.
fn starting_goods(
    archive: &Archive,
    profile: &MapGenerator,
    structures: &[(banished_formats::game::PlacedStructure, Option<usize>)],
    templates: &[Template],
) -> (Vec<RawMaterial>, Vec<Storage>) {
    let mut materials = Vec::new();
    for r in &profile.raw_materials {
        let key = normalise(r);
        if materials.iter().any(|m: &RawMaterial| m.reference == key) {
            continue;
        }
        let Some(d) = read(archive, &key.replace(":resource", ":rawmaterial"))
            .and_then(|d| raw_material_description(&d).ok())
        else {
            continue;
        };
        materials.push(RawMaterial {
            reference: key,
            size: d.size,
            flags: d.flags,
        });
    }
    let mut storages: Vec<Storage> = structures
        .iter()
        .filter_map(|(record, t)| {
            let t = &templates[(*t)?];
            if !t.storage_location {
                return None;
            }
            let (accepts, capacity, per_cell) = t.storage?;
            let (w, h) = t.size.unwrap_or((record.a, record.b));
            Some(Storage {
                capacity: capacity * if per_cell { w * h } else { 1 },
                accepts,
                items: Vec::new(),
            })
        })
        .collect();
    let candidates: Vec<usize> = (0..materials.len()).collect();
    const MASKS: [u32; 7] = [0x400, 0x800, 0x1000, 0x200, 0x20, 0x40, 0x4000];
    for (i, mask) in MASKS.iter().enumerate() {
        distribute(
            &mut storages,
            &materials,
            &candidates,
            *mask,
            profile.ints[i + 1],
        );
    }
    for s in &storages {
        log::debug!(
            "storage cap {} accepts {:#x} used {}",
            s.capacity,
            s.accepts,
            s.used(&materials)
        );
    }
    (materials, storages)
}

/// `FUN_140002080`: the start cell's centre on the ground.
fn grid_start(map_size: usize, terrain: &HeightGrid, start: (i32, i32)) -> [f32; 3] {
    let n = map_size as i32;
    let c = (n >> 1) as f32 - 0.5;
    let (x, y) = (start.0 as f32 - c, start.1 as f32 - c);
    let mut normal = [0.0; 3];
    [x, y, terrain.ground(x, y, &mut normal)]
}

fn households(
    archive: &Archive,
    profile: &MapGenerator,
    start: &[f32; 3],
    rng: &mut Random,
) -> (Vec<Citizen>, CitizenRules) {
    let rules = read(
        archive,
        &normalise(&profile.citizen).replace(":resource", ":citizen"),
    )
    .and_then(|d| citizen_description(&d).ok())
    .map(|c| CitizenRules {
        adult_age: c.adult_age(),
        start_age: c.start_age(),
        lifespan: c.lifespan(),
        education: c.i(0x118),
    });
    let names = |n: &str| {
        archive
            .read(&format!("Game/Names/{n}_resource.crs"))
            .ok()
            .and_then(|d| NameDatabase::read(&d).ok())
    };
    let (Some(rules), Some(male), Some(female)) = (rules, names("Male"), names("Female")) else {
        log::warn!("households: citizen description or name databases missing");
        return (
            Vec::new(),
            CitizenRules {
                adult_age: 2.0,
                start_age: (2.2, 8.0),
                lifespan: (15.0, 3.0),
                education: 0,
            },
        );
    };
    let mut h = Households {
        rules,
        male_names: &male,
        female_names: &female,
        spread: profile.value_40,
        start: *start,
        citizens: Vec::new(),
    };
    h.create_all(rng, &profile.households);
    log::info!(
        "{} citizens: {:?}",
        h.citizens.len(),
        h.citizens
            .iter()
            .map(|c| c.name.as_str())
            .collect::<Vec<_>>()
    );
    (h.citizens, rules)
}

pub struct StartingTown {
    pub start: (i32, i32),
    pub citizens: Vec<Citizen>,
    pub rules: CitizenRules,
    pub materials: Vec<RawMaterial>,
    pub storages: Vec<Storage>,
    pub templates: Vec<Template>,
    pub placed: Vec<Placed>,
    pub cells: Cells,
    scheduler: EntityScheduler<usize>,
    removed: Vec<usize>,
    pub animation_events: Vec<(usize, String)>,
    citizen_model: Option<banished_formats::game::ModelDescription>,
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
        let mut cells = Cells::new(map_size, map.types);
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
        seeder.seed(&list, start.0, start.1, radius * radius);
        drop(seeder);
        let mut scheduler = EntityScheduler::default();
        for (id, p) in placed.iter().enumerate() {
            scheduler.register(id, templates[p.template].bucket);
        }
        // `FUN_1400a62a0`: the starting households around the start cell centre.
        let (citizens, rules) = households(
            archive,
            &profile,
            &grid_start(map_size, terrain, start),
            rng,
        );
        // Deer and other populations (`FUN_1400a5eb0`, `FUN_1400a57c0`) come here; not ported yet.
        let (materials, storages) = starting_goods(archive, &profile, &structures, &templates);
        log::info!(
            "start {:?}: {} natural resources placed",
            start,
            placed.len()
        );
        Some(StartingTown {
            start,
            citizens,
            rules,
            materials,
            storages,
            templates,
            placed,
            cells,
            scheduler,
            removed,
            animation_events,
            citizen_model: read(archive, r"Template\Citizen.rsc:model")
                .and_then(|d| banished_formats::game::model_description(&d).ok()),
        })
    }

    /// FUN_1400c9560: run placed entities on their template's scheduled bucket.
    pub fn iterate(
        &mut self,
        terrain: &HeightGrid,
        rng: &mut Random,
        dt: f32,
        temperature: f32,
    ) -> bool {
        Seeder {
            templates: &self.templates,
            terrain,
            cells: &mut self.cells,
            rng,
            placed: &mut self.placed,
            removed: &mut self.removed,
            animation_events: &mut self.animation_events,
        }
        .iterate(&mut self.scheduler, dt, temperature, false)
    }

    /// FUN_1400c98a0's post-iteration phase; also runs when the toolbar pauses simulation.
    pub fn smooth(&mut self, terrain: &HeightGrid, rng: &mut Random) -> bool {
        Seeder {
            templates: &self.templates,
            terrain,
            cells: &mut self.cells,
            rng,
            placed: &mut self.placed,
            removed: &mut self.removed,
            animation_events: &mut self.animation_events,
        }
        .smooth()
    }

    /// Model instances of every placed entity, grouped by mesh.
    pub fn models(&self, archive: &Archive) -> SceneModels {
        SceneModels::from_instances(archive, self.model_instances())
    }

    /// Keep mesh/texture resources cached while GrowthComponent changes only instance scales.
    pub fn refresh_models(&self, archive: &Archive, models: &mut SceneModels) {
        models.update_instances(archive, self.model_instances());
    }

    fn model_instances(&self) -> Vec<(String, usize, Vec<ModelInstance>)> {
        let mut groups: HashMap<String, Vec<ModelInstance>> = HashMap::new();
        for p in &self.placed {
            if !p.alive {
                continue;
            }
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
            groups.entry(mesh.clone()).or_default().push(ModelInstance {
                matrix: placement_matrix(p.position, [p.scale[0], p.scale[1], p.scale[2]], p.yaw),
                locals: p.animation.as_ref().and_then(|playback| {
                    self.templates[p.template]
                        .animations
                        .get(p.variant)
                        .and_then(Option::as_ref)
                        .and_then(|g| g.clips.get(playback.clip))
                        .map(|clip| playback.pose(clip))
                }),
            });
        }
        // Citizens: `Template/Citizen.rsc:model` variant 0 (men) or 1 (women); children use
        // the scale `DAT_140178818` (`FUN_14011a700`). Facing is not set at creation (yaw 0).
        if let Some(model) = &self.citizen_model {
            for c in &self.citizens {
                let variant = if c.male { 0 } else { 1 };
                let Some(Some(mesh)) = model.variants.get(variant).and_then(|v| v.meshes.first())
                else {
                    continue;
                };
                let s = if c.is_adult(&self.rules) {
                    1.0
                } else {
                    CHILD_SCALE
                };
                groups.entry(mesh.clone()).or_default().push(ModelInstance {
                    matrix: placement_matrix(c.position, [s, s, s], 0.0),
                    locals: None,
                });
            }
        }
        let mut list: Vec<_> = groups.into_iter().map(|(m, i)| (m, 0, i)).collect();
        list.sort_by(|a, b| a.0.cmp(&b.0));
        list
    }
}
