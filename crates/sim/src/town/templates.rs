//! Reading templates and professions from the game data: models, floors and the loaded professions.

use super::*;

/// `Profession.rsc:resource` (laborer `+0x30`, builder `+0x38`) and the loaded professions
/// (`crate::mods::profession_list`).
pub(super) fn professions(archive: &Archive) -> Professions {
    let list = crate::mods::profession_list(archive);
    let list: Vec<Profession> = list
        .iter()
        .map(|r| {
            let p = read(archive, r).and_then(|d| unbanished_formats::game::profession(&d).ok());
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

/// A floor's picking mesh in model space: each submesh through its bind hierarchy (Runtime
/// AddMesh, `180127b70`).
pub(super) fn floor_triangles(
    mesh: &unbanished_formats::picking::PickingMesh,
) -> Vec<[[f32; 3]; 3]> {
    use unbanished_formats::skeleton::Transform;
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
pub(super) fn load_template(
    archive: &Archive,
    reference: &str,
    table: &mut Vec<Template>,
) -> Option<usize> {
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
        fenced_area: false,
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
                                            unbanished_formats::types::animation_group(&bytes).ok()
                                        })
                                        .map(|mut g| {
                                            g.name = normalise(reference);
                                            Arc::new(g)
                                        })
                                })
                                .clone()
                        })
                        .collect();
                    // The AnimAudioEvents the clips reference.
                    let groups: Vec<_> = t.animations.iter().flatten().cloned().collect();
                    anim_audio_events(archive, groups.iter().map(|g| &**g), &mut t.audio_events);
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
                    unbanished_formats::game::fruit_description(&data)
                {
                    t.fruit = Some(unbanished_game::production::orchard::FruitTemplate {
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
                let list = unbanished_formats::game::interact_description(&data)
                    .ok()
                    .and_then(|i| i.points)
                    .and_then(|r| read(archive, &normalise(&r)))
                    .and_then(|d| unbanished_formats::game::point_list(&d).ok());
                let points = |category| {
                    list.as_ref().map_or(Vec::new(), |l| {
                        unbanished_formats::game::interaction_points(l, category)
                            .iter()
                            .map(|p| p.position)
                            .collect()
                    })
                };
                t.use_points = points(0);
                t.build_points = points(1);
                t.create_points = points(2);
                t.use_yaws = list.as_ref().map_or(Vec::new(), |l| {
                    unbanished_formats::game::interaction_points(l, 0)
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
                let d = unbanished_formats::game::floor_description(&data).ok();
                t.floor_on_activate = d.as_ref().is_some_and(|f| f.active);
                t.floor = d
                    .and_then(|f| f.mesh)
                    .and_then(|m| read(archive, &normalise(&m)))
                    .and_then(|d| unbanished_formats::picking::picking_mesh(&d).ok())
                    .map(|m| std::sync::Arc::new(floor_triangles(&m)))
            }
            "tracker" => t.tracker = true,
            "createfenced" => t.fenced_area = true,
            "trade" => t.trading_post = true,
            "tornado" => {
                t.tornado = unbanished_formats::game::tornado_description(&data)
                    .ok()
                    .map(|(speed, min_distance, reach, interval, throw)| {
                        unbanished_game::disasters::tornado::TornadoTemplate {
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
                    })
            }
            "terraform" => {
                let mesh = |m: &Option<String>| {
                    m.as_deref()
                        .and_then(|m| read(archive, &normalise(m)))
                        .and_then(|d| unbanished_formats::picking::picking_mesh(&d).ok())
                        .and_then(|m| m.meshes.first().map(|s| Arc::new(s.vertices.clone())))
                };
                t.terraform = unbanished_formats::game::terraform_description(&data)
                    .ok()
                    .map(|d| unbanished_game::world::terraform::TerraformTemplate {
                        floor: mesh(&d.floor),
                        floor_mask: d.floor_mask,
                        ceiling: mesh(&d.ceiling),
                        ceiling_mask: d.ceiling_mask,
                    })
            }
            "quarry" => {
                t.quarry = unbanished_formats::game::quarry_description(&data)
                    .ok()
                    .and_then(|d| {
                        let mesh = d
                            .mesh
                            .as_deref()
                            .and_then(|m| read(archive, &normalise(m)))
                            .and_then(|d| unbanished_formats::picking::picking_mesh(&d).ok())?;
                        Some(unbanished_game::production::quarry::QuarryTemplate {
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

impl StartingTown {
    /// Load (or find) a template for an entity created during play.
    pub fn load_template(&mut self, archive: &Archive, reference: &str) -> Option<usize> {
        let t = load_template(archive, reference, &mut self.templates);
        self.resolve_piles(archive);
        t
    }
}

/// The `AnimAudioEvent`s the groups' clips name, each once, added to `out`: the event resource,
/// its SoundEffect and its probability.
pub(super) fn anim_audio_events<'g>(
    archive: &Archive,
    groups: impl Iterator<Item = &'g unbanished_formats::types::AnimationGroup>,
    out: &mut Vec<(String, Option<String>, f32)>,
) {
    for group in groups {
        for clip in &group.clips {
            for event in &clip.events {
                let Some(r) = event.resource.as_deref() else {
                    continue;
                };
                if out.iter().any(|(e, _, _)| e == r) {
                    continue;
                }
                if let Some(a) = read(archive, r)
                    .and_then(|b| unbanished_formats::audio::anim_audio_event(&b).ok())
                {
                    out.push((r.to_string(), a.effect, a.probability));
                }
            }
        }
    }
}
