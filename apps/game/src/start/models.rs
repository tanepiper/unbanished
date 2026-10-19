//! The town's models: smoothing, carried goods, storage displays and the model instances to draw.

use super::*;

impl StartingTown {
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
    pub(super) fn cache_carry_models(&self, archive: &Archive) {
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
    pub(super) fn storage_displays(
        &self,
        groups: &mut HashMap<(String, GroupMaterial), Vec<ModelInstance>>,
    ) {
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
        // Placed entities, grouped by mesh and material set in placement order. A large town
        // has about 28,000 of them every frame, so each finds its group through a small table
        // per template (variant, mesh, set) instead of hashing a copy of the mesh name, and
        // the sites are walked alongside the entities instead of looked up one by one. The
        // groups and their instance order are the same as grouping by name directly.
        let mut placed_groups: Vec<((String, GroupMaterial), Vec<ModelInstance>)> = Vec::new();
        let mut by_name: HashMap<(String, GroupMaterial), usize> = HashMap::new();
        let mut by_template: Vec<Vec<((usize, usize, usize), usize)>> =
            vec![Vec::new(); self.templates.len()];
        let mut sites = self.construction.sites.iter().peekable();
        for (e, p) in self.placed.iter().enumerate() {
            while sites.next_if(|(&k, _)| k < e).is_some() {}
            let site = sites.next_if(|(&k, _)| k == e).map(|(_, s)| s);
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
            let set = site
                .filter(|s| s.destroy.as_ref().is_some_and(|d| d.damaged_look))
                .and_then(|s| self.construction.templates[s.template].destroy)
                .map_or(0, |t| t.material.max(0) as usize);
            let key = (p.variant, p.mesh, set);
            let slots = &mut by_template[p.template];
            let group = match slots.iter().find(|(k, _)| *k == key) {
                Some(&(_, g)) => g,
                None => {
                    let name = (mesh.clone(), GroupMaterial { set, first: None });
                    let g = *by_name.entry(name.clone()).or_insert_with(|| {
                        placed_groups.push((name, Vec::new()));
                        placed_groups.len() - 1
                    });
                    slots.push((key, g));
                    g
                }
            };
            placed_groups[group].1.push(ModelInstance {
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
        let mut groups: HashMap<(String, GroupMaterial), Vec<ModelInstance>> =
            placed_groups.into_iter().collect();
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
                    .and_then(|(ch, g)| ch.animation.pose(g));
                // ShowTool (`FUN_14011c880`): the tool's mesh, or the carried goods', on the
                // citizen mesh's `tool` node (`Geometry::Attach`), at its scale.
                let shown = self.agents.states.get(i).map_or(-1, |s| s.shown_tool);
                let attached = match shown {
                    -2 => self.agents.states[i]
                        .carried
                        .items
                        .iter()
                        .fold(
                            None::<&banished_game::work::stock::Item>,
                            |best, it| match best {
                                Some(b) if it.count <= b.count => Some(b),
                                _ => Some(it),
                            },
                        )
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
            let locals = self
                .wildlife
                .group(animal)
                .and_then(|g| ch.animation.pose(g));
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
                .and_then(|g| ch.animation.pose(g));
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
