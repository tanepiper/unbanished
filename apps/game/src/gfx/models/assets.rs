//! Loading the models of a scene: each group's meshes, materials and images, shared through a
//! cache, and the scene's instances.

use super::*;

pub(super) fn with_object(reference: &str) -> String {
    let path = reference.replace('\\', "/");
    if path.contains(':') {
        path
    } else {
        format!("{path}:resource")
    }
}

pub(super) fn image(archive: &Archive, reference: Option<&str>) -> Option<ImageBuffer> {
    let r = reference?;
    image_buffer(&archive.read_ref(&with_object(r)).ok()?).ok()
}

pub(super) fn load_group(
    archive: &Archive,
    mesh_ref: &str,
    material: &GroupMaterial,
    textures: &mut HashMap<String, ModelImage>,
    pose: Option<&[Transform]>,
) -> Option<ModelAsset> {
    let pending = read_group(archive, mesh_ref, material, pose)?;
    Some(pending.finish(&mut |r| cached(archive, textures, r)))
}

/// A texture from the scene's cache, read on first use.
pub(super) fn cached(
    archive: &Archive,
    textures: &mut HashMap<String, ModelImage>,
    reference: Option<String>,
) -> ModelImage {
    let r = reference?;
    textures
        .entry(r.clone())
        .or_insert_with(|| image(archive, Some(&r)).map(std::sync::Arc::new))
        .clone()
}

/// [`load_group`] for many groups: the meshes, then the textures not in the cache yet, are
/// read on several threads (native; each is a pure function of its resource), then the
/// groups take their textures from the cache in order, so the result is the same.
pub(super) fn load_groups(
    archive: &Archive,
    groups: &[(&str, &GroupMaterial, Option<&[Transform]>)],
    textures: &mut HashMap<String, ModelImage>,
) -> Vec<Option<ModelAsset>> {
    let pending = unbanished_formats::par::map(groups, |&(mesh_ref, material, pose)| {
        read_group(archive, mesh_ref, material, pose)
    });
    let mut missing: Vec<String> = Vec::new();
    for r in pending.iter().flatten().flat_map(PendingAsset::references) {
        if !textures.contains_key(r) && !missing.iter().any(|m| m == r) {
            missing.push(r.to_string());
        }
    }
    let images = unbanished_formats::par::map(&missing, |r| {
        image(archive, Some(r)).map(std::sync::Arc::new)
    });
    textures.extend(missing.into_iter().zip(images));
    pending
        .into_iter()
        .map(|p| Some(p?.finish(&mut |r| cached(archive, textures, r))))
        .collect()
}

/// A group's mesh and materials, read without the texture cache: the ranges' textures are
/// still references (each level's AO texture, then each range's snow, damage and diffuse).
pub(super) struct PendingAsset {
    asset: ModelAsset,
    ao: Vec<Option<String>>,
    textures: Vec<[Option<String>; 3]>,
}

pub(super) fn read_group(
    archive: &Archive,
    mesh_ref: &str,
    material: &GroupMaterial,
    pose: Option<&[Transform]>,
) -> Option<PendingAsset> {
    let mesh = graphics_mesh(&archive.read_ref(&with_object(mesh_ref)).ok()?)
        .map_err(|e| log::warn!("{mesh_ref}: {e}"))
        .ok()?;
    let world = unbanished_gfx::model::bind_world(&mesh);
    // Every detail level (part) is loaded; `ModelRenderer::cull` picks one per instance and
    // pass. Only six shipped meshes have a second level (three rocks and three ores, switching
    // at 50 to 70 units); every other mesh has one level, up to 10,000.
    let first_part = mesh.parts.first()?;
    let mut set: Option<Vec<Option<String>>> =
        unbanished_gfx::model::material_set(&mesh, material.set).map(<[_]>::to_vec);
    if let (Some(first), Some(list)) = (&material.first, set.as_mut()) {
        if let Some(m) = list.first_mut() {
            *m = Some(first.clone());
        }
    }
    let set = set.as_deref();
    let mut ranges = Vec::new();
    let mut textures = Vec::new();
    // Instances without their own pose are skinned in the given group pose (shared
    // `Animation::GetPoseAtTime`), otherwise the bind pose.
    let default_palette = match pose {
        Some(locals) if locals.len() == mesh.bones.len() => {
            unbanished_formats::skeleton::palette(&mesh.bones, locals)
        }
        _ => bind_palette(&mesh.bones),
    };
    for (level, part) in mesh.parts.iter().enumerate() {
        for range in &part.ranges {
            if range.index_count == 0 {
                continue;
            }
            let (index_data, vertex_data, stride) = part.geometry(range);
            let first = range.start_index as usize * 2;
            let end = (range.start_index + range.index_count) as usize * 2;
            let v0 = range.start_vertex as usize * stride;
            let v1 = v0 + range.vertex_count as usize * stride;
            let (Some(indices), Some(vertices)) =
                (index_data.get(first..end), vertex_data.get(v0..v1))
            else {
                continue;
            };
            let mut indices = indices.to_vec();
            indices.resize(indices.len().next_multiple_of(4), 0);
            let m = unbanished_gfx::model::range_material(&world, set, range, &mut |r| {
                material_instance(&archive.read_ref(&with_object(r)).ok()?).ok()
            });
            ranges.push(RangeData {
                kind: m.kind,
                snow: None,
                damage: None,
                vertices: vertices.to_vec(),
                indices,
                index_count: range.index_count as u32,
                diffuse: None,
                ao: None,
                rigid: m.rigid.to_matrix(),
                skinned: m.skinned,
                level,
            });
            textures.push([m.snow, m.damage, m.diffuse]);
        }
    }
    let animation_bone = unbanished_gfx::model::animation_bone(first_part);
    Some(PendingAsset {
        ao: mesh.parts.iter().map(|p| p.ao_texture.clone()).collect(),
        asset: ModelAsset {
            ranges,
            levels: mesh.parts.iter().map(|p| p.lod_distance).collect(),
            bounds: (mesh.bounds_min, mesh.bounds_max),
            bones: mesh.bones,
            animation_bone,
            default_palette,
        },
        textures,
    })
}

impl PendingAsset {
    pub(super) fn references(&self) -> impl Iterator<Item = &str> {
        self.ao
            .iter()
            .flatten()
            .chain(self.textures.iter().flatten().flatten())
            .map(String::as_str)
    }

    /// Fill in the textures, in `load_group`'s order.
    pub(super) fn finish(
        mut self,
        cached: &mut impl FnMut(Option<String>) -> ModelImage,
    ) -> ModelAsset {
        let ao: Vec<ModelImage> = self.ao.into_iter().map(&mut *cached).collect();
        for (range, [snow, damage, diffuse]) in self.asset.ranges.iter_mut().zip(self.textures) {
            range.snow = cached(snow);
            range.damage = cached(damage);
            range.diffuse = cached(diffuse);
            range.ao = ao.get(range.level).cloned().flatten();
        }
        self.asset
    }
}

impl SceneModels {
    /// Group a menu scene's placements by mesh and material set (`FUN_1400a9030`).
    pub fn from_menu_scene(archive: &Archive, scene: &MenuScene) -> SceneModels {
        let mut by_key: HashMap<(i16, i16), Vec<[f32; 16]>> = HashMap::new();
        for p in &scene.placements {
            let f = p.floats;
            by_key
                .entry((p.mesh, p.material))
                .or_default()
                .push(placement_matrix(
                    [f[0], f[1], f[2]],
                    [f[3], f[4], f[5]],
                    f[6],
                ));
        }
        let mut textures = HashMap::new();
        let mut keys: Vec<_> = by_key.keys().copied().collect();
        keys.sort();
        let jobs: Vec<_> = keys
            .into_iter()
            .filter_map(|key| {
                let Some(Some(mesh_ref)) = scene.meshes.get(key.0 as usize) else {
                    return None;
                };
                let material = GroupMaterial {
                    set: key.1.max(0) as usize,
                    first: None,
                };
                Some((key, mesh_ref, material))
            })
            .collect();
        let requests: Vec<_> = jobs
            .iter()
            .map(|(_, mesh_ref, material)| (mesh_ref.as_str(), material, None))
            .collect();
        let assets = load_groups(archive, &requests, &mut textures);
        let mut groups = Vec::new();
        for ((key, mesh_ref, material), asset) in jobs.into_iter().zip(assets) {
            let Some(asset) = asset else {
                continue;
            };
            let mut group = asset.into_group(mesh_ref.clone(), material);
            let instances = by_key.remove(&key).unwrap_or_default();
            group.set_instances(
                instances
                    .into_iter()
                    .map(|matrix| ModelInstance {
                        matrix,
                        locals: None,
                    })
                    .collect(),
            );
            groups.push(group);
        }
        SceneModels {
            groups,
            revision: 0,
            textures,
        }
    }

    /// Build groups from `(mesh reference, material set, instance transforms)` — entities placed
    /// by the game (natural resources, buildings).
    pub fn from_instances(archive: &Archive, instances: Vec<GroupInstances>) -> SceneModels {
        let mut textures = HashMap::new();
        let requests: Vec<_> = instances
            .iter()
            .map(|(mesh_ref, material, _, pose)| (mesh_ref.as_str(), material, pose.as_deref()))
            .collect();
        let assets = load_groups(archive, &requests, &mut textures);
        let groups = instances
            .into_iter()
            .zip(assets)
            .filter_map(|((mesh_ref, material, instances, _), asset)| {
                let asset = asset?;
                let mut group = asset.into_group(mesh_ref, material);
                group.set_instances(instances);
                Some(group)
            })
            .collect();
        SceneModels {
            groups,
            revision: 0,
            textures,
        }
    }

    /// Growth/removal changes transforms without decoding meshes or textures again. Retain
    /// empty groups so later offspring can reuse their assets and group indices stay stable.
    pub fn update_instances(&mut self, archive: &Archive, instances: Vec<GroupInstances>) {
        // Groups missing from the new list are emptied; groups whose placements are the same
        // as last time keep their instances (`set_instances`).
        let index: HashMap<(&str, &GroupMaterial), usize> = self
            .groups
            .iter()
            .enumerate()
            .map(|(i, g)| ((g.name.as_str(), &g.material), i))
            .collect();
        let mut listed = vec![false; self.groups.len()];
        let mut found = Vec::with_capacity(instances.len());
        for (name, material, _, _) in &instances {
            let i = index.get(&(name.as_str(), material)).copied();
            if let Some(i) = i {
                listed[i] = true;
            }
            found.push(i);
        }
        drop(index);
        for (group, listed) in self.groups.iter_mut().zip(listed) {
            if !listed {
                group.set_instances(Vec::new());
            }
        }
        for ((name, material, instances, pose), found) in instances.into_iter().zip(found) {
            if let Some(i) = found {
                self.groups[i].set_instances(instances);
            } else if let Some(asset) = load_group(
                archive,
                &name,
                &material,
                &mut self.textures,
                pose.as_deref(),
            ) {
                let mut group = asset.into_group(name, material);
                group.set_instances(instances);
                self.groups.push(group);
            }
        }
        self.revision += 1;
    }
}
