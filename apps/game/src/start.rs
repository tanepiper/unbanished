//! New Game start-condition setup (`FUN_1400a87d0`): start point, profile structures and natural
//! resource seeding, run on the game's shared random stream. See docs/RESOURCE-SEEDING.md and
//! START-CONDITION-PROFILES.md. Starting households and scheduled natural resources are active;
//! deer, crops, orchards and livestock come next.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use banished_formats::game::{
    citizen_description, component_description, create_placed_description, growth_description,
    map_description, map_generator, model_description, natural_resource_description,
    particle_description, radius_description, raw_material_description, spawn_description,
    storage_description, storage_location_description, MapGenerator,
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
use banished_game::town::cell_ground;

use banished_game::construction::{Construction, Controller, Site, SiteTemplate};
use banished_game::professions::{Profession, Professions, WorkPlace};

mod bridge;
mod load;
// The web build has no save path yet (no Save dialog, no file access).
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
mod save;
mod tornado;
pub use bridge::BridgeSpec;

use crate::models::{placement_matrix, GroupInstances, GroupMaterial, ModelInstance, SceneModels};

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
                sprite_sheet: p.as_ref().and_then(|p| p.sprite_sheet.clone()),
                works_anywhere: p.as_ref().is_some_and(|p| p.flag_4c),
                immune_at_work: p.as_ref().is_some_and(|p| p.flag_4d),
                death: p.as_ref().map(|p| p.death.clone()).unwrap_or_default(),
                accident: p.as_ref().map_or((0.0, 0.0), |p| (p.value_54, p.value_58)),
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
        pile_spacing: d.value_4c,
        pile_amount: d.value_40,
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

/// A rectangle of cells (`x0, y0, x1, y1`, or `x, y, w, h`).
type Cells4 = (i32, i32, i32, i32);

/// `FUN_1400e1460`: the shelter's cells `(x0, y0, x1, y1)` in the corner of the area the
/// rotation faces (or centred on a side), its turn (0, −90, 180, 90 degrees) and its centre
/// (the middle of its corner cells, at the area's mean ground height, `FUN_140106c00`).
fn fenced_shelter(
    d: &banished_formats::game::CreateFencedDescription,
    size: (i32, i32),
    (x0, y0, x1, y1): Cells4,
    rotation: i32,
    ground: impl Fn(i32, i32) -> [f32; 3],
) -> Option<(Cells4, f32, [f32; 3])> {
    if x1 - x0 + 1 < d.min.0 || y1 - y0 + 1 < d.min.1 {
        return None;
    }
    let r = rotation.rem_euclid(4);
    let yaw = [0.0, -90.0, 180.0, 90.0][r as usize];
    let (sw, sh) = if r == 1 || r == 3 {
        (size.1, size.0)
    } else {
        size
    };
    let (mut a0, mut b0, mut a1, mut b1) = (x0, y0, x1, y1);
    if !d.centered {
        match r {
            0 => {
                a0 = a1 - sw + 1;
                b0 = b1 - sh + 1;
            }
            1 => {
                a0 = a1 - sw + 1;
                b1 = b0 - 1 + sh;
            }
            2 => {
                a1 = a0 - 1 + sw;
                b1 = b0 - 1 + sh;
            }
            _ => {
                b0 = b1 - sh + 1;
                a1 = a0 - 1 + sw;
            }
        }
    } else {
        match r {
            0 => {
                b1 = b0;
                a0 = ((a0 + a1) >> 1) - (sw >> 1);
                a1 = sw - 1 + a0;
            }
            1 => {
                a1 = a0;
                b0 = ((b0 + b1) >> 1) - (sh >> 1);
                b1 = sh - 1 + b0;
            }
            2 => {
                b0 = b1;
                a0 = ((a0 + a1) >> 1) - (sw >> 1);
                a1 = sw - 1 + a0;
            }
            _ => {
                a0 = a1;
                b0 = ((b0 + b1) >> 1) - (sh >> 1);
                b1 = sh - 1 + b0;
            }
        }
    }
    let mut mean = 0.0;
    let share = 1.0 / ((a1 - a0 + 1) * (b1 - b0 + 1)) as f32;
    for y in b0..=b1 {
        for x in a0..=a1 {
            mean += ground(x, y)[2] * share;
        }
    }
    let g0 = ground(a0, b0);
    let g1 = ground(a1, b1);
    let centre = [(g0[0] + g1[0]) * 0.5, (g0[1] + g1[1]) * 0.5, mean];
    Some(((a0, b0, a1, b1), yaw, centre))
}

/// `FUN_1400e0d90`: the pieces of one side from `from` to `to` (one cell wide) around the
/// shelter's cells `s`, as (part index, `x, y, w, h`, turn): an end piece on either side of
/// the shelter, single straight pieces where two will not fit, else long ones, the second of
/// which is the gate (none on a side the centred shelter crosses).
fn fence_run(
    from: (i32, i32),
    to: (i32, i32),
    s: Cells4,
    turn: i32,
    centered: bool,
) -> Vec<(usize, Cells4, i32)> {
    let mut out = Vec::new();
    let (x0, mut y) = from;
    let (x1, y1) = to;
    let sx = i32::from(0 < x1 - x0);
    let sy = i32::from(0 < y1 - y);
    let (lw, lh) = if turn == 0 { (2, 1) } else { (1, 2) };
    let inside = |x: i32, y: i32| s.0 <= x && x <= s.2 && s.1 <= y && y <= s.3;
    let mut clear = true;
    if centered && y <= y1 {
        for cy in y..=y1 {
            for cx in x0..=x1 {
                if inside(cx, cy) {
                    clear = false;
                }
            }
        }
    }
    let (mut fence, mut past, mut long, mut gate) = (false, false, false, false);
    while y <= y1 {
        let mut x = x0;
        while x <= x1 {
            if inside(x, y) {
                fence = false;
                past = true;
                long = false;
            } else {
                let next = inside(x + sx, y + sy);
                if past && !fence {
                    out.push((1, (x, y, 1, 1), turn));
                } else if fence && next {
                    out.push((1, (x, y, 1, 1), if turn == 0 { 180 } else { -90 }));
                } else if x1 < x + sx * 2 || y1 < y + sy * 2 || next {
                    out.push((3, (x, y, 1, 1), turn));
                } else if gate || !long || !clear {
                    out.push((4, (x, y, lw, lh), turn));
                    x += sx;
                    y += sy;
                    long = true;
                } else {
                    out.push((5, (x, y, lw, lh), turn));
                    x += sx;
                    y += sy;
                    gate = true;
                }
                fence = true;
            }
            x += 1;
        }
        y += 1;
    }
    out
}

/// An animal template's AnimalDescription, CharacterDescription, the raw materials it leaves
/// and its animation groups.
fn animal_kind(
    archive: &Archive,
    reference: &str,
    template: usize,
    templates: &mut Vec<Template>,
    wildlife: &mut banished_game::wildlife::Wildlife,
) {
    let refs = description_refs(archive, reference);
    let find = |k: &str| {
        refs.iter()
            .find(|(key, _)| key == k)
            .and_then(|(_, r)| read(archive, &normalise(r)))
    };
    let ad = find("animal").and_then(|d| banished_formats::game::animal_description(&d).ok());
    let cd = find("character").and_then(|d| banished_formats::game::character_description(&d).ok());
    let (Some(ad), Some(cd)) = (ad, cd) else {
        return;
    };
    let materials = ad
        .materials
        .iter()
        .flatten()
        .filter_map(|m| load_template(archive, m, templates))
        .collect();
    for (v, g) in templates[template].animations.iter().enumerate() {
        if let Some(g) = g {
            wildlife.groups.insert((template, v), g.clone());
        }
    }
    wildlife.animal_kinds.insert(
        template,
        banished_game::wildlife::AnimalKind {
            desc: ad,
            materials,
            character: cd,
        },
    );
}

/// A livestock template: its animal and its LivestockDescription (the product's pile
/// template resolved).
fn livestock_kind(
    archive: &Archive,
    reference: &str,
    templates: &mut Vec<Template>,
    wildlife: &mut banished_game::wildlife::Wildlife,
) -> Option<usize> {
    let t = load_template(archive, reference, templates)?;
    if wildlife.livestock_kinds.contains_key(&t) {
        return Some(t);
    }
    animal_kind(archive, reference, t, templates, wildlife);
    let refs = description_refs(archive, reference);
    let d = refs
        .iter()
        .find(|(key, _)| key == "livestock")
        .and_then(|(_, r)| read(archive, &normalise(r)))
        .and_then(|d| banished_formats::game::livestock_description(&d).ok())?;
    let product = d
        .product
        .as_deref()
        .and_then(|p| load_template(archive, p, templates));
    wildlife.livestock_kinds.insert(
        t,
        banished_game::wildlife::LivestockKind {
            product,
            interval: d.interval,
            group: d.group,
        },
    );
    Some(t)
}

/// `FUN_1400a5eb0`'s templates: each population's, its herd's and its animal's, with their
/// descriptions. Returns the population templates.
fn wildlife_kinds(
    archive: &Archive,
    populations: &[String],
    templates: &mut Vec<Template>,
    wildlife: &mut banished_game::wildlife::Wildlife,
) -> Vec<usize> {
    use banished_game::wildlife::{HerdKind, PopulationKind};
    let find = |refs: &[(String, String)], k: &str| {
        refs.iter()
            .find(|(key, _)| key == k)
            .and_then(|(_, r)| read(archive, &normalise(r)))
    };
    let mut out = Vec::new();
    for r in populations {
        let Some(pt) = load_template(archive, r, templates) else {
            continue;
        };
        let refs = description_refs(archive, r);
        let Some(pd) = find(&refs, "population")
            .and_then(|d| banished_formats::game::population_description(&d).ok())
        else {
            continue;
        };
        let herd = pd
            .herd
            .as_deref()
            .and_then(|h| load_template(archive, h, templates).map(|t| (h.to_string(), t)));
        if let Some((href, ht)) = &herd {
            let hrefs = description_refs(archive, href);
            let hd = find(&hrefs, "herd")
                .and_then(|d| banished_formats::game::herd_description(&d).ok());
            if let Some(hd) = hd {
                let animal = hd
                    .animal
                    .as_deref()
                    .and_then(|a| load_template(archive, a, templates).map(|t| (a.to_string(), t)));
                if let Some((aref, at)) = animal {
                    animal_kind(archive, &aref, at, templates, wildlife);
                    wildlife.herd_kinds.insert(
                        *ht,
                        HerdKind {
                            desc: hd,
                            animal: at,
                        },
                    );
                }
            }
        }
        wildlife.population_kinds.insert(
            pt,
            PopulationKind {
                desc: pd,
                herd: herd.map(|(_, t)| t),
            },
        );
        out.push(pt);
    }
    out
}

/// A floor's picking mesh in model space: each submesh through its bind hierarchy (Runtime
/// AddMesh, `180127b70`).
fn floor_triangles(mesh: &banished_formats::picking::PickingMesh) -> Vec<[[f32; 3]; 3]> {
    use banished_formats::skeleton::Transform;
    let root = Transform {
        rotation: [0.0, 0.0, 0.0, 1.0],
        translation: [0.0; 3],
    };
    let mut world: Vec<Transform> = Vec::with_capacity(mesh.meshes.len());
    let mut out = Vec::new();
    for (i, m) in mesh.meshes.iter().enumerate() {
        let t = if i == 0 {
            root
        } else {
            let [x, y, z, w] = m.rotation;
            world.get(m.node as usize).unwrap_or(&root).mul(&Transform {
                rotation: [-x, -y, -z, w],
                translation: m.translation,
            })
        };
        world.push(t);
        for tri in &m.triangles {
            out.push(tri.vertices.map(|v| t.apply(m.vertices[v as usize])));
        }
    }
    out
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
        fruit: None,
        growth: None,
        growth_reference: None,
        spawn: Vec::new(),
        updates: Vec::new(),
        size: None,
        storage: None,
        storage_location: false,
        storage_location_flags: 0,
        storage_display: false,
        market_radius: None,
        market_location: false,
        has_interact: false,
        use_points: Vec::new(),
        build_points: Vec::new(),
        use_yaws: Vec::new(),
        create_points: Vec::new(),
        named_points: Vec::new(),
        floor: None,
        floor_on_activate: false,
        terraform: None,
        tornado: None,
        quarry: None,
        trading_post: false,
        tracker: false,
        pile: None,
        pile_material: None,
        pile_yield: (0, 0),
    };
    let mut pile_ref = None;
    let mut fruit_ref = None;
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
            "fruit" => {
                if let Ok((months, best, range, pile)) =
                    banished_formats::game::fruit_description(&data)
                {
                    t.fruit = Some(banished_game::orchard::FruitTemplate {
                        months,
                        best,
                        range,
                        pile: None,
                    });
                    fruit_ref = pile;
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
            "storagedisplay" => t.storage_display = true,
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
                t.create_points = points(2);
                t.use_yaws = list.as_ref().map_or(Vec::new(), |l| {
                    banished_formats::game::interaction_points(l, 0)
                        .iter()
                        .map(|p| p.yaw)
                        .collect()
                });
                t.named_points = list.as_ref().map_or(Vec::new(), |l| {
                    l.points
                        .iter()
                        .filter(|p| !matches!(p.name.as_str(), "use" | "build" | "create"))
                        .map(|p| (p.name.clone(), p.position, p.yaw))
                        .collect()
                });
            }
            "floor" => {
                let d = banished_formats::game::floor_description(&data).ok();
                t.floor_on_activate = d.as_ref().is_some_and(|f| f.active);
                t.floor = d
                    .and_then(|f| f.mesh)
                    .and_then(|m| read(archive, &normalise(&m)))
                    .and_then(|d| banished_formats::picking::picking_mesh(&d).ok())
                    .map(|m| std::sync::Arc::new(floor_triangles(&m)))
            }
            "tracker" => t.tracker = true,
            "trade" => t.trading_post = true,
            "tornado" => {
                t.tornado = banished_formats::game::tornado_description(&data).ok().map(
                    |(speed, min_distance, reach, interval, throw)| {
                        banished_game::tornado::TornadoTemplate {
                            speed,
                            min_distance,
                            interval,
                            reach,
                            throw,
                            fade: desc
                                .descriptions
                                .iter()
                                .flatten()
                                .find(|d| d.ends_with(":particle"))
                                .and_then(|d| read(archive, d))
                                .and_then(|d| particle_description(&d).ok())
                                .and_then(|p| p.slots.first().map(|s| s.fade))
                                .unwrap_or(0),
                        }
                    },
                )
            }
            "terraform" => {
                let mesh = |m: &Option<String>| {
                    m.as_deref()
                        .and_then(|m| read(archive, &normalise(m)))
                        .and_then(|d| banished_formats::picking::picking_mesh(&d).ok())
                        .and_then(|m| m.meshes.first().map(|s| Arc::new(s.vertices.clone())))
                };
                t.terraform = banished_formats::game::terraform_description(&data)
                    .ok()
                    .map(|d| banished_game::terraform::TerraformTemplate {
                        floor: mesh(&d.floor),
                        floor_mask: d.floor_mask,
                        ceiling: mesh(&d.ceiling),
                        ceiling_mask: d.ceiling_mask,
                    })
            }
            "quarry" => {
                t.quarry = banished_formats::game::quarry_description(&data)
                    .ok()
                    .and_then(|d| {
                        let mesh = d
                            .mesh
                            .as_deref()
                            .and_then(|m| read(archive, &normalise(m)))
                            .and_then(|d| banished_formats::picking::picking_mesh(&d).ok())?;
                        Some(banished_game::quarry::QuarryTemplate {
                            shallow: d.shallow,
                            deep: d.deep,
                            vertices: Arc::new(mesh.meshes.first()?.vertices.clone()),
                        })
                    })
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
    if let Some(r) = fruit_ref {
        let pile = load_template(archive, &r, table);
        if let Some(f) = &mut table[index].fruit {
            f.pile = pile;
        }
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
            pile_spacing: d.value_4c,
            pile_amount: d.value_40,
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

/// Each citizen variant's mesh nodes and the index of its `tool` node.
fn citizen_tool_nodes(
    archive: &Archive,
    model: &banished_formats::game::ModelDescription,
) -> Vec<Option<(Vec<banished_formats::types::Bone>, usize)>> {
    model
        .variants
        .iter()
        .map(|v| {
            let mesh = v.meshes.first().cloned().flatten()?;
            let m = banished_formats::types::graphics_mesh(&read(archive, &mesh)?).ok()?;
            let tool = m.bones.iter().position(|b| b.name == "tool")?;
            Some((m.bones, tool))
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

/// The CitizenDescription's ages and education (`citizen` is the profile's citizen template).
fn citizen_rules(archive: &Archive, citizen: &str) -> Option<CitizenRules> {
    read(
        archive,
        &normalise(citizen).replace(":resource", ":citizen"),
    )
    .and_then(|d| citizen_description(&d).ok())
    .map(|c| CitizenRules {
        adult_age: c.adult_age(),
        start_age: c.start_age(),
        lifespan: c.lifespan(),
        education: c.i(0x118),
    })
}

fn households(
    archive: &Archive,
    profile: &MapGenerator,
    start: &[f32; 3],
    rng: &mut Random,
) -> (Vec<Citizen>, CitizenRules) {
    let rules = citizen_rules(archive, &profile.citizen);
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
    wildlife: banished_game::wildlife::Wildlife,
    known: Vec<usize>,
    known_materials: Vec<usize>,
    water: banished_game::fire::Water,
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
    /// The nomad templates' NomadDescription materials (men, women).
    nomad_materials: HashMap<usize, [Option<String>; 2]>,
    /// ShowTool's meshes: CitizenDescription `+0x138` (`FUN_14011c880`), and each citizen
    /// mesh's nodes with its `tool` node (`GraphicsMesh::GetNodeIndexFromName`).
    tool_meshes: Vec<Option<String>>,
    citizen_bones: Vec<Option<(Vec<banished_formats::types::Bone>, usize)>>,
    /// Carried goods' meshes and scales by raw material (its template's first mesh,
    /// RawMaterialDescription `+0x50`), filled when the models are built.
    carry_models: std::cell::RefCell<HashMap<usize, Option<(String, f32)>>>,
    /// Each variant's animation group (`Models\Citizen\CitizenAnims.rsc`).
    pub(crate) citizen_anims: Vec<Option<Arc<banished_formats::types::AnimationGroup>>>,
    /// The citizens' CharacterComponent and CitizenAIComponent (docs/CITIZEN-AI.md).
    pub agents: Agents,
    /// Wild animals, their herds and populations (docs/WILDLIFE-POPULATION.md).
    pub wildlife: banished_game::wildlife::Wildlife,
    /// The placement tool's ghost (`PlacementTool +0x68`): mesh and transform.
    pub ghost: Option<(String, [f32; 16])>,
    /// The job manager, professions, workplaces and construction sites (docs/JOBS.md).
    pub construction: Construction,
    /// `Game/Names/Male` and `Female` (`FUN_1400aa130`), for births.
    names: Option<(NameDatabase, NameDatabase)>,
    /// Each StorageDisplay's cells on the ground, by rows (presentation only).
    display_cells: BTreeMap<usize, Vec<[f32; 3]>>,
    /// For a town loaded from a save: the saved object each entity and citizen came from, so
    /// saving writes the town back into them (docs/SAVE-WRITING.md).
    pub save_ids: Option<SaveIds>,
}

/// The saved objects behind a loaded town: per placed entity its ComponentEntity, per citizen
/// its CitizenComponent.
#[derive(Debug, Clone, Default)]
pub struct SaveIds {
    pub entities: Vec<Option<banished_formats::savegame::ObjId>>,
    pub citizens: Vec<banished_formats::savegame::ObjId>,
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
        // The map's setup files the shore cells as water sources (`FUN_1400a43f0`).
        let water = banished_game::fire::Water::new(&cells);
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
        let mut wildlife = banished_game::wildlife::Wildlife::default();
        let populations =
            wildlife_kinds(archive, &profile.populations, &mut templates, &mut wildlife);
        for t in populations {
            banished_game::wildlife::Wild {
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
                banished_game::wildlife::Wild {
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
            .and_then(|d| banished_formats::game::model_description(&d).ok());
        let citizen_ref = normalise(citizen);
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
                water_distance: c.i(0xf8),
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
        let mut agents = Agents::new(&citizens, &cells, character, ai_rules, carried, |c| {
            if c.is_adult(&rules) {
                1.0
            } else {
                CHILD_SCALE
            }
        });
        if let Some(d) = read(archive, &citizen_ref.replace(":resource", ":citizen"))
            .and_then(|d| citizen_description(&d).ok())
        {
            agents.life = banished_game::life::LifeRules::new(&d);
        }
        agents.diseases = read(archive, "Game/Disease/Diseases.rsc:resource")
            .and_then(|d| banished_formats::game::external_list(&d).ok())
            .unwrap_or_default()
            .iter()
            .filter_map(|r| read(archive, &normalise(r)))
            .filter_map(|d| banished_formats::game::disease(&d).ok())
            .collect();
        agents.carry = read(archive, "Game/Profession/Profession.rsc:laborer")
            .and_then(|d| banished_formats::game::profession(&d).ok())
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
            tool_meshes: read(archive, &citizen_ref.replace(":resource", ":citizen"))
                .and_then(|d| citizen_description(&d).ok())
                .map(|c| c.list)
                .unwrap_or_default(),
            citizen_bones: citizen_model
                .as_ref()
                .map(|m| citizen_tool_nodes(archive, m))
                .unwrap_or_default(),
            carry_models: Default::default(),
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
            let rect = banished_game::placement::Rect {
                x0: p.cell.0,
                y0: p.cell.1,
                x1: p.cell.0 + p.footprint.0 - 1,
                y1: p.cell.1 + p.footprint.1 - 1,
            };
            let degrees = p.yaw as i32;
            if let Some(map) = map {
                let rules = banished_game::placement::PlacementRules {
                    mask: 0,
                    masks: Vec::new(),
                    mask_chars: String::new(),
                    footprint: map.footprint.clone(),
                    width: 0,
                    height: 0,
                    cell_type: map.cell_type as u8,
                };
                if !map.no_stamp {
                    banished_game::placement::stamp(&mut self.cells, &rules, rect, degrees, e);
                }
                banished_game::placement::zone(&mut self.cells, &rules, rect, degrees, e);
            }
        }
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

    /// StatusIconComponent billboards (`FUN_1401310a0` anchors): each building with status bits
    /// ([`StartingTown::building_icons`]), and each citizen's above its model (position + model
    /// box height).
    pub fn status_icons(&self) -> Vec<crate::status_icons::StatusIcon> {
        let mut icons = self.building_icons();
        let Some(d) = &self.citizen_icons else {
            return icons;
        };
        let Some(sheet) = d.sheet.clone() else {
            return icons;
        };
        icons.extend(
            self.citizens
                .iter()
                .zip(&self.agents.states)
                .zip(&self.agents.characters)
                .filter(|((c, s), _)| c.alive && s.status != 0)
                .map(|((c, s), ch)| crate::status_icons::StatusIcon {
                    anchor: glam::Vec3::from(ch.position)
                        + glam::Vec3::Z * self.citizen_heights[usize::from(!c.male)],
                    mask: s.status,
                    sheet: sheet.clone(),
                    size: d.size,
                    screen_constant: d.screen_constant,
                }),
        );
        icons
    }

    /// The icon sheets `status_icons` refers to.
    pub fn icon_sheet_refs(&self) -> Vec<String> {
        let mut refs: Vec<String> = self
            .citizen_icons
            .iter()
            .chain(
                self.construction
                    .templates
                    .iter()
                    .filter_map(|t| t.status_icon.as_ref()),
            )
            .filter_map(|d| d.sheet.clone())
            .collect();
        refs.sort();
        refs.dedup();
        refs
    }

    /// The buildings' StatusIconComponents with bits set (`FUN_1401310a0`): above the model
    /// (its position plus its height), or for a MultiObject (or without a model) the map
    /// rectangle's centre cell on the ground plus 1.
    fn building_icons(&self) -> Vec<crate::status_icons::StatusIcon> {
        let mut out = Vec::new();
        for (&e, site) in &self.construction.sites {
            let t = &self.construction.templates[site.template];
            let Some(d) = &t.status_icon else { continue };
            let Some(sheet) = d.sheet.clone() else {
                continue;
            };
            let mut mask = self
                .construction
                .workforce
                .workplaces
                .get(&e)
                .map_or(0, |w| w.status);
            if let Some(h) = self.construction.homes.get(&e) {
                mask |= h.status;
            }
            if mask == 0 || !self.placed[e].alive {
                continue;
            }
            let p = &self.placed[e];
            let anchor = match t.icon_height.filter(|_| site.parts.is_empty()) {
                Some(h) => glam::Vec3::from(p.position) + glam::Vec3::Z * h,
                None => {
                    let (x, y) = p.cell;
                    let (w, h) = p.footprint;
                    let n = self.cells.size as i32;
                    let half = (n >> 1) as f32 - 0.5;
                    let (cx, cy) = ((x + (w >> 1)) as f32 - half, (y + (h >> 1)) as f32 - half);
                    glam::Vec3::new(cx, cy, p.position[2] + 1.0)
                }
            };
            out.push(crate::status_icons::StatusIcon {
                anchor,
                mask,
                sheet,
                size: d.size,
                screen_constant: d.screen_constant,
            });
        }
        out
    }

    /// `FUN_1400c9d70`: register a new entity in its template's bucket.
    pub fn register_entity(&mut self, id: usize) {
        let handle = self.scheduler.register(id, self.placed[id].bucket);
        debug_assert_eq!(handle, id);
    }

    /// A removal marker (`FUN_1400c8bf0` of `CreateRemovalDescription +0x38`, its MapComponent
    /// at `cell`, 1 × 1, then `FUN_1400c9d70`): registered, its ZoneComponent occupies the cell
    /// and its StateMachine is activated before the next iteration.
    pub fn add_marker(
        &mut self,
        archive: &Archive,
        template: usize,
        cell: (i32, i32),
        terrain: &HeightGrid,
    ) -> usize {
        let n = self.cells.size as i32;
        let half = (n >> 1) as f32 - 0.5;
        let (px, py) = (cell.0 as f32 - half, cell.1 as f32 - half);
        let mut normal = [0.0; 3];
        let z = terrain.ground(px, py, &mut normal);
        let id = self.placed.len();
        self.placed.push(Placed {
            template,
            bucket: self.templates[template].bucket,
            cell,
            footprint: (1, 1),
            position: [px, py, z],
            yaw: 0.0,
            variant: 0,
            mesh: usize::MAX,
            growth: 0.0,
            growth_state: None,
            spawn_state: None,
            alive: true,
            removal_requested: false,
            animation: None,
            scale: [1.0; 4],
        });
        self.register_entity(id);
        let i = (cell.1 * n + cell.0) as usize;
        self.cells.occupants[i] = Some(id);
        self.add_site(archive, id);
        id
    }

    /// A dragged area (CreateDrag: fields, orchards, the storage yard) over `rect` (`x, y, w,
    /// h`): the entity, its zone over every cell, and its site (DragPlacementTool's apply,
    /// `FUN_140100dd0`, through `removal_tool`; tests also place areas directly).
    pub fn place_area(
        &mut self,
        archive: &Archive,
        reference: &str,
        rect: (i32, i32, i32, i32),
        terrain: &HeightGrid,
    ) -> Option<usize> {
        let template = self.load_template(archive, reference)?;
        let (x, y, w, h) = rect;
        let n = self.cells.size as i32;
        let half = (n >> 1) as f32 - 0.5;
        let (px, py) = (
            x as f32 + (w - 1) as f32 / 2.0 - half,
            y as f32 + (h - 1) as f32 / 2.0 - half,
        );
        let mut normal = [0.0; 3];
        let z = terrain.ground(px, py, &mut normal);
        let id = self.placed.len();
        self.placed.push(Placed {
            template,
            bucket: self.templates[template].bucket,
            cell: (x, y),
            footprint: (w, h),
            position: [px, py, z],
            yaw: 0.0,
            variant: 0,
            mesh: usize::MAX,
            growth: 0.0,
            growth_state: None,
            spawn_state: None,
            alive: true,
            removal_requested: false,
            animation: None,
            scale: [1.0; 4],
        });
        self.register_entity(id);
        for cy in y..y + h {
            for cx in x..x + w {
                self.cells.occupants[(cy * n + cx) as usize] = Some(id);
            }
        }
        self.add_site(archive, id);
        Some(id)
    }

    /// The fenced tool (FencedTool, vtable `0x140166a10`) dragged from cell `a` to cell `b`
    /// with the shelter turned `rotation` quarter turns: slot `+0x70` (`FUN_1400e0960`) checks
    /// the area, slot `+0x68` (`FUN_1400e02d0`) places the entity over it with its shelter,
    /// corners and fence runs (its MultiObjectComponent's parts). The drag runs in
    /// `removal_tool`'s fenced mode. Returns the entity.
    pub fn place_fenced(
        &mut self,
        archive: &Archive,
        reference: &str,
        a: (i32, i32),
        b: (i32, i32),
        rotation: i32,
        terrain: &HeightGrid,
    ) -> Option<usize> {
        let template = self.load_template(archive, reference)?;
        let key = normalise(reference);
        let d = read(archive, &key.replace(":resource", ":createfenced"))
            .and_then(|d| banished_formats::game::create_fenced_description(&d).ok())?;
        let mut parts = Vec::with_capacity(6);
        for p in &d.parts {
            parts.push(p.as_deref().and_then(|p| self.load_template(archive, p)));
        }
        let clear = d
            .clear
            .as_deref()
            .and_then(|c| self.load_template(archive, c));
        let shelter = d.parts.first().cloned().flatten()?;
        let size = read(
            archive,
            &normalise(&shelter).replace(":resource", ":createplaced"),
        )
        .and_then(|d| banished_formats::game::create_placed_description(&d).ok())
        .map(|c| (c.width, c.height))?;
        let n = self.cells.size as i32;
        let (x0, x1) = (a.0.min(b.0), a.0.max(b.0));
        let (y0, y1) = (a.1.min(b.1), a.1.max(b.1));
        let (w, h) = (x1 - x0 + 1, y1 - y0 + 1);
        // `FUN_1400e0960`: the size, then every cell: of an allowed type, no occupant but the
        // clear template's, no zone, and not too high.
        if w < d.min.0 || h < d.min.1 || d.max.0 < w || d.max.1 < h {
            return None;
        }
        for y in y0..=y1 {
            for x in x0..=x1 {
                if !self.fenced_cell_ok(&d, clear, x, y, terrain) {
                    return None;
                }
            }
        }
        let (s, yaw, centre) = fenced_shelter(&d, size, (x0, y0, x1, y1), rotation, |x, y| {
            cell_ground(self.cells.size, terrain, y * n + x)
        })?;
        let mut degrees = yaw as i32;
        while degrees < 0 {
            degrees += 360;
        }
        // `FUN_1400c8bf0` of the tool's template, its MapComponent over the area.
        let half = (n >> 1) as f32 - 0.5;
        let (px, py) = (
            x0 as f32 + (w - 1) as f32 / 2.0 - half,
            y0 as f32 + (h - 1) as f32 / 2.0 - half,
        );
        let mut normal = [0.0; 3];
        let z = terrain.ground(px, py, &mut normal);
        let entity = self.placed.len();
        self.placed.push(Placed {
            template,
            bucket: self.templates[template].bucket,
            cell: (x0, y0),
            footprint: (w, h),
            position: [px, py, z],
            yaw: degrees as f32,
            variant: 0,
            mesh: usize::MAX,
            growth: 0.0,
            growth_state: None,
            spawn_state: None,
            alive: true,
            removal_requested: false,
            animation: None,
            scale: [1.0; 4],
        });
        // `FUN_140106da0`: the clear template's entities in the area go.
        if let Some(c) = clear {
            for y in y0..=y1 {
                for x in x0..=x1 {
                    if let Some(o) = self.cells.occupants[(y * n + x) as usize] {
                        if self.placed[o].template == c && !self.construction.destroy.contains(&o) {
                            self.construction.destroy.push(o);
                        }
                    }
                }
            }
        }
        let mut multi = Vec::new();
        // The shelter, turned with the tool.
        if let Some(t) = parts[0] {
            let rect = (s.0, s.1, s.2 - s.0 + 1, s.3 - s.1 + 1);
            multi.push(self.fence_part(t, rect, degrees, centre));
        }
        // `FUN_1400e1150` kind 2: a corner at each corner outside the shelter.
        let outside = |x: i32, y: i32| x < s.0 || s.2 < x || y < s.1 || s.3 < y;
        for (x, y, turn) in [(x0, y0, 90), (x0, y1, 0), (x1, y1, -90), (x1, y0, 180)] {
            if outside(x, y) {
                if let Some(p) = self.fence_piece(parts[2], (x, y, 1, 1), turn, terrain) {
                    multi.push(p);
                }
            }
        }
        // `FUN_1400e0d90`: the four sides between the corners.
        for (from, to, turn) in [
            ((x0 + 1, y0), (x1 - 1, y0), 0),
            ((x0 + 1, y1), (x1 - 1, y1), 0),
            ((x0, y0 + 1), (x0, y1 - 1), 90),
            ((x1, y0 + 1), (x1, y1 - 1), 90),
        ] {
            for (kind, rect, t) in fence_run(from, to, s, turn, d.centered) {
                if let Some(p) = self.fence_piece(parts[kind], rect, t, terrain) {
                    multi.push(p);
                }
            }
        }
        // `FUN_1400c9d70` of the entity last: its zone and its site.
        self.register_entity(entity);
        for cy in y0..=y1 {
            for cx in x0..=x1 {
                self.cells.occupants[(cy * n + cx) as usize] = Some(entity);
            }
        }
        self.add_site(archive, entity);
        if let Some(site) = self.construction.sites.get_mut(&entity) {
            site.parts = multi;
            // MultiObjectComponent `+0x58`: the border's cells.
            site.multi_stages = Some((x1 - x0 + 1 + y1 - y0 + 1) * 2 - 4);
        }
        Some(entity)
    }

    /// `FUN_1400e0960`'s test of one cell: on the map, of an allowed type, no occupant but the
    /// clear template's, no zone, and not too high.
    pub fn fenced_cell_ok(
        &self,
        d: &banished_formats::game::CreateFencedDescription,
        clear: Option<usize>,
        x: i32,
        y: i32,
        terrain: &HeightGrid,
    ) -> bool {
        let n = self.cells.size as i32;
        if x < 0 || n <= x || y < 0 || n <= y {
            return false;
        }
        let i = (y * n + x) as usize;
        if !d.mask & (1 << (self.cells.types[i].0 & 0x1f)) != 0 {
            return false;
        }
        if let Some(o) = self.cells.occupants[i] {
            if clear.is_none() || Some(self.placed[o].template) != clear {
                return false;
            }
        }
        if self.cells.flags[i] != 0 {
            return false;
        }
        let z = cell_ground(self.cells.size, terrain, i as i32)[2];
        z.abs() <= 100.0
    }

    /// `FUN_1400e1150`: a part over `rect` (`x, y, w, h`) at the middle of its corner cells'
    /// ground, turned `degrees`.
    fn fence_piece(
        &mut self,
        template: Option<usize>,
        rect: (i32, i32, i32, i32),
        degrees: i32,
        terrain: &HeightGrid,
    ) -> Option<usize> {
        let t = template?;
        let n = self.cells.size as i32;
        let (x, y, w, h) = rect;
        let g0 = cell_ground(self.cells.size, terrain, n * y + x);
        let g1 = cell_ground(self.cells.size, terrain, n * (y + h - 1) + x + w - 1);
        let centre = [
            (g0[0] + g1[0]) * 0.5,
            (g0[1] + g1[1]) * 0.5,
            (g0[2] + g1[2]) * 0.5,
        ];
        let mut degrees = degrees;
        while degrees < 0 {
            degrees += 360;
        }
        Some(self.fence_part(t, rect, degrees, centre))
    }

    /// A MultiObject part (`FUN_1400c8bf0`, its MapComponent and model, `FUN_1400c9d70`): it
    /// stamps the map with its parent (entity event 1).
    fn fence_part(
        &mut self,
        template: usize,
        rect: (i32, i32, i32, i32),
        degrees: i32,
        position: [f32; 3],
    ) -> usize {
        let t = &self.templates[template];
        let (variant, mesh) = t.model.as_ref().map_or((0, 0), |m| {
            (m.variant.max(0) as usize, m.mesh.max(0) as usize)
        });
        let id = self.placed.len();
        self.placed.push(Placed {
            template,
            bucket: t.bucket,
            cell: (rect.0, rect.1),
            footprint: (rect.2, rect.3),
            position,
            yaw: degrees as f32,
            variant,
            mesh,
            growth: 0.0,
            growth_state: None,
            spawn_state: None,
            alive: true,
            removal_requested: false,
            animation: None,
            scale: [1.0; 4],
        });
        self.register_entity(id);
        id
    }

    /// The animals' context over the town.
    #[cfg(test)]
    pub fn with_wild<R>(
        &mut self,
        terrain: &HeightGrid,
        rng: &mut Random,
        f: impl FnOnce(&mut banished_game::wildlife::Wild) -> R,
    ) -> R {
        let mut wild = banished_game::wildlife::Wild {
            wildlife: &mut self.wildlife,
            placed: &mut self.placed,
            templates: &self.templates,
            cells: &self.cells,
            terrain,
            rng,
            scheduler: &mut self.scheduler,
        };
        f(&mut wild)
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
        // The wild animals' CharacterComponent (the bit-0x04 phase).
        self.agents.move_animals(
            &mut self.wildlife,
            &self.cells,
            terrain,
            rng,
            &self.materials,
            dt,
        );
        let wildlife = &mut self.wildlife;
        let fruits = &mut self.construction.fruits;
        let tombstones = &mut self.construction.tombstones;
        let calendar = self.construction.calendar;
        let changed = Seeder {
            templates: &self.templates,
            terrain,
            cells: &mut self.cells,
            rng,
            placed: &mut self.placed,
            removed: &mut self.removed,
            animation_events: &mut self.animation_events,
        }
        .iterate_with(
            &mut self.scheduler,
            dt,
            temperature,
            false,
            &mut |seeder, scheduler, id, elapsed| {
                // TombstoneComponent (`FUN_140058130`).
                if tombstones.contains_key(&id) {
                    return banished_game::cemetery::tombstone_visit(
                        tombstones,
                        seeder.cells,
                        seeder.placed,
                        id,
                        elapsed,
                    );
                }
                // FruitComponent (`FUN_14004a740`) after the tree's growth.
                let p = &seeder.placed[id];
                if let Some(f) = seeder.templates[p.template].fruit {
                    let fruit = fruits.entry(id).or_default();
                    banished_game::orchard::ripen(
                        fruit,
                        &f,
                        p.growth,
                        elapsed,
                        calendar,
                        temperature,
                    );
                    return false;
                }
                let mut wild = banished_game::wildlife::Wild {
                    wildlife: &mut *wildlife,
                    placed: seeder.placed,
                    templates: seeder.templates,
                    cells: seeder.cells,
                    terrain: seeder.terrain,
                    rng: seeder.rng,
                    scheduler,
                };
                wild.scheduled(id, elapsed);
                false
            },
        );
        // The entity manager deletes the queued animals.
        let buried = banished_game::wildlife::Wild {
            wildlife: &mut self.wildlife,
            placed: &mut self.placed,
            templates: &self.templates,
            cells: &self.cells,
            terrain,
            rng,
            scheduler: &mut self.scheduler,
        }
        .bury();
        changed | buried
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
                    .filter(|_| c.alive)
            })
            .collect();
        let mut env = banished_game::town::Env {
            wildlife: &mut self.wildlife,
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
        let rules = self.rules;
        let names = self.names.as_ref();
        // Citizens made from nomads take a name, appearance and lifespan as newborns do.
        let mut newborn = |rng: &mut Random, model: &Citizen, male: bool| match names {
            Some((m, f)) => banished_game::citizens::newborn(rng, &rules, m, f, model, male),
            None => Citizen {
                male,
                alive: true,
                ..model.clone()
            },
        };
        self.agents
            .apply_commands(&mut self.citizens, &mut env, &groups, &mut newborn);
        self.apply_storage_changes();
        // Remove Roads' markers on built road cells (`FUN_140108ab0`).
        for (cell, marker) in std::mem::take(&mut self.construction.road_markers) {
            let n = self.cells.size as i32;
            self.add_cached_marker(marker, (cell % n, cell / n), terrain);
        }
    }

    /// StockManager::Update's commute optimisation (`banished_game::commute`) by `dt` seconds.
    pub fn update_commute(&mut self, terrain: &HeightGrid, rng: &mut Random, dt: f32) {
        let groups: Vec<_> = self
            .citizens
            .iter()
            .map(|c| {
                self.citizen_anims
                    .get(usize::from(!c.male))
                    .and_then(Option::as_deref)
                    .filter(|_| c.alive)
            })
            .collect();
        let mut env = banished_game::town::Env {
            wildlife: &mut self.wildlife,
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
        self.agents.update_commute(
            &self.citizens,
            &mut env,
            &groups,
            banished_game::scheduler::ticks(dt),
        );
    }

    /// [`Self::add_marker`] for a marker whose construction template is loaded.
    fn add_cached_marker(&mut self, template: usize, cell: (i32, i32), terrain: &HeightGrid) {
        let reference = self.templates[template].reference.clone();
        let Some(st) = self
            .construction
            .templates
            .iter()
            .position(|t| t.reference == reference)
        else {
            return;
        };
        let n = self.cells.size as i32;
        let half = (n >> 1) as f32 - 0.5;
        let (px, py) = (cell.0 as f32 - half, cell.1 as f32 - half);
        let mut normal = [0.0; 3];
        let z = terrain.ground(px, py, &mut normal);
        let id = self.placed.len();
        self.placed.push(Placed {
            template,
            bucket: self.templates[template].bucket,
            cell,
            footprint: (1, 1),
            position: [px, py, z],
            yaw: 0.0,
            variant: 0,
            mesh: usize::MAX,
            growth: 0.0,
            growth_state: None,
            spawn_state: None,
            alive: true,
            removal_requested: false,
            animation: None,
            scale: [1.0; 4],
        });
        self.register_entity(id);
        let i = (cell.1 * n + cell.0) as usize;
        self.cells.occupants[i] = Some(id);
        self.add_site_from(st, id);
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
                    .filter(|_| c.alive)
            })
            .collect();
        let mut env = banished_game::town::Env {
            wildlife: &mut self.wildlife,
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
        let rules = self.rules;
        let names = self.names.as_ref();
        let mut newborn = |rng: &mut Random, mother: &Citizen, male: bool| match names {
            Some((m, f)) => banished_game::citizens::newborn(rng, &rules, m, f, mother, male),
            None => Citizen {
                male,
                alive: true,
                ..mother.clone()
            },
        };
        self.agents
            .update(&mut self.citizens, &mut env, &groups, dt, &mut newborn);
        // `FUN_1400c9d70`: piles left by cleared resources join the entity manager's buckets.
        for id in before..self.placed.len() {
            self.register_entity(id);
        }
        self.apply_rebuilds();
        self.apply_storage_changes();
        self.place_displays(terrain);
        !self.citizens.is_empty()
    }

    /// The ground under each StorageDisplay's cells (`FUN_140057090`'s ground points), for
    /// the entities that have none yet.
    fn place_displays(&mut self, terrain: &HeightGrid) {
        for (e, p) in self.placed.iter().enumerate() {
            if !p.alive
                || !self.templates[p.template].storage_display
                || self.display_cells.contains_key(&e)
            {
                continue;
            }
            let n = self.cells.size as i32;
            let (w, h) = p.footprint;
            let cells = (0..w * h)
                .map(|i| {
                    let cell = (p.cell.1 + i / w) * n + p.cell.0 + i % w;
                    cell_ground(self.cells.size, terrain, cell)
                })
                .collect();
            self.display_cells.insert(e, cells);
        }
    }

    /// What DestroyComponents put in torn-down buildings' places (`FUN_1401215f0`): the
    /// template over the same map rectangle, turned and placed as the old model, its zone
    /// reserving the footprint; a site is activated before the next iteration, anything else
    /// stamps the map now.
    fn apply_rebuilds(&mut self) {
        for r in std::mem::take(&mut self.construction.rebuilds) {
            let id = self.placed.len();
            let t = &self.templates[r.template];
            let (bucket, reference) = (t.bucket, t.reference.clone());
            let footprint = t
                .map
                .as_ref()
                .map(|m| m.footprint.clone())
                .unwrap_or_default();
            let cell_type = t.map.as_ref().map_or(0, |m| m.cell_type as u8);
            let stamps = t.map.as_ref().is_some_and(|m| !m.no_stamp);
            let mesh = t.model.as_ref().map(|m| m.mesh.max(0) as usize);
            self.placed.push(Placed {
                template: r.template,
                bucket,
                cell: r.cell,
                footprint: r.footprint,
                position: r.position,
                yaw: r.yaw,
                variant: r.variant,
                mesh: usize::MAX,
                growth: 0.0,
                growth_state: None,
                spawn_state: None,
                alive: true,
                removal_requested: false,
                animation: None,
                scale: [1.0; 4],
            });
            self.register_entity(id);
            let rules = banished_game::placement::PlacementRules {
                mask: 0,
                masks: Vec::new(),
                mask_chars: String::new(),
                footprint,
                width: 0,
                height: 0,
                cell_type,
            };
            let rect = banished_game::placement::Rect {
                x0: r.cell.0,
                y0: r.cell.1,
                x1: r.cell.0 + r.footprint.0 - 1,
                y1: r.cell.1 + r.footprint.1 - 1,
            };
            let degrees = r.yaw as i32;
            banished_game::placement::zone(&mut self.cells, &rules, rect, degrees, id);
            if let Some(st) = self
                .construction
                .templates
                .iter()
                .position(|t| t.reference == reference)
            {
                self.add_site_from(st, id);
            } else {
                // A plain object: its model's mesh, its MapComponent stamped.
                if let Some(mesh) = mesh {
                    self.placed[id].mesh = mesh;
                }
                if stamps {
                    banished_game::placement::stamp(&mut self.cells, &rules, rect, degrees, id);
                }
            }
            self.construction.models_dirty = true;
        }
    }

    /// StorageLocationComponent's state 2 and 3 enters (`FUN_140132700` / `FUN_1401326c0`):
    /// a site's storage joins the town's (appended) or leaves it.
    fn apply_storage_changes(&mut self) {
        for (e, join) in std::mem::take(&mut self.construction.storage_changes) {
            if !join {
                self.storage_sites.retain(|s| s.entity != e);
                continue;
            }
            let Some(site) = self.construction.sites.get(&e) else {
                continue;
            };
            if self.storage_sites.iter().any(|s| s.entity == e) {
                continue;
            }
            let t = &self.templates[self.placed[e].template];
            self.storage_sites.push(StorageSite {
                entity: e,
                storage: site.storage,
                location_flags: t.storage_location_flags,
                // A MarketLocationComponent's market (`+0x10`), searched within its radius.
                radius: if t.market_location {
                    t.market_radius
                } else {
                    None
                },
            });
        }
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
            if citizen.alive && citizen.is_adult(&self.rules) && citizen.school.is_none() {
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
                        let m = m?;
                        let pile = self.load_template(archive, &m);
                        let m = material_index(archive, &mut self.materials, &m)?;
                        t.materials.push((n, m));
                        t.material_templates.push(pile);
                    }
                }
                "well" => t.controllers.push(Controller::Well),
                "trade" => {
                    if let Ok(d) = banished_formats::game::trade_description(&data) {
                        let boat = d.boat.as_deref().and_then(|r| self.load_boat(archive, r));
                        if let Some(boat) = boat {
                            self.load_trade_goods(archive);
                            t.trade = Some(banished_game::trade::TradeTemplate {
                                boat,
                                merchants: d.merchants.clone(),
                                rows: d.rows.clone(),
                                categories: d.categories.iter().map(|c| c.0).collect(),
                                labels: d
                                    .categories
                                    .iter()
                                    .map(|c| (c.1.clone(), c.2.clone()))
                                    .collect(),
                                sprites: d.sprites.clone(),
                                strings: d.strings.clone(),
                            });
                            t.trade_risk = Some((d.value_68, d.value_6c));
                            t.controllers.push(Controller::Trade);
                        }
                    }
                }
                "immigration" => {
                    if let Ok(d) = banished_formats::game::immigration_description(&data) {
                        let nomad = d.nomad.as_deref().and_then(|r| self.load_nomad(archive, r));
                        let required = d
                            .required
                            .iter()
                            .flatten()
                            .filter_map(|r| self.load_template(archive, r))
                            .collect();
                        if let Some(nomad) = nomad {
                            t.immigration = Some(banished_game::immigration::ImmigrationTemplate {
                                nomad,
                                population: d.population,
                                required,
                                share: d.share,
                                wait: d.wait,
                                months: d.months,
                                leave: 7,
                                margin: 10,
                                risk: d.risk,
                                risk_months: d.risk_months,
                            });
                            t.controllers.push(Controller::Immigration);
                        }
                    }
                }
                name if name.starts_with("fire") => {
                    t.fire = banished_formats::game::fire_description(&data).ok();
                }
                "multiobject" => t.controllers.push(Controller::MultiObject),
                "destroy" => {
                    t.controllers.push(Controller::Destroy);
                    let d = banished_formats::game::destroy_description(&data).ok()?;
                    let replace = d
                        .replace
                        .as_deref()
                        .and_then(|r| self.load_template(archive, r));
                    // A replacement that is a site itself has its construction template ready
                    // for when it is placed.
                    if let Some(r) = replace {
                        if r != template {
                            self.site_template(archive, r);
                        }
                    }
                    t.destroy = Some(banished_game::destroy::DestroyTemplate {
                        replace,
                        material: d.material,
                        decal: d.decal,
                        min_mesh: d.min_mesh,
                    });
                }
                "attendance" => {
                    t.controllers.push(Controller::Attendance);
                    t.attendance = banished_formats::game::attendance_description(&data).ok();
                }
                "healer" => t.controllers.push(Controller::Healer),
                "residence" => {
                    t.controllers.push(Controller::Residence);
                    t.residence = banished_formats::game::residence_description(&data).ok();
                }
                "clearresource" => t.controllers.push(Controller::ClearResource),
                "herbalist" => {
                    let Ok(d) = banished_formats::game::herbalist_description(&data) else {
                        continue;
                    };
                    t.controllers.push(Controller::ResourceProduce);
                    let mut resources = Vec::new();
                    for r in d.resources.iter().flatten() {
                        resources.push(self.load_template(archive, r)?);
                    }
                    t.herbalist = Some(banished_game::herbalist::HerbalistTemplate {
                        resources,
                        group: d.group,
                    });
                }
                "radius" => {
                    t.radius = banished_formats::game::radius_description(&data)
                        .ok()
                        .map(|(r, _)| r);
                }
                "happiness" => {
                    if let Ok(d) = banished_formats::game::happiness_description(&data) {
                        t.controllers.push(Controller::Happiness);
                        t.happiness = Some(d);
                    }
                }
                "cropfield" => {
                    if let Ok((tend, group)) = banished_formats::game::crop_field_description(&data)
                    {
                        t.controllers.push(Controller::CropField);
                        t.crop_field = Some(banished_game::farm::CropFieldTemplate { tend, group });
                    }
                }
                "absorbresource" => t.controllers.push(Controller::AbsorbResource),
                "marketlocation" => t.controllers.push(Controller::MarketLocation),
                "market" => {
                    if let Ok(d) = banished_formats::game::market_description(&data) {
                        t.controllers.push(Controller::ResourceProduce);
                        t.market = Some(d);
                    }
                }
                "statusicon" => {
                    t.status_icon = banished_formats::game::status_icon_description(&data).ok();
                }
                "cemetery" => {
                    let tombstone = banished_formats::game::cemetery_description(&data)
                        .ok()
                        .flatten()
                        .and_then(|r| {
                            let stone = self.load_template(archive, &r)?;
                            let life =
                                read(archive, &normalise(&r).replace(":resource", ":tombstone"))
                                    .and_then(|d| {
                                        banished_formats::game::tombstone_description(&d).ok()
                                    })
                                    .unwrap_or((60.0, 24.0));
                            Some((stone, life))
                        });
                    if let Some((stone, (life, range))) = tombstone {
                        t.cemetery = Some(banished_game::cemetery::CemeteryTemplate {
                            tombstone: stone,
                            life,
                            range,
                        });
                        t.controllers.push(Controller::Cemetery);
                    }
                }
                "storagelocation" => t.controllers.push(Controller::StorageLocation),
                "disease" => {
                    t.disease = banished_formats::game::disease_description(&data).ok();
                }
                "pasture" => {
                    if let Ok(group) = banished_formats::game::pasture_description(&data) {
                        t.controllers.push(Controller::Pasture);
                        t.pasture = Some(group);
                    }
                }
                "orchard" => {
                    if let Ok((spacing, tend, group)) =
                        banished_formats::game::orchard_description(&data)
                    {
                        t.controllers.push(Controller::Orchard);
                        t.orchard = Some(banished_game::orchard::OrchardTemplate {
                            spacing,
                            tend,
                            group,
                        });
                    }
                }
                "hunt" => {
                    if let Ok(d) = banished_formats::game::hunt_description(&data) {
                        t.controllers.push(Controller::ResourceProduce);
                        t.hunt = Some(d);
                    }
                }
                "fishing" => {
                    let Ok(d) = banished_formats::game::fishing_description(&data) else {
                        continue;
                    };
                    t.controllers.push(Controller::ResourceProduce);
                    let output = self.load_template(archive, d.output.as_deref()?)?;
                    t.fishing = Some(banished_game::fishing::FishingTemplate {
                        output,
                        mask: d.mask,
                        group: d.group,
                    });
                }
                "forester" => {
                    let Ok(d) = banished_formats::game::forester_description(&data) else {
                        continue;
                    };
                    t.controllers.push(Controller::ResourceProduce);
                    let tree = self.load_template(archive, d.tree.as_deref()?)?;
                    t.forester = Some(banished_game::forester::ForesterTemplate {
                        tree,
                        group: d.group,
                    });
                }
                "autopickup" => {
                    t.auto_pickup = banished_formats::game::auto_pickup_description(&data).ok();
                }
                "consumeproduce" => {
                    let Ok(d) = banished_formats::game::consume_produce_description(&data) else {
                        continue;
                    };
                    t.controllers.push(Controller::ResourceProduce);
                    let mut recipes = Vec::new();
                    for r in &d.recipes {
                        let mut inputs = Vec::new();
                        for (m, n) in &r.inputs {
                            let m = m
                                .as_deref()
                                .and_then(|m| material_index(archive, &mut self.materials, m))?;
                            inputs.push((m, *n));
                        }
                        let out = r.output.as_deref()?;
                        let output = self.load_template(archive, out)?;
                        let output_material = self.templates[output].pile_material?;
                        recipes.push(banished_game::construction::Recipe {
                            inputs,
                            output,
                            output_material,
                            group: r.group,
                        });
                    }
                    t.produce = Some(banished_game::construction::ProduceTemplate {
                        recipes,
                        cycles: d.cycles,
                        work_type: d.work_type,
                        message: (d.message_type, d.message_value),
                        duration: d.duration,
                        lifetime: d.lifetime,
                        selector: d.selector,
                    });
                }
                "work" => {
                    if let Ok(w) = banished_formats::game::work_description(&data) {
                        t.max_workers = w.max_workers;
                        t.per_area = w.per_area.then_some(w.area);
                        t.adjustable = w.flag_20;
                        t.min_jobs = w.value_28;
                        t.max_jobs = w.value_2c;
                    }
                }
                "storage" => {
                    t.storage = storage_description(&data)
                        .ok()
                        .map(|d| (d.accepts, d.capacity, d.flag_20))
                }
                _ => {}
            }
        }
        // Sites: buildings (BuildComponent), removal markers (ClearResourceComponent) and
        // fields (CropFieldComponent).
        if !has_build
            && !t.controllers.contains(&Controller::ClearResource)
            && !t.controllers.contains(&Controller::CropField)
            && !t.controllers.contains(&Controller::Orchard)
            && !t.controllers.contains(&Controller::StorageLocation)
        {
            return None;
        }
        t.meshes = self.templates[template]
            .model
            .as_ref()
            .map(|m| m.variants.iter().map(|v| v.meshes.len()).collect())
            .unwrap_or_default();
        // The finished mesh's height (the icons' anchor above the model).
        t.icon_height = self.templates[template]
            .model
            .as_ref()
            .and_then(|m| m.variants.first()?.meshes.last()?.clone())
            .and_then(|r| read(archive, &normalise(&r)))
            .and_then(|d| banished_formats::types::graphics_mesh(&d).ok())
            .map(|m| m.bounds_max[2] - m.bounds_min[2]);
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
        self.add_site_from(st, entity);
    }

    /// The trader boat template (`Template\\TraderBoat.rsc`): its CharacterDescription and
    /// its model's animation group.
    pub(in crate::start) fn load_boat(
        &mut self,
        archive: &Archive,
        reference: &str,
    ) -> Option<usize> {
        let template = self.load_template(archive, reference)?;
        let file = normalise(reference).replace(":resource", ":character");
        let character = read(archive, &file)
            .and_then(|d| banished_formats::game::character_description(&d).ok())?;
        if let Some(Some(g)) = self.templates[template].animations.first() {
            self.wildlife.groups.insert((template, 0), g.clone());
        }
        self.wildlife
            .boat_kinds
            .insert(template, banished_game::boat::BoatKind { character });
        Some(template)
    }

    /// Every template with a ValueDescription, in `Game/GameResources.rsc`'s order (inferred
    /// for the original's factory order, `FUN_140059fd0`), its raw material registered.
    pub(in crate::start) fn load_trade_goods(&mut self, archive: &Archive) {
        if !self.construction.trade_goods.is_empty() {
            return;
        }
        let Some(list) = read(archive, "Game/GameResources.rsc:resource")
            .and_then(|d| banished_formats::game::external_list(&d).ok())
        else {
            return;
        };
        for r in &list {
            let r = normalise(r);
            if !r.starts_with("Template") || !r.ends_with(":resource") {
                continue;
            }
            let Some(value) = read(archive, &r.replace(":resource", ":value"))
                .and_then(|d| banished_formats::game::value_description(&d).ok())
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
                .push(banished_game::trade::TradeGood {
                    template,
                    material,
                    value,
                });
        }
    }

    /// The nomad template (`Template\\Nomad.rsc`): its CharacterDescription and its model's
    /// animation groups, for the nomads the town hall calls.
    fn load_nomad(&mut self, archive: &Archive, reference: &str) -> Option<usize> {
        let template = self.load_template(archive, reference)?;
        let file = normalise(reference).replace(":resource", ":character");
        let character = read(archive, &file)
            .and_then(|d| banished_formats::game::character_description(&d).ok())?;
        for (v, g) in self.templates[template].animations.iter().enumerate() {
            if let Some(g) = g {
                self.wildlife.groups.insert((template, v), g.clone());
            }
        }
        self.wildlife.nomad_kinds.insert(
            template,
            banished_game::immigration::NomadKind { character },
        );
        let file = normalise(reference).replace(":resource", ":nomad");
        if let Some((_, materials)) =
            read(archive, &file).and_then(|d| banished_formats::game::nomad_description(&d).ok())
        {
            self.nomad_materials.insert(template, materials);
        }
        Some(template)
    }

    /// [`Self::add_site`] with its construction template loaded.
    fn add_site_from(&mut self, st: usize, entity: usize) {
        if self.templates[self.placed[entity].template].tracker {
            self.construction.trackers.entry(entity).or_default();
        }
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
            self.construction.workforce.workplaces.insert(
                entity,
                WorkPlace::new(t.profession, t.default_max_workers((w, h))),
            );
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
                clear_total: 0,
                destroyed: false,
                // `FUN_14011e3c0`: the selector from the description, every weight 1.
                produce: t
                    .produce
                    .as_ref()
                    .map(|p| banished_game::construction::ProduceState {
                        disabled: false,
                        selector: p.selector,
                        weights: vec![1.0; p.recipes.len()],
                        produced: 0,
                    })
                    .or_else(|| {
                        (t.herbalist.is_some()
                            || t.forester.is_some()
                            || t.fishing.is_some()
                            || t.hunt.is_some())
                        .then(banished_game::construction::ProduceState::default)
                    }),
                forester: t
                    .forester
                    .as_ref()
                    .map(|_| banished_game::forester::Forester::default()),
                happiness_registered: false,
                hunt_time: t.hunt.map(|_| 0),
                crop_field: t
                    .crop_field
                    .map(|_| banished_game::farm::CropField::default()),
                orchard: t
                    .orchard
                    .map(|_| banished_game::orchard::Orchard::default()),
                fishing: t
                    .fishing
                    .as_ref()
                    .map(|_| banished_game::fishing::Fishing::default()),
                auto_pickup: t
                    .auto_pickup
                    .map(|enabled| banished_game::herbalist::AutoPickup {
                        enabled,
                        works: Vec::new(),
                    }),
                parts: Vec::new(),
                multi_stages: None,
                graves: Vec::new(),
                destroy: t.destroy.as_ref().map(banished_game::destroy::Destroy::new),
                fire: t.fire.map(|_| banished_game::fire::Fire::default()),
                // TradeComponent's activation names the merchants (`Sim::activate`).
                trade: None,
                immigration: t
                    .immigration
                    .as_ref()
                    .map(|_| banished_game::immigration::Immigration::default()),
                home: None,
                // DiseaseComponent's setup (`FUN_1400495b0`): on a pasture or not.
                disease: t.disease.map(|_| banished_game::blight::Blight {
                    pasture: t.pasture.is_some(),
                    ..Default::default()
                }),
            },
        );
        // PastureComponent's setup (`FUN_14012cf20`, no kind) over its MapComponent's area.
        if t.pasture.is_some() {
            let (x, y) = self.placed[entity].cell;
            self.wildlife.pastures.insert(
                entity,
                banished_game::wildlife::Pasture {
                    area: [x as i16, y as i16, w as i16, h as i16],
                    ..Default::default()
                },
            );
        }
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
        self.cache_carry_models(archive);
        SceneModels::from_instances(archive, self.model_instances())
    }

    /// `FUN_14011c880` with −2: what a citizen carries shows as its most-held raw material's
    /// template's first mesh (variant 0), scaled by its RawMaterialDescription `+0x50`.
    fn cache_carry_models(&self, archive: &Archive) {
        let mut cache = self.carry_models.borrow_mut();
        for s in &self.agents.states {
            for item in &s.carried.items {
                cache.entry(item.material).or_insert_with(|| {
                    let r = &self.materials.get(item.material)?.reference;
                    let model = read(archive, &r.replace(":resource", ":model"))
                        .and_then(|d| banished_formats::game::model_description(&d).ok())?;
                    let mesh = model.variants.first()?.meshes.first().cloned().flatten()?;
                    let scale = read(archive, &r.replace(":resource", ":rawmaterial"))
                        .and_then(|d| raw_material_description(&d).ok())
                        .map_or(1.0, |d| d.value_50);
                    Some((mesh, scale))
                });
            }
        }
    }

    /// Keep mesh/texture resources cached while GrowthComponent changes only instance scales.
    pub fn refresh_models(&self, archive: &Archive, models: &mut SceneModels) {
        self.cache_carry_models(archive);
        models.update_instances(archive, self.model_instances());
    }

    /// StorageDisplayComponent (`FUN_140057090`, update `FUN_140056d40` / `FUN_140056c10`):
    /// one mesh per footprint cell (by rows, turned 90 degrees on alternate cells). Each
    /// stored item, in order, fills `size × count / capacity` of the cells with its pile
    /// model's last mesh, and the next cell with the mesh for the fraction left; the rest are
    /// empty.
    fn storage_displays(&self, groups: &mut HashMap<(String, GroupMaterial), Vec<ModelInstance>>) {
        let storages = self
            .storage_sites
            .iter()
            .map(|s| (s.entity, s.storage))
            .chain(
                self.construction
                    .sites
                    .values()
                    .map(|s| (s.entity, s.storage)),
            );
        let mut seen = std::collections::BTreeSet::new();
        for (e, storage) in storages {
            let p = &self.placed[e];
            if !p.alive || !self.templates[p.template].storage_display || !seen.insert(e) {
                continue;
            }
            let Some(s) = self.storages.get(storage) else {
                continue;
            };
            let (w, h) = p.footprint;
            let cells = (w * h) as usize;
            let mut meshes: Vec<Option<String>> = Vec::with_capacity(cells);
            for item in &s.items {
                if item.count == 0 || meshes.len() == cells {
                    continue;
                }
                // The material's pile template's model, variant 0.
                let list = self
                    .templates
                    .iter()
                    .find(|t| t.pile_material == Some(item.material) && t.model.is_some())
                    .and_then(|t| t.model.as_ref())
                    .and_then(|m| m.variants.first())
                    .map(|v| v.meshes.clone())
                    .unwrap_or_default();
                let size = self.materials[item.material].size;
                let share = (size * item.count) as f32 / s.capacity.max(1) as f32 * cells as f32;
                let full = share as i32;
                let pick = |fraction: f32| {
                    let n = list.len() as i32;
                    let i = ((n as f32 * fraction) as i32).min(n - 1);
                    list.get(i.max(0) as usize).cloned().flatten()
                };
                for _ in 0..full.max(0) {
                    if meshes.len() < cells {
                        meshes.push(pick(1.0));
                    }
                }
                if meshes.len() < cells {
                    meshes.push(pick(share - full as f32));
                }
            }
            let Some(ground) = self.display_cells.get(&e) else {
                continue;
            };
            for (i, mesh) in meshes.into_iter().enumerate() {
                let Some(mesh) = mesh else { continue };
                let (row, col) = (i as i32 / w, i as i32 % w);
                let Some(&at) = ground.get(i) else { continue };
                let yaw = if (row + col) & 1 != 0 { 0.0 } else { 90.0 };
                groups
                    .entry((mesh, GroupMaterial::default()))
                    .or_default()
                    .push(ModelInstance {
                        matrix: placement_matrix(at, [1.0; 3], yaw),
                        locals: None,
                    });
            }
        }
    }

    pub(crate) fn model_instances(&self) -> Vec<GroupInstances> {
        let mut groups: HashMap<(String, GroupMaterial), Vec<ModelInstance>> = HashMap::new();
        for (e, p) in self.placed.iter().enumerate() {
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
            // A damaged building's model shows the DestroyDescription's material set.
            let set = self
                .construction
                .sites
                .get(&e)
                .filter(|s| s.destroy.as_ref().is_some_and(|d| d.damaged_look))
                .and_then(|s| self.construction.templates[s.template].destroy)
                .map_or(0, |t| t.material.max(0) as usize);
            groups
                .entry((mesh.clone(), GroupMaterial { set, first: None }))
                .or_default()
                .push(ModelInstance {
                    matrix: placement_matrix(
                        p.position,
                        [p.scale[0], p.scale[1], p.scale[2]],
                        p.yaw,
                    ),
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
        self.storage_displays(&mut groups);
        // Citizens: `Template/Citizen.rsc:model` variant 0 (men) or 1 (women); children use
        // the scale `DAT_140178818` (`FUN_14011a700`).
        if let Some(model) = &self.citizen_model {
            for (i, c) in self.citizens.iter().enumerate().filter(|(_, c)| c.alive) {
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
                // ShowTool (`FUN_14011c880`): the tool's mesh, or the carried goods', on the
                // citizen mesh's `tool` node (`Geometry::Attach`), at its scale.
                let shown = self.agents.states.get(i).map_or(-1, |s| s.shown_tool);
                let attached = match shown {
                    -2 => self.agents.states[i]
                        .carried
                        .items
                        .iter()
                        .fold(None::<&banished_game::stock::Item>, |best, it| match best {
                            Some(b) if it.count <= b.count => Some(b),
                            _ => Some(it),
                        })
                        .and_then(|it| self.carry_models.borrow().get(&it.material).cloned())
                        .flatten(),
                    n if (0..=10).contains(&n) => self
                        .tool_meshes
                        .get(n as usize)
                        .cloned()
                        .flatten()
                        .map(|m| (m, 1.0)),
                    _ => None,
                };
                if let (Some((tool, scale)), Some(Some((bones, node)))) =
                    (attached, self.citizen_bones.get(variant))
                {
                    let mut pose: Vec<banished_formats::skeleton::Transform> = match &locals {
                        Some(l) if l.len() == bones.len() => l.clone(),
                        _ => bones
                            .iter()
                            .map(banished_formats::skeleton::local_bind)
                            .collect(),
                    };
                    if let Some(root) = pose.first_mut() {
                        *root = banished_formats::skeleton::Transform::IDENTITY;
                    }
                    let world = banished_formats::skeleton::world_transforms(bones, &pose);
                    let mut placed = matrix;
                    placed[15] = 1.0;
                    let m = glam::Mat4::from_cols_array(&placed)
                        * glam::Mat4::from_cols_array(&world[*node].to_matrix())
                        * glam::Mat4::from_scale(glam::Vec3::splat(scale));
                    groups
                        .entry((tool, GroupMaterial::default()))
                        .or_default()
                        .push(ModelInstance {
                            matrix: m.to_cols_array(),
                            locals: None,
                        });
                }
                groups
                    .entry((mesh.clone(), GroupMaterial::default()))
                    .or_default()
                    .push(ModelInstance { matrix, locals });
            }
        }
        // Wild animals: their model's variant and mesh, placed by their CharacterComponent,
        // scaled by age (`FUN_14010c7f0`).
        for animal in self.wildlife.animals.values().filter(|a| !a.dying) {
            let Some(model) = &self.templates[animal.template].model else {
                continue;
            };
            let Some(Some(mesh)) = model
                .variants
                .get(animal.variant)
                .and_then(|v| v.meshes.get(animal.mesh))
            else {
                continue;
            };
            let ch = &animal.character;
            let s = animal.scale;
            let locals = self.wildlife.group(animal).and_then(|g| {
                g.clips
                    .get(ch.animation.clip)
                    .map(|clip| ch.animation.pose(clip))
            });
            groups
                .entry((mesh.clone(), GroupMaterial::default()))
                .or_default()
                .push(ModelInstance {
                    matrix: placement_matrix(ch.position, [s, s, s], ch.yaw),
                    locals,
                });
        }
        // Nomads: their template's model (men variant 0, women 1) with the NomadDescription's
        // material in place of material 0 (`FUN_140050990`), placed by their
        // CharacterComponent.
        for nomad in self.wildlife.nomads.values() {
            let Some(model) = &self.templates[nomad.template].model else {
                continue;
            };
            let Some(Some(mesh)) = model
                .variants
                .get(nomad.variant)
                .and_then(|v| v.meshes.first())
            else {
                continue;
            };
            let ch = &nomad.character;
            let locals = self
                .wildlife
                .groups
                .get(&(nomad.template, nomad.variant))
                .and_then(|g| {
                    g.clips
                        .get(ch.animation.clip)
                        .map(|clip| ch.animation.pose(clip))
                });
            let material = GroupMaterial {
                set: 0,
                first: self
                    .nomad_materials
                    .get(&nomad.template)
                    .and_then(|m| m.get(nomad.variant).cloned().flatten()),
            };
            groups
                .entry((mesh.clone(), material))
                .or_default()
                .push(ModelInstance {
                    matrix: placement_matrix(ch.position, [1.0; 3], ch.yaw),
                    locals,
                });
        }
        if let Some((mesh, matrix)) = &self.ghost {
            groups
                .entry((mesh.clone(), GroupMaterial::default()))
                .or_default()
                .push(ModelInstance {
                    matrix: *matrix,
                    locals: None,
                });
        }
        let mut list: Vec<_> = groups
            .into_iter()
            .map(|((m, material), i)| (m, material, i, None))
            .collect();
        list.sort_by(|a, b| (&a.0, &a.1).cmp(&(&b.0, &b.1)));
        list
    }
}

#[cfg(test)]
mod storage_fetch_tests {
    use super::*;
    use banished_formats::pkg::Package;
    use banished_game::{ai::activity, stock::stock_table};

    /// ShowTool (`FUN_14011c880`): a citizen's tool is drawn on its mesh's `tool` node, carried
    /// goods as their pile model; nothing with −1.
    #[test]
    fn shown_tools_hang_on_the_tool_node() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../game-data/original/WinData");
        let mut archive = Archive::default();
        for name in ["data1.pkg", "patch1.pkg", "patch0.pkg", "data0.pkg"] {
            let Ok(bytes) = std::fs::read(dir.join(name)) else {
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
        assert_eq!(town.tool_meshes.len(), 11);
        assert!(town.citizen_bones.iter().all(Option::is_some));
        let tools = |town: &StartingTown, name: &str| -> Vec<[f32; 16]> {
            town.model_instances()
                .into_iter()
                .filter(|(m, ..)| m.contains(name))
                .flat_map(|(_, _, i, _)| i.into_iter().map(|i| i.matrix))
                .collect()
        };
        assert!(tools(&town, "ToolAxe").is_empty());
        town.agents.states[0].shown_tool = 0;
        let axes = tools(&town, "ToolAxe");
        assert_eq!(axes.len(), 1);
        let at = town.citizens[0].position;
        let d = (axes[0][12] - at[0]).hypot(axes[0][13] - at[1]);
        assert!(
            d < 1.0 && 0.2 < axes[0][14] - at[2],
            "{:?} vs {at:?}",
            &axes[0][12..15]
        );
        // Carried logs: the log pile's mesh.
        let log = town
            .materials
            .iter()
            .position(|m| m.reference.contains("Wood"))
            .unwrap();
        town.agents.states[0].carried.add(log, 3);
        town.agents.states[0].shown_tool = -2;
        town.cache_carry_models(&archive);
        let carried: Vec<String> = town
            .model_instances()
            .into_iter()
            .map(|(m, ..)| m)
            .filter(|m| m.contains("RawMaterial") || m.contains("Wood") || m.contains("Log"))
            .collect();
        assert!(!carried.is_empty(), "nothing carried");
    }

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
            // Adults loiter; children loiter or play next to a parent (activity 11).
            let current = town.agents.ais[i].current;
            assert!(
                current == activity::IDLE
                    || (!citizen.is_adult(&town.rules) && current == activity::PLAY),
                "{} is doing {current}",
                citizen.name
            );
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
