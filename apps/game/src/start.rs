//! New Game start-condition setup (`FUN_1400a87d0`): start point, profile structures and natural
//! resource seeding, run on the game's shared random stream. See docs/RESOURCE-SEEDING.md and
//! START-CONDITION-PROFILES.md. Starting households and scheduled natural resources are active;
//! deer, crops, orchards and livestock come next.

use std::collections::HashMap;
use std::sync::Arc;

use banished_formats::game::{
    citizen_description, component_description, create_placed_description, growth_description,
    map_description, map_generator, model_description, natural_resource_description,
    radius_description, raw_material_description, spawn_description, storage_description,
    storage_location_description, MapGenerator,
};
use banished_formats::names::NameDatabase;
use banished_formats::pkg::Archive;
use banished_game::agents::{Agents, CitizenRulesAi};
use banished_game::ai::Thresholds;
use banished_game::citizens::{Citizen, CitizenRules, Households};
use banished_game::map::Map;
use banished_game::random::Random;
use banished_game::scheduler::EntityScheduler;
use banished_game::seeding::{Cells, Placed, ScheduledComponent, Seeder, Template};
use banished_game::stock::{distribute, RawMaterial, Storage, StorageSite};
use banished_game::terrain::HeightGrid;

use banished_game::construction::{Construction, Controller, Site, SiteTemplate};
use banished_game::professions::{Profession, Professions, WorkPlace};

use crate::models::{placement_matrix, GroupInstances, ModelInstance, SceneModels};

/// `Profession.rsc:resource` (laborer `+0x30`, builder `+0x38`) and `:professionList`.
fn professions(archive: &Archive) -> Professions {
    let list = read(archive, "Game/Profession/Profession.rsc:professionList")
        .and_then(|d| banished_formats::game::external_list(&d).ok())
        .unwrap_or_default();
    let list: Vec<Profession> = list
        .iter()
        .map(|r| {
            let p = read(archive, r).and_then(|d| banished_formats::game::profession(&d).ok());
            Profession {
                reference: r.clone(),
                name: p.as_ref().map(|p| p.name.clone()).unwrap_or_default(),
                string_table: p.as_ref().and_then(|p| p.string_table.clone()),
                works_anywhere: p.is_some_and(|p| p.flag_4c),
            }
        })
        .collect();
    let find = |key: &str| {
        list.iter()
            .position(|p| p.reference.ends_with(&format!(":{key}")))
            .unwrap_or(0)
    };
    Professions {
        laborer: find("laborer"),
        builder: find("builder"),
        list,
    }
}

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
    });
    Some(materials.len() - 1)
}

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
        audio_events: Vec::new(),
        natural: None,
        growth: None,
        growth_reference: None,
        spawn: Vec::new(),
        updates: Vec::new(),
        size: None,
        storage: None,
        storage_location: false,
        storage_location_flags: 0,
        market_radius: None,
        market_location: false,
        has_interact: false,
        use_points: Vec::new(),
        build_points: Vec::new(),
        pile: None,
        pile_material: None,
        pile_yield: (0, 0),
    };
    let mut pile_ref = None;
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
                    // The AnimAudioEvents the clips reference.
                    for group in t.animations.iter().flatten() {
                        for clip in &group.clips {
                            for event in &clip.events {
                                let Some(r) = event.resource.as_deref() else {
                                    continue;
                                };
                                if t.audio_events.iter().any(|(e, _, _)| e == r) {
                                    continue;
                                }
                                if let Some(a) = read(archive, r).and_then(|b| {
                                    banished_formats::audio::anim_audio_event(&b).ok()
                                }) {
                                    t.audio_events
                                        .push((r.to_string(), a.effect, a.probability));
                                }
                            }
                        }
                    }
                }
            }
            "naturalresource" => {
                t.natural = natural_resource_description(&data).ok();
                pile_ref = t.natural.as_ref().and_then(|n| n.raw_material.clone());
            }
            "rawmaterial" => {
                if let Ok(d) = raw_material_description(&data) {
                    t.pile_yield = (d.value_40, d.value_44);
                }
            }
            "growth" => {
                t.growth = growth_description(&data).ok();
                t.growth_reference = Some(normalise(d));
                t.updates.push(ScheduledComponent::Growth);
            }
            "spawn" => {
                spawn = spawn_description(&data).unwrap_or_default();
                t.updates.push(ScheduledComponent::Spawn);
            }
            "storage" => {
                t.storage = storage_description(&data)
                    .ok()
                    .map(|d| (d.accepts, d.capacity, d.flag_20))
            }
            "storagelocation" => {
                t.storage_location = true;
                t.storage_location_flags = storage_location_description(&data).unwrap_or(0);
            }
            "radius" => t.market_radius = radius_description(&data).ok().map(|r| r.0),
            "marketlocation" => t.market_location = true,
            "interact" => {
                t.has_interact = true;
                let list = banished_formats::game::interact_description(&data)
                    .ok()
                    .and_then(|i| i.points)
                    .and_then(|r| read(archive, &normalise(&r)))
                    .and_then(|d| banished_formats::game::point_list(&d).ok());
                let points = |category| {
                    list.as_ref().map_or(Vec::new(), |l| {
                        banished_formats::game::interaction_points(l, category)
                            .iter()
                            .map(|p| p.position)
                            .collect()
                    })
                };
                t.use_points = points(0);
                t.build_points = points(1);
            }
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
    // `FUN_14012b5c0` clones the resource's raw-material template as a pile.
    if let Some(r) = pile_ref {
        table[index].pile = load_template(archive, &r, table);
    }
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
/// Town storage registrations retain the placed entity identity and creation order. Capacity is
/// `capacity · (per-cell ? footprint area : 1)` as in `FUN_140131b10`.
fn starting_goods(
    archive: &Archive,
    profile: &MapGenerator,
    placed: &[Placed],
    templates: &[Template],
) -> (Vec<RawMaterial>, Vec<Storage>, Vec<StorageSite>) {
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
            tool_uses: d.value_54,
            preferred: d.flag_58,
        });
    }
    let mut storages = Vec::new();
    let mut sites = Vec::new();
    for (entity, p) in placed.iter().enumerate() {
        let t = &templates[p.template];
        if !t.storage_location && !t.market_location {
            continue;
        }
        let Some((accepts, capacity, per_cell)) = t.storage else {
            continue;
        };
        let (w, h) = p.footprint;
        sites.push(StorageSite {
            entity,
            storage: storages.len(),
            location_flags: t.storage_location_flags,
            radius: if t.market_location {
                t.market_radius
            } else {
                None
            },
        });
        storages.push(Storage {
            capacity: capacity * if per_cell { w * h } else { 1 },
            accepts,
            items: Vec::new(),
        });
    }
    // FUN_140096b30 walks the ordinary storage registry, excluding market owners.
    let mut ordinary: Vec<_> = sites
        .iter()
        .filter(|s| s.radius.is_none())
        .map(|s| storages[s.storage].clone())
        .collect();
    let candidates: Vec<usize> = (0..materials.len()).collect();
    const MASKS: [u32; 7] = [0x400, 0x800, 0x1000, 0x200, 0x20, 0x40, 0x4000];
    for (i, mask) in MASKS.iter().enumerate() {
        distribute(
            &mut ordinary,
            &materials,
            &candidates,
            *mask,
            profile.ints[i + 1],
        );
    }
    for (site, inventory) in sites.iter().filter(|s| s.radius.is_none()).zip(ordinary) {
        storages[site.storage] = inventory;
    }
    for s in &storages {
        log::debug!(
            "storage cap {} accepts {:#x} used {}",
            s.capacity,
            s.accepts,
            s.used(&materials)
        );
    }
    (materials, storages, sites)
}

/// Each citizen model variant's animation group.
fn citizen_animations(
    archive: &Archive,
    model: &banished_formats::game::ModelDescription,
) -> Vec<Option<Arc<banished_formats::types::AnimationGroup>>> {
    model
        .variants
        .iter()
        .map(|v| {
            v.animations
                .as_deref()
                .and_then(|r| read(archive, r))
                .and_then(|d| banished_formats::types::animation_group(&d).ok())
                .map(Arc::new)
        })
        .collect()
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
    /// Citizen StatusIconDescription and the male/female mesh box heights (model `+0x5c`).
    citizen_icons: Option<banished_formats::game::StatusIconDescription>,
    citizen_heights: [f32; 2],
    /// Professions CitizenUI names (laborer, child, student), by resource key.
    archive_professions: Vec<(String, banished_formats::game::Profession)>,
    pub start: (i32, i32),
    pub citizens: Vec<Citizen>,
    pub rules: CitizenRules,
    pub materials: Vec<RawMaterial>,
    pub storages: Vec<Storage>,
    pub storage_sites: Vec<StorageSite>,
    pub templates: Vec<Template>,
    pub placed: Vec<Placed>,
    pub cells: Cells,
    scheduler: EntityScheduler<usize>,
    removed: Vec<usize>,
    pub animation_events: Vec<(usize, String)>,
    pub(crate) citizen_model: Option<banished_formats::game::ModelDescription>,
    /// Each variant's animation group (`Models\Citizen\CitizenAnims.rsc`).
    pub(crate) citizen_anims: Vec<Option<Arc<banished_formats::types::AnimationGroup>>>,
    /// The citizens' CharacterComponent and CitizenAIComponent (docs/CITIZEN-AI.md).
    pub agents: Agents,
    /// The placement tool's ghost (`PlacementTool +0x68`): mesh and transform.
    pub ghost: Option<(String, [f32; 16])>,
    /// The job manager, professions, workplaces and construction sites (docs/JOBS.md).
    pub construction: Construction,
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
            scheduler.register(id, p.bucket);
        }
        // `FUN_1400a62a0`: the starting households around the start cell centre.
        let (citizens, rules) = households(
            archive,
            &profile,
            &grid_start(map_size, terrain, start),
            rng,
        );
        // Deer and other populations (`FUN_1400a5eb0`, `FUN_1400a57c0`) come here; not ported yet.
        let (materials, storages, storage_sites) =
            starting_goods(archive, &profile, &placed, &templates);
        log::info!(
            "start {:?}: {} natural resources placed",
            start,
            placed.len()
        );
        let citizen_model = read(archive, r"Template\Citizen.rsc:model")
            .and_then(|d| banished_formats::game::model_description(&d).ok());
        let citizen_ref = normalise(&profile.citizen);
        let character = read(archive, &citizen_ref.replace(":resource", ":character"))
            .and_then(|d| banished_formats::game::character_description(&d).ok())
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
                health_max: c.f(0x70),
                hunger_limit: c.f(0xd0),
                cold_limit: c.f(0xf0),
                profession_allows_idle: read(
                    archive,
                    "Game/Profession/Profession.rsc:professionList",
                )
                .and_then(|d| banished_formats::game::external_list(&d).ok())
                .and_then(|l| l.first().cloned())
                .and_then(|r| read(archive, &r))
                .and_then(|d| banished_formats::game::profession(&d).ok())
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
        let agents = Agents::new(&citizens, &cells, character, ai_rules, carried, |c| {
            if c.is_adult(&rules) {
                1.0
            } else {
                CHILD_SCALE
            }
        });
        let archive_professions = ["laborer", "child", "student"]
            .iter()
            .filter_map(|k| {
                read(archive, &format!("Game/Profession/Profession.rsc:{k}"))
                    .and_then(|d| banished_formats::game::profession(&d).ok())
                    .map(|p| (k.to_string(), p))
            })
            .collect();
        let citizen_icons = read(archive, &citizen_ref.replace(":resource", ":statusicon"))
            .and_then(|d| banished_formats::game::status_icon_description(&d).ok());
        let citizen_heights = [0, 1].map(|v| {
            citizen_model
                .as_ref()
                .and_then(|m| m.variants.get(v)?.meshes.first()?.clone())
                .and_then(|r| read(archive, &normalise(&r)))
                .and_then(|d| banished_formats::types::graphics_mesh(&d).ok())
                .map_or(0.0, |m| m.bounds_max[2] - m.bounds_min[2])
        });
        let mut town = StartingTown {
            citizen_icons,
            citizen_heights,
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
            removed,
            animation_events,
            citizen_anims: citizen_model
                .as_ref()
                .map(|m| citizen_animations(archive, m))
                .unwrap_or_default(),
            citizen_model,
            agents,
            ghost: None,
            construction: Construction {
                professions: professions(archive),
                ..Default::default()
            },
        };
        town.resolve_piles(archive);
        Some(town)
    }

    /// `Game/Profession/Profession.rsc:<key>` (read on demand; presentation only).
    pub fn profession(&self, key: &str) -> Option<banished_formats::game::Profession> {
        self.archive_professions
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, p)| p.clone())
    }

    /// The minimap overlay (`Game +0x148`): every live entity whose MapDescription has the
    /// minimap flag (`+0x32`) fills its map rectangle with its colour (`FUN_140129680` →
    /// `FUN_1400aa200`, rows flipped so the top row is north). RGBA bytes, `size²` pixels.
    pub fn minimap_overlay(&self) -> Vec<u8> {
        let n = self.cells.size as i32;
        let mut out = vec![0u8; (n * n * 4) as usize];
        for p in self.placed.iter().filter(|p| p.alive) {
            let Some(map) = self.templates[p.template]
                .map
                .as_ref()
                .filter(|m| m.minimap)
            else {
                continue;
            };
            let c = map.color;
            let rgba = [(c >> 16) as u8, (c >> 8) as u8, c as u8, (c >> 24) as u8];
            let ((x, y), (w, h)) = (p.cell, p.footprint);
            for j in 0..h {
                for i in 0..w {
                    let (px, row) = (x + i, n - y - j - 1);
                    if (0..n).contains(&px) && (0..n).contains(&row) {
                        let o = ((row * n + px) * 4) as usize;
                        out[o..o + 4].copy_from_slice(&rgba);
                    }
                }
            }
        }
        out
    }

    /// StatusIconComponent billboards (`FUN_1401310a0` anchors): each citizen with status bits,
    /// above its model (position + model box height). Building status bits are not ported.
    pub fn status_icons(&self) -> Vec<crate::status_icons::StatusIcon> {
        let Some(d) = &self.citizen_icons else {
            return Vec::new();
        };
        let Some(sheet) = d.sheet.clone() else {
            return Vec::new();
        };
        self.citizens
            .iter()
            .zip(&self.agents.states)
            .zip(&self.agents.characters)
            .filter(|((_, s), _)| s.status != 0)
            .map(|((c, s), ch)| crate::status_icons::StatusIcon {
                anchor: glam::Vec3::from(ch.position)
                    + glam::Vec3::Z * self.citizen_heights[usize::from(!c.male)],
                mask: s.status,
                sheet: sheet.clone(),
                size: d.size,
                screen_constant: d.screen_constant,
            })
            .collect()
    }

    /// The icon sheets `status_icons` refers to.
    pub fn icon_sheet_refs(&self) -> Vec<String> {
        self.citizen_icons
            .iter()
            .filter_map(|d| d.sheet.clone())
            .collect()
    }

    /// `FUN_1400c9d70`: register a new entity in its template's bucket.
    pub fn register_entity(&mut self, id: usize) {
        let handle = self.scheduler.register(id, self.placed[id].bucket);
        debug_assert_eq!(handle, id);
    }

    /// Load (or find) a template for an entity created during play.
    pub fn load_template(&mut self, archive: &Archive, reference: &str) -> Option<usize> {
        let t = load_template(archive, reference, &mut self.templates);
        self.resolve_piles(archive);
        t
    }

    /// Pile templates' raw materials in the town's list (`FUN_140097330`, which the original
    /// runs when a pile is set up; resolved here when its template loads).
    fn resolve_piles(&mut self, archive: &Archive) {
        for i in 0..self.templates.len() {
            let t = &self.templates[i];
            if t.pile_material.is_some() || t.pile_yield == (0, 0) {
                continue;
            }
            let reference = t.reference.clone();
            self.templates[i].pile_material =
                material_index(archive, &mut self.materials, &reference);
        }
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

    /// Player commands and newly placed sites (before an iteration).
    pub fn apply_commands(&mut self, terrain: &HeightGrid, rng: &mut Random) {
        let groups: Vec<_> = self
            .citizens
            .iter()
            .map(|c| {
                self.citizen_anims
                    .get(usize::from(!c.male))
                    .and_then(Option::as_deref)
            })
            .collect();
        let mut env = banished_game::town::Env {
            cells: &mut self.cells,
            placed: &mut self.placed,
            templates: &self.templates,
            terrain,
            materials: &self.materials,
            storages: &mut self.storages,
            sites: &self.storage_sites,
            rng,
            construction: &mut self.construction,
            stock: [(0, 0); 21],
            temperature: 0.0,
        };
        self.agents
            .apply_commands(&self.citizens, &mut env, &groups);
    }

    /// The citizens' animation, characters and AI for one simulation iteration
    /// (`banished_game::agents`). `stock` is the stock manager's `(count, limit)` per slot.
    /// Returns whether any citizen is drawn.
    pub fn update_citizens(
        &mut self,
        terrain: &HeightGrid,
        rng: &mut Random,
        dt: f32,
        temperature: f32,
        stock: [(i32, i32); 21],
    ) -> bool {
        let before = self.placed.len();
        let groups: Vec<_> = self
            .citizens
            .iter()
            .map(|c| {
                self.citizen_anims
                    .get(usize::from(!c.male))
                    .and_then(Option::as_deref)
            })
            .collect();
        let mut env = banished_game::town::Env {
            cells: &mut self.cells,
            placed: &mut self.placed,
            templates: &self.templates,
            terrain,
            materials: &self.materials,
            storages: &mut self.storages,
            sites: &self.storage_sites,
            rng,
            construction: &mut self.construction,
            stock,
            temperature,
        };
        self.agents
            .update(&mut self.citizens, &mut env, &groups, dt);
        // `FUN_1400c9d70`: piles left by cleared resources join the entity manager's buckets.
        for id in before..self.placed.len() {
            self.register_entity(id);
        }
        !self.citizens.is_empty()
    }

    /// The registered town storages (`Game +0xf0 +0x00/+0x10`), the ones the stock counts.
    pub fn registered_storages(&self) -> Vec<Storage> {
        self.storage_sites
            .iter()
            .map(|s| self.storages[s.storage].clone())
            .collect()
    }

    /// Adult counts per profession and each profession's workplace places (`FUN_14007ba70`).
    pub fn profession_counts(&self) -> (Vec<i32>, Vec<i32>) {
        let c = &self.construction;
        let mut counts = vec![0; c.professions.list.len().max(1)];
        for (citizen, state) in self.citizens.iter().zip(&self.agents.states) {
            if citizen.is_adult(&self.rules) && citizen.school.is_none() {
                if let Some(n) = counts.get_mut(state.profession) {
                    *n += 1;
                }
            }
        }
        let capacity = (0..counts.len()).map(|p| c.workforce.capacity(p)).collect();
        (counts, capacity)
    }

    /// The construction descriptions of a template (`None` without a BuildDescription).
    pub fn site_template(&mut self, archive: &Archive, template: usize) -> Option<usize> {
        let reference = self.templates[template].reference.clone();
        if let Some(i) = self
            .construction
            .templates
            .iter()
            .position(|t| t.reference == reference)
        {
            return Some(i);
        }
        let desc = component_description(&read(archive, &reference)?).ok()?;
        let mut t = SiteTemplate {
            reference: reference.clone(),
            ..Default::default()
        };
        let mut has_build = false;
        for d in desc.descriptions.iter().flatten() {
            let Some(data) = read(archive, d) else {
                continue;
            };
            match d.rsplit(':').next().unwrap_or("") {
                "cleararea" => {
                    t.controllers.push(Controller::ClearArea);
                    t.clear = banished_formats::game::clear_area_description(&data).ok();
                }
                "workplace" => {
                    t.controllers.push(Controller::WorkPlace);
                    t.profession = banished_formats::game::workplace_description(&data)
                        .ok()
                        .and_then(|w| w.profession)
                        .and_then(|p| {
                            let p = p.replace('/', "\\");
                            self.construction
                                .professions
                                .list
                                .iter()
                                .position(|x| x.reference.eq_ignore_ascii_case(&p))
                        });
                }
                "build" => {
                    has_build = true;
                    t.controllers.push(Controller::Build);
                    let b = banished_formats::game::build_description(&data).ok()?;
                    t.work = b.work;
                    for (n, m) in b.materials {
                        let m = m.and_then(|m| material_index(archive, &mut self.materials, &m))?;
                        t.materials.push((n, m));
                    }
                }
                "residence" => t.controllers.push(Controller::Residence),
                "work" => {
                    t.max_workers =
                        banished_formats::game::work_description(&data).map_or(0, |w| w.max_workers)
                }
                "storage" => {
                    t.storage = storage_description(&data)
                        .ok()
                        .map(|d| (d.accepts, d.capacity, d.flag_20))
                }
                _ => {}
            }
        }
        if !has_build {
            return None;
        }
        t.meshes = self.templates[template]
            .model
            .as_ref()
            .map(|m| m.variants.iter().map(|v| v.meshes.len()).collect())
            .unwrap_or_default();
        self.construction.templates.push(t);
        Some(self.construction.templates.len() - 1)
    }

    /// A placed building with a BuildComponent becomes a construction site: its own
    /// StorageComponent and WorkPlaceComponent; activated before the next iteration.
    pub fn add_site(&mut self, archive: &Archive, entity: usize) {
        let template = self.placed[entity].template;
        let Some(st) = self.site_template(archive, template) else {
            return;
        };
        let t = &self.construction.templates[st];
        let (w, h) = self.placed[entity].footprint;
        let storage = self.storages.len();
        self.storages.push(match t.storage {
            Some((accepts, capacity, per_cell)) => Storage {
                capacity: capacity * if per_cell { w * h } else { 1 },
                accepts,
                items: Vec::new(),
            },
            None => Storage::default(),
        });
        if t.controllers.contains(&Controller::WorkPlace) {
            self.construction
                .workforce
                .workplaces
                .insert(entity, WorkPlace::new(t.profession, t.max_workers));
        }
        let machine = banished_game::construction::Machine::new(t.controllers.clone());
        self.construction.sites.insert(
            entity,
            Site {
                entity,
                template: st,
                machine,
                progress: 0,
                stages: 1,
                work_id: -1,
                work_priority: None,
                storage,
                complete: false,
            },
        );
        self.construction.placed.push(entity);
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

    fn model_instances(&self) -> Vec<GroupInstances> {
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
        // the scale `DAT_140178818` (`FUN_14011a700`).
        if let Some(model) = &self.citizen_model {
            for (i, c) in self.citizens.iter().enumerate() {
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
                // FUN_14011a5e0/FUN_14011a700 write the outfit (`+0x34`) into the mesh
                // instance's `w`, read by the character shader from the root translation.
                // CharacterComponent: model yaw (heading + yaw offset) and its animation.
                let ch = self.agents.characters.get(i);
                let yaw = ch.map_or(0.0, |ch| ch.yaw);
                let mut matrix = placement_matrix(c.position, [s, s, s], yaw);
                matrix[15] = c.appearance as f32;
                let locals = ch
                    .zip(self.citizen_anims.get(variant).and_then(Option::as_ref))
                    .and_then(|(ch, g)| {
                        let p = &ch.animation;
                        g.clips.get(p.clip).map(|clip| p.pose(clip))
                    });
                groups
                    .entry(mesh.clone())
                    .or_default()
                    .push(ModelInstance { matrix, locals });
            }
        }
        if let Some((mesh, matrix)) = &self.ghost {
            groups.entry(mesh.clone()).or_default().push(ModelInstance {
                matrix: *matrix,
                locals: None,
            });
        }
        let mut list: Vec<_> = groups.into_iter().map(|(m, i)| (m, 0, i, None)).collect();
        list.sort_by(|a, b| a.0.cmp(&b.0));
        list
    }
}

#[cfg(test)]
mod storage_fetch_tests {
    use super::*;
    use banished_formats::pkg::Package;
    use banished_game::{ai::activity, stock::stock_table};

    #[test]
    fn new_citizens_equip_from_registered_storage_then_loiter() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../game-data/original/WinData");
        let mut archive = Archive::default();
        for name in ["data1.pkg", "patch1.pkg", "patch0.pkg", "data0.pkg"] {
            let Ok(bytes) = std::fs::read(dir.join(name)) else {
                eprintln!("game data missing; skipping citizen fetch integration");
                return;
            };
            archive.add(Package::parse(bytes).unwrap());
        }
        let world = crate::world::World::generate(
            &archive,
            &crate::world::WorldParams {
                seed: 1234,
                ..Default::default()
            },
        )
        .unwrap();
        let terrain = HeightGrid {
            width: world.terrain.width(),
            heights: &world.terrain.heights,
        };
        let mut rng = Random::new(1234);
        let mut town = StartingTown::create(
            &archive,
            "Game/StartCondition/Medium.rsc:resource",
            world.terrain.size,
            &terrain,
            &mut rng,
        )
        .unwrap();
        assert!(!town.storage_sites.is_empty());
        let initial = stock_table(&town.storages, &town.materials);
        assert_eq!(&initial[..8], &[120, 60, 40, 200, 40, 1800, 0, 30]);
        let yard = town
            .storage_sites
            .iter()
            .find(|s| {
                town.templates[town.placed[s.entity].template]
                    .reference
                    .contains("StorageYard")
            })
            .unwrap();
        assert_eq!(town.placed[yard.entity].footprint, (4, 6));
        assert_eq!(town.storages[yard.storage].capacity, 6000);
        let positions: Vec<_> = town.citizens.iter().map(|c| c.position).collect();
        let adults = town
            .citizens
            .iter()
            .filter(|c| c.is_adult(&town.rules))
            .count();
        for _ in 0..3600 {
            town.update_citizens(&terrain, &mut rng, 1.0 / 30.0, 0.0, [(1000, 0); 21]);
        }
        let final_stock = stock_table(&town.storages, &town.materials);
        assert_eq!(initial[4] - final_stock[4], adults as i32);
        assert_eq!(initial[7] - final_stock[7], town.citizens.len() as i32);
        for (i, (citizen, state)) in town.citizens.iter().zip(&town.agents.states).enumerate() {
            assert!(state.clothing > 0, "{} has no clothing", citizen.name);
            assert_eq!(state.clothing_status, state.clothing);
            if citizen.is_adult(&town.rules) {
                assert!(state.tool_uses > 0, "{} has no tool", citizen.name);
            }
            assert_eq!(town.agents.ais[i].current, activity::IDLE);
            assert_eq!(state.health_events & 4, 4);
            assert_ne!(citizen.position, positions[i]);
        }
        assert!(town
            .storages
            .iter()
            .all(|s| s.items.iter().all(|e| e.reserved == 0)));
        eprintln!(
            "equipped {} citizens / {adults} adults; tools {} -> {}, clothes {} -> {}",
            town.citizens.len(),
            initial[4],
            final_stock[4],
            initial[7],
            final_stock[7]
        );
        // Exhausted storage must set failure/status fields and cool down both searches.
        for s in &mut town.storages {
            s.items.clear();
        }
        for i in 0..town.citizens.len() {
            let group = town.citizen_anims[usize::from(!town.citizens[i].male)]
                .as_deref()
                .unwrap();
            let ch = &mut town.agents.characters[i];
            let mut stack = std::mem::take(&mut ch.stack);
            stack.clear(ch, group, &mut town.storages);
            ch.stack = stack;
            town.agents.states[i] = banished_game::agents::CitizenState {
                tool_status: 9,
                clothing_status: 9,
                job: 42,
                ..Default::default()
            };
            town.agents.ais[i] = Default::default();
        }
        town.update_citizens(&terrain, &mut rng, 1.0 / 30.0, 0.0, [(1000, 0); 21]);
        for (i, state) in town.agents.states.iter().enumerate() {
            assert_eq!(state.flags & 4, 4);
            assert_eq!(state.clothing_status, 0);
            assert!(town.agents.ais[i].cooldowns[activity::CLOTHING as usize] > 0);
            if town.citizens[i].is_adult(&town.rules) {
                assert_eq!(state.tool_status, 0);
                assert!(town.agents.ais[i].cooldowns[activity::TOOL as usize] > 0);
            }
            assert_eq!(state.job, 42); // The success-only writes must not run.
            assert_eq!(state.health_events & 4, 0);
        }
    }
}
