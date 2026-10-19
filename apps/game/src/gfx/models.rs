//! Placed static meshes (menu scenes now; buildings and resources in game later): CPU loading of
//! `GraphicsMesh` resources and an instanced wgpu renderer.
//!
//! Each placement is `FromYawPitchOffset(yaw, offset)` (`FUN_1400a9030`): a rotation about z by
//! `yaw` degrees and a translation. Every scene mesh uses rigid triangle lists (one node
//! transform per range), so no skinning is needed here. Shading follows the disassembled model
//! programs: Opaque, OpaqueSnow (roof snow), AlphaTest, Foliage (pines: wind and snow) and
//! Billboard (leaf cards). See docs/MENU-SCENE.md.

use std::collections::HashMap;

use banished_formats::menu_scene::MenuScene;
use banished_formats::pkg::Archive;
use banished_formats::skeleton::{bind_palette, inverse_bind, world_transforms, Transform};
use banished_formats::texture::decode_rgba;
use banished_formats::types::{
    graphics_mesh, image_buffer, material_instance, Bone, ImageBuffer, STATIC_STRIDE,
};
use glam::{Mat4, Quat, Vec3};
use wgpu::util::DeviceExt;

pub use banished_gfx::material::MaterialKind;

pub const KINDS: [MaterialKind; 6] = [
    MaterialKind::Opaque,
    MaterialKind::OpaqueSnow,
    MaterialKind::AlphaTest,
    MaterialKind::Foliage,
    MaterialKind::Billboard,
    MaterialKind::Character,
];

/// A decoded texture, shared by every range that uses it (one copy in memory, one upload).
pub type ModelImage = Option<std::sync::Arc<ImageBuffer>>;

/// One draw range of a mesh with its material textures.
pub struct RangeData {
    pub kind: MaterialKind,
    pub vertices: Vec<u8>,
    pub indices: Vec<u8>,
    pub index_count: u32,
    pub diffuse: ModelImage,
    pub ao: ModelImage,
    pub snow: ModelImage,
    /// The damaged materials' damage texture (blended into the diffuse).
    pub damage: ModelImage,
    pub rigid: [f32; 16],
    /// `ModelSkinLayout` vertices blended by the instance's bone palette on the GPU.
    pub skinned: bool,
}

/// A mesh (most detailed level) and the transforms of its instances.
pub struct ModelGroup {
    pub name: String,
    /// The mesh's material set and material 0 override.
    pub material: GroupMaterial,
    pub ranges: Vec<RangeData>,
    pub instances: Vec<[f32; 16]>,
    /// One `MAX_BONES` palette per instance (skinned groups only).
    pub skins: Vec<[f32; 16]>,
    bones: Vec<Bone>,
    animation_bone: Option<usize>,
    /// The palette instances without their own pose use (the group pose or the bind pose).
    default_palette: Vec<Transform>,
}

/// A placement plus its current animation pose. Rigid trees share mesh/texture assets while
/// each expiring instance has its own playback clock and node transform.
pub struct ModelInstance {
    pub matrix: [f32; 16],
    pub locals: Option<Vec<Transform>>,
}

struct ModelAsset {
    ranges: Vec<RangeData>,
    bones: Vec<Bone>,
    animation_bone: Option<usize>,
    default_palette: Vec<Transform>,
}

impl ModelAsset {
    fn into_group(self, name: String, material: GroupMaterial) -> ModelGroup {
        ModelGroup {
            name,
            material,
            ranges: self.ranges,
            instances: Vec::new(),
            skins: Vec::new(),
            bones: self.bones,
            animation_bone: self.animation_bone,
            default_palette: self.default_palette,
        }
    }
}

impl ModelGroup {
    fn skinned(&self) -> bool {
        self.ranges.iter().any(|r| r.skinned)
    }

    /// Set the instances: transforms, and for skinned meshes each instance's bone palette from
    /// its own pose (`MeshInstance` nodes) or the group default.
    fn set_instances(&mut self, instances: Vec<ModelInstance>) {
        self.skins.clear();
        if self.skinned() {
            for instance in &instances {
                let palette = match &instance.locals {
                    Some(locals) if locals.len() == self.bones.len() => {
                        banished_formats::skeleton::palette(&self.bones, &placed_pose(locals))
                    }
                    _ => self.default_palette.clone(),
                };
                self.skins
                    .extend(banished_gfx::skin::palette_matrices(&palette));
            }
            self.instances = instances.into_iter().map(|i| i.matrix).collect();
            return;
        }
        self.instances = self.matrices(instances);
    }

    fn matrices(&self, instances: Vec<ModelInstance>) -> Vec<[f32; 16]> {
        instances
            .into_iter()
            .map(|instance| {
                let Some(locals) = instance.locals else {
                    return instance.matrix;
                };
                let Some(bone) = self.animation_bone else {
                    return instance.matrix;
                };
                if locals.len() != self.bones.len() {
                    return instance.matrix;
                }
                // Runtime UpdateNodes (0x1800ff4b0) extracts root movement, then replaces the root
                // pose with the placement transform. Tree root movement is zero in shipped clips.
                let world = world_transforms(&self.bones, &placed_pose(&locals));
                // The existing range uniform contains the bind node transform. Cancel that bind
                // frame in the instance transform before applying this instance's animated node.
                let correction = world[bone].mul(&inverse_bind(&self.bones[bone]));
                (Mat4::from_cols_array(&instance.matrix)
                    * Mat4::from_cols_array(&correction.to_matrix()))
                .to_cols_array()
            })
            .collect()
    }
}

/// The pose a placed mesh is drawn in. Runtime `MeshInstance::UpdateHierarchy` (0x1801021a0)
/// writes node 0 from the instance transform and `GetBones` (0x180102900) multiplies by its
/// inverse, so the root's sampled movement delta (`sample_pose`) is extracted, never drawn.
fn placed_pose(locals: &[Transform]) -> Vec<Transform> {
    let mut pose = locals.to_vec();
    if let Some(root) = pose.first_mut() {
        *root = Transform::IDENTITY;
    }
    pose
}

/// One mesh's instances: mesh reference, material set, instances, and the local pose skinned
/// meshes are drawn in (`None` = bind pose).
pub type GroupInstances = (
    String,
    GroupMaterial,
    Vec<ModelInstance>,
    Option<Vec<Transform>>,
);

/// What a group's mesh is drawn with: its material set (a damaged building's DestroyDescription
/// `+0x28`) and, for nomads, a MaterialInstance in place of material 0
/// (`MeshInstance::SetMaterialInstance(0, ..)`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct GroupMaterial {
    pub set: usize,
    pub first: Option<String>,
}

pub struct SceneModels {
    pub groups: Vec<ModelGroup>,
    pub revision: u64,
    /// Decoded textures by reference, kept for the scene's life so groups added later share
    /// them too. Each range held its own copy before: 101 copies for 64 textures on one save.
    textures: HashMap<String, ModelImage>,
}

fn with_object(reference: &str) -> String {
    let path = reference.replace('\\', "/");
    if path.contains(':') {
        path
    } else {
        format!("{path}:resource")
    }
}

fn image(archive: &Archive, reference: Option<&str>) -> Option<ImageBuffer> {
    let r = reference?;
    image_buffer(&archive.read_ref(&with_object(r)).ok()?).ok()
}

/// A placement's transform: `FromYawPitchOffset(yaw, offset)` (yaw in degrees about z) with the
/// per-axis scale the shaders read from the transform rows' `w` (placement floats 3..6).
pub fn placement_matrix(position: [f32; 3], scale: [f32; 3], yaw: f32) -> [f32; 16] {
    Mat4::from_scale_rotation_translation(
        Vec3::from(scale),
        Quat::from_rotation_z(yaw.to_radians()),
        Vec3::from(position),
    )
    .to_cols_array()
}

fn load_group(
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
fn cached(
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
fn load_groups(
    archive: &Archive,
    groups: &[(&str, &GroupMaterial, Option<&[Transform]>)],
    textures: &mut HashMap<String, ModelImage>,
) -> Vec<Option<ModelAsset>> {
    let pending = banished_formats::par::map(groups, |&(mesh_ref, material, pose)| {
        read_group(archive, mesh_ref, material, pose)
    });
    let mut missing: Vec<String> = Vec::new();
    for r in pending.iter().flatten().flat_map(PendingAsset::references) {
        if !textures.contains_key(r) && !missing.iter().any(|m| m == r) {
            missing.push(r.to_string());
        }
    }
    let images = banished_formats::par::map(&missing, |r| {
        image(archive, Some(r)).map(std::sync::Arc::new)
    });
    textures.extend(missing.into_iter().zip(images));
    pending
        .into_iter()
        .map(|p| Some(p?.finish(&mut |r| cached(archive, textures, r))))
        .collect()
}

/// A group's mesh and materials, read without the texture cache: the ranges' textures are
/// still references (the AO texture, then each range's snow, damage and diffuse).
struct PendingAsset {
    asset: ModelAsset,
    ao: Option<String>,
    textures: Vec<[Option<String>; 3]>,
}

impl PendingAsset {
    fn references(&self) -> impl Iterator<Item = &str> {
        self.ao
            .iter()
            .chain(self.textures.iter().flatten().flatten())
            .map(String::as_str)
    }

    /// Fill in the textures, in `load_group`'s order.
    fn finish(mut self, cached: &mut impl FnMut(Option<String>) -> ModelImage) -> ModelAsset {
        let ao = cached(self.ao);
        for (range, [snow, damage, diffuse]) in self.asset.ranges.iter_mut().zip(self.textures) {
            range.snow = cached(snow);
            range.damage = cached(damage);
            range.diffuse = cached(diffuse);
            range.ao = ao.clone();
        }
        self.asset
    }
}

fn read_group(
    archive: &Archive,
    mesh_ref: &str,
    material: &GroupMaterial,
    pose: Option<&[Transform]>,
) -> Option<PendingAsset> {
    let mesh = graphics_mesh(&archive.read_ref(&with_object(mesh_ref)).ok()?)
        .map_err(|e| log::warn!("{mesh_ref}: {e}"))
        .ok()?;
    let world = banished_gfx::model::bind_world(&mesh);
    let part = mesh.parts.first()?;
    let mut set: Option<Vec<Option<String>>> =
        banished_gfx::model::material_set(&mesh, material.set).map(<[_]>::to_vec);
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
            banished_formats::skeleton::palette(&mesh.bones, locals)
        }
        _ => bind_palette(&mesh.bones),
    };
    for range in &part.ranges {
        if range.index_count == 0 {
            continue;
        }
        let (index_data, vertex_data, stride) = part.geometry(range);
        let first = range.start_index as usize * 2;
        let end = (range.start_index + range.index_count) as usize * 2;
        let v0 = range.start_vertex as usize * stride;
        let v1 = v0 + range.vertex_count as usize * stride;
        let (Some(indices), Some(vertices)) = (index_data.get(first..end), vertex_data.get(v0..v1))
        else {
            continue;
        };
        let mut indices = indices.to_vec();
        indices.resize(indices.len().next_multiple_of(4), 0);
        let m = banished_gfx::model::range_material(&world, set, range, &mut |r| {
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
        });
        textures.push([m.snow, m.damage, m.diffuse]);
    }
    let animation_bone = banished_gfx::model::animation_bone(part);
    Some(PendingAsset {
        ao: part.ao_texture.clone(),
        asset: ModelAsset {
            ranges,
            bones: mesh.bones,
            animation_bone,
            default_palette,
        },
        textures,
    })
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
        for group in &mut self.groups {
            group.instances.clear();
            group.skins.clear();
        }
        for (name, material, instances, pose) in instances {
            if let Some(group) = self
                .groups
                .iter_mut()
                .find(|g| g.name == name && g.material == material)
            {
                group.set_instances(instances);
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

const SHADER: &str = r#"
@group(1) @binding(0) var diffuse_tex: texture_2d<f32>;
@group(1) @binding(1) var ao_tex: texture_2d<f32>;
@group(1) @binding(2) var samp: sampler;
// The range's rigid node transform, and `damage.x` 1 for a damaged material.
struct Range {
    rigid: mat4x4<f32>,
    damage: vec4<f32>,
};
@group(1) @binding(3) var<uniform> range: Range;
@group(1) @binding(4) var snow_tex: texture_2d<f32>;
@group(1) @binding(5) var damage_tex: texture_2d<f32>;

// The damaged materials (`OpaqueSnowDamageMaterial_ps` and the others): the damage texture,
// tiled 4× over the AO coordinates, replaces the diffuse by
// `min(1 − (1 − (r² + g² + r·b))², 0.75)` (none below 0).
fn damaged(base: vec3<f32>, uv_ao: vec2<f32>) -> vec3<f32> {
    let d = textureSample(damage_tex, samp, uv_ao * 4.0).rgb;
    if (range.damage.x < 0.5) {
        return base;
    }
    let l = 1.0 - (base.r * base.r + base.g * base.g + base.r * base.b);
    let w = min(1.0 - l * l, 0.75);
    if (w < 0.0) {
        return base;
    }
    return base + (d - base) * w;
}

struct VsIn {
    @location(0) position: vec4<i32>,
    @location(1) texcoord: vec4<i32>,
    @location(2) normal: vec4<f32>,
    // userData: billboard corner or foliage wind weight.
    @location(3) user: vec2<i32>,
    @location(5) m0: vec4<f32>,
    @location(6) m1: vec4<f32>,
    @location(7) m2: vec4<f32>,
    @location(8) m3: vec4<f32>,
};

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) uv_ao: vec2<f32>,
    // x: diffuse (-n.L), y: ambient factor (n.L / 2 + 1.5), z: fog, w: leaf flag.
    @location(2) light: vec4<f32>,
    // xy: snow UV (world / 16), z: world normal z, w: world z (reflection clip).
    @location(3) snow: vec4<f32>,
    // Sun shadow map coordinate (lc[10..13]).
    @location(4) sh: vec3<f32>,
};

fn finish(world: vec3<f32>, n: vec3<f32>, texcoord: vec4<i32>, leaf: f32) -> VsOut {
    var out: VsOut;
    out.clip = g.view_proj * vec4<f32>(world, 1.0);
    out.uv = vec2<f32>(texcoord.xy) / 512.0;
    out.uv_ao = vec2<f32>(texcoord.zw) / 8192.0;
    let d = dot(n, g.light_dir.xyz);
    out.light = vec4<f32>(-d, d * 0.5 + 1.5, (out.clip.w + g.fog.x) * g.fog.y, leaf);
    out.snow = vec4<f32>(world.xy * 0.0625, n.z, world.z);
    out.sh = shadow_coord(world);
    return out;
}

@vertex
fn vs_main(v: VsIn) -> VsOut {
    let model = mat4x4<f32>(v.m0, v.m1, v.m2, v.m3) * range.rigid;
    let p = model * vec4<f32>(vec3<f32>(v.position.xyz) / 512.0, 1.0);
    let n = normalize((model * vec4<f32>(v.normal.xyz * 2.0 - 1.0, 0.0)).xyz);
    return finish(p.xyz, n, v.texcoord, 0.0);
}

// NormalSkin_charactervs: the instance's translation w carries the outfit (0..7) and selects one
// eighth of the texture atlas horizontally.
@vertex
fn vs_character(v: VsIn) -> VsOut {
    let appearance = v.m3.w;
    let model = mat4x4<f32>(v.m0, v.m1, v.m2, vec4<f32>(v.m3.xyz, 1.0)) * range.rigid;
    let p = model * vec4<f32>(vec3<f32>(v.position.xyz) / 512.0, 1.0);
    let n = normalize((model * vec4<f32>(v.normal.xyz * 2.0 - 1.0, 0.0)).xyz);
    var out = finish(p.xyz, n, v.texcoord, 0.0);
    out.uv.x = (f32(v.texcoord.x) * 0.001953125 + appearance) * 0.125;
    return out;
}

struct SkinIn {
    @location(0) position: vec4<i32>,
    @location(1) texcoord: vec4<i32>,
    @location(2) normal: vec4<f32>,
    @location(3) weights: vec4<f32>,
    @location(4) bones: vec4<u32>,
    @location(5) m0: vec4<f32>,
    @location(6) m1: vec4<f32>,
    @location(7) m2: vec4<f32>,
    @location(8) m3: vec4<f32>,
};

// NormalSkin_vs: blend four bones of the instance's palette, then the placement. The skinned
// point takes w = 1 (`skin_point`) before the placement; otherwise weight sums below 1 would
// scale each vertex's share of the world translation.
fn skinned(v: SkinIn) -> VsOut {
    let placement = mat4x4<f32>(v.m0, v.m1, v.m2, vec4<f32>(v.m3.xyz, 1.0));
    let skin_m = skin_matrix(v.weights, v.bones);
    let p = placement * skin_point(skin_m, vec3<f32>(v.position.xyz) / 512.0);
    let n = normalize((placement * skin_m * vec4<f32>(v.normal.xyz * 2.0 - 1.0, 0.0)).xyz);
    return finish(p.xyz, n, v.texcoord, 0.0);
}

@vertex
fn vs_skinned(v: SkinIn) -> VsOut {
    return skinned(v);
}

// NormalSkin_charactervs: as above, with the outfit column of the texture atlas.
@vertex
fn vs_skinned_character(v: SkinIn) -> VsOut {
    var out = skinned(v);
    out.uv.x = (f32(v.texcoord.x) * 0.001953125 + v.m3.w) * 0.125;
    return out;
}

// FoliageMaterial_vs: sway by sin(world.xy + time) weighted by userData.
@vertex
fn vs_foliage(v: VsIn) -> VsOut {
    let model = mat4x4<f32>(v.m0, v.m1, v.m2, v.m3) * range.rigid;
    var p = (model * vec4<f32>(vec3<f32>(v.position.xyz) / 512.0, 1.0)).xyz;
    let n = normalize((model * vec4<f32>(v.normal.xyz * 2.0 - 1.0, 0.0)).xyz);
    let wind = sin(p.xy + g.camera_forward.w) * vec2<f32>(v.user);
    p = vec3<f32>(p.xy + wind * 3.05175786e-06, p.z);
    return finish(p, n, v.texcoord, 0.0);
}

// BillboardMaterial_vs: vertices with a corner offset (userData) are leaf cards. They are pushed
// out along the camera's right and up axes (bent towards the view by the normal), sway with the
// wind, and are lit with a rounded crown normal.
@vertex
fn vs_billboard(v: VsIn) -> VsOut {
    let model = mat4x4<f32>(v.m0, v.m1, v.m2, v.m3) * range.rigid;
    let local = vec3<f32>(v.position.xyz) / 512.0;
    var world = (model * vec4<f32>(local, 1.0)).xyz;
    let scale = vec3<f32>(length(v.m0.xyz), length(v.m1.xyz), length(v.m2.xyz));
    let n = normalize((model * vec4<f32>(normalize(v.normal.xyz * 2.0 - 1.0), 0.0)).xyz);
    let corner = vec2<f32>(v.user);
    let leaf = select(0.0, 1.0, dot(corner, corner) >= 0.05);
    let right = g.camera_right.xyz;
    let forward = g.camera_forward.xyz;
    let up = g.camera_up.xyz;
    let nc = vec3<f32>(dot(n, right), dot(n, forward), dot(n, up)) * leaf;
    let r = normalize(right + 0.5 * nc.x * forward) * scale.x;
    let u = normalize(up + 0.5 * nc.z * forward) * scale.z;
    let wind = sin(world.xy + g.camera_forward.w) * 0.025 * dot(nc, nc) * scale.xz * leaf;
    world += r * wind.x + u * wind.y;
    let c = corner / 8192.0;
    world += r * c.x + u * c.y;
    let crown = normalize(vec3<f32>(local.x * scale.x, local.y * scale.y, local.z * scale.z + 4.0));
    let lit_normal = mix(n, crown, leaf);
    return finish(world, lit_normal, v.texcoord, leaf);
}

fn lit(base: vec3<f32>, ao: f32, in: VsOut) -> vec3<f32> {
    let light = g.ambient_color.rgb * in.light.y * ao
        + g.light_color.rgb * sun_shadow(in.sh) * clamp(in.light.x, 0.0, 1.0);
    let c = base * light;
    let fog = min(g.fog_color.w, clamp(in.light.z, 0.0, 1.0));
    return c + fog * (g.fog_color.rgb - c);
}

// OpaqueMaterial_ps.
@fragment
fn fs_opaque(in: VsOut) -> @location(0) vec4<f32> {
    let base = textureSample(diffuse_tex, samp, in.uv);
    let ao = textureSample(ao_tex, samp, in.uv_ao).r;
    let c = lit(damaged(base.rgb, in.uv_ao), ao, in);
    reflect_clip(in.snow.w);
    return vec4<f32>(c, base.a);
}

// OpaqueSnowMaterial_ps: snow where diffuse alpha × up-facing × snow amount × AO².
@fragment
fn fs_snow(in: VsOut) -> @location(0) vec4<f32> {
    let base = textureSample(diffuse_tex, samp, in.uv);
    let s = textureSample(snow_tex, samp, in.snow.xy);
    let ao = textureSample(ao_tex, samp, in.uv_ao).r;
    let m = base.a * in.snow.z * g.user.x * ao * ao;
    let c = lit(mix(damaged(base.rgb, in.uv_ao), s.rgb, m), ao, in);
    reflect_clip(in.snow.w);
    return vec4<f32>(c, 0.0);
}

@fragment
fn fs_alpha(in: VsOut) -> @location(0) vec4<f32> {
    let base = textureSample(diffuse_tex, samp, in.uv);
    let ao = textureSample(ao_tex, samp, in.uv_ao).r;
    let c = lit(damaged(base.rgb, in.uv_ao), ao, in);
    if (base.a < 0.5) {
        discard;
    }
    reflect_clip(in.snow.w);
    return vec4<f32>(c, base.a);
}

// FoliageMaterial_ps: alpha cut; snow on upward faces weighted by AO⁴.
@fragment
fn fs_foliage(in: VsOut) -> @location(0) vec4<f32> {
    let base = textureSample(diffuse_tex, samp, in.uv);
    let ao = textureSample(ao_tex, samp, in.uv_ao).r;
    let s = textureSample(snow_tex, samp, in.snow.xy);
    let a2 = ao * ao;
    let m = a2 * a2 * in.snow.z * g.user.x;
    let c = lit(mix(base.rgb, s.rgb, m), ao, in);
    if (base.a < 0.5) {
        discard;
    }
    reflect_clip(in.snow.w);
    return vec4<f32>(c, 0.0);
}

// BillboardMaterial_ps: leaves thin out with userPosition.z and turn autumn-coloured with .y.
@fragment
fn fs_billboard(in: VsOut) -> @location(0) vec4<f32> {
    let base = textureSample(diffuse_tex, samp, in.uv);
    let ao = textureSample(ao_tex, samp, in.uv_ao).r;
    let alpha = base.a - in.light.w * g.user.z;
    let lum = dot(base.rgb, base.rgb);
    let autumn = lum * vec3<f32>(0.913725495, 0.725490212, 0.278431386);
    let rgb = base.rgb + in.light.w * g.user.y * (autumn - base.rgb);
    let c = lit(rgb, ao, in);
    if (alpha < 0.5) {
        discard;
    }
    reflect_clip(in.snow.w);
    return vec4<f32>(c, 0.0);
}

// Shadow pass (`*_depth` passes). OpaqueMaterial_DepthAlpha_ps (AlphaTest and Foliage) cuts at
// 0.25, half the colour pass's threshold.
@fragment
fn fs_depth_alpha(in: VsOut) {
    if (textureSample(diffuse_tex, samp, in.uv).a < 0.25) {
        discard;
    }
}

// BillboardMaterial_Depth_ps: the colour pass's leaf thinning and 0.5 cut.
@fragment
fn fs_depth_billboard(in: VsOut) {
    if (textureSample(diffuse_tex, samp, in.uv).a - in.light.w * g.user.z < 0.5) {
        discard;
    }
}
"#;

struct GpuRange {
    kind: MaterialKind,
    skinned: bool,
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    index_count: u32,
    bind: wgpu::BindGroup,
}

struct GpuGroup {
    ranges: Vec<GpuRange>,
    instances: wgpu::Buffer,
    count: u32,
    /// Per-instance bone palettes and their dynamic-offset bind group (skinned groups).
    skin: Option<(wgpu::Buffer, wgpu::BindGroup)>,
}

pub struct ModelRenderer {
    pipelines: [wgpu::RenderPipeline; 6],
    /// Skinned Opaque and Character pipelines.
    skinned_pipelines: [wgpu::RenderPipeline; 2],
    /// The same, for the sun's shadow pass (`*_depth` material passes).
    depth_pipelines: [wgpu::RenderPipeline; 6],
    skinned_depth_pipelines: [wgpu::RenderPipeline; 2],
    skin_layout: wgpu::BindGroupLayout,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    white: wgpu::TextureView,
    groups: Vec<GpuGroup>,
    /// Uploaded textures by their image data's address. The scene's groups keep their images
    /// until the next full upload, which clears this.
    views: HashMap<usize, wgpu::TextureView>,
}

impl ModelRenderer {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        globals_layout: &wgpu::BindGroupLayout,
    ) -> ModelRenderer {
        let (format, depth) = (
            crate::gfx::scene_gfx::SCENE_FORMAT,
            crate::gfx::scene_gfx::DEPTH_FORMAT,
        );
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("models"),
            source: wgpu::ShaderSource::Wgsl(
                format!(
                    "{}{}{SHADER}",
                    crate::gfx::scene_gfx::GLOBALS_WGSL,
                    banished_gfx::skin::SKIN_WGSL
                )
                .into(),
            ),
        });
        let tex = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("model material"),
            entries: &[
                tex(0),
                tex(1),
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                tex(4),
                tex(5),
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("models"),
            bind_group_layouts: &[Some(globals_layout), Some(&layout)],
            immediate_size: 0,
        });
        let make = |vs: &str, fs: &str| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(fs),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some(vs),
                    compilation_options: Default::default(),
                    buffers: &[
                        Some(wgpu::VertexBufferLayout {
                            array_stride: STATIC_STRIDE as u64,
                            step_mode: wgpu::VertexStepMode::Vertex,
                            attributes: &wgpu::vertex_attr_array![
                                0 => Sint16x4, 1 => Sint16x4, 2 => Unorm8x4, 3 => Sint16x2
                            ],
                        }),
                        Some(wgpu::VertexBufferLayout {
                            array_stride: 64,
                            step_mode: wgpu::VertexStepMode::Instance,
                            attributes: &wgpu::vertex_attr_array![
                                5 => Float32x4, 6 => Float32x4, 7 => Float32x4, 8 => Float32x4
                            ],
                        }),
                    ],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(fs),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    // Foliage cards are single-sided in the data but seen from both sides.
                    cull_mode: None,
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: depth,
                    depth_write_enabled: Some(true),
                    depth_compare: Some(wgpu::CompareFunction::Less),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: crate::gfx::scene_gfx::multisample(),
                multiview_mask: None,
                cache: None,
            })
        };
        let skin_layout = banished_gfx::skin::skin_layout(device, true);
        let skinned_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("skinned models"),
            bind_group_layouts: &[Some(globals_layout), Some(&layout), Some(&skin_layout)],
            immediate_size: 0,
        });
        let skin_attributes = banished_gfx::skin::skinned_attributes();
        let make_skinned = |vs: &str| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(vs),
                layout: Some(&skinned_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some(vs),
                    compilation_options: Default::default(),
                    buffers: &[
                        Some(wgpu::VertexBufferLayout {
                            array_stride: banished_formats::types::SKINNED_STRIDE as u64,
                            step_mode: wgpu::VertexStepMode::Vertex,
                            attributes: &skin_attributes,
                        }),
                        Some(wgpu::VertexBufferLayout {
                            array_stride: 64,
                            step_mode: wgpu::VertexStepMode::Instance,
                            attributes: &wgpu::vertex_attr_array![
                                5 => Float32x4, 6 => Float32x4, 7 => Float32x4, 8 => Float32x4
                            ],
                        }),
                    ],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_opaque"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    cull_mode: None,
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: depth,
                    depth_write_enabled: Some(true),
                    depth_compare: Some(wgpu::CompareFunction::Less),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: crate::gfx::scene_gfx::multisample(),
                multiview_mask: None,
                cache: None,
            })
        };
        let skinned_pipelines = [
            make_skinned("vs_skinned"),
            make_skinned("vs_skinned_character"),
        ];
        // Shadow pass: the colour passes' vertex programs under the sun camera's globals; only
        // the alpha-tested materials have a depth pixel shader.
        let static_attributes = wgpu::vertex_attr_array![
            0 => Sint16x4, 1 => Sint16x4, 2 => Unorm8x4, 3 => Sint16x2
        ];
        let instance_attributes = wgpu::vertex_attr_array![
            5 => Float32x4, 6 => Float32x4, 7 => Float32x4, 8 => Float32x4
        ];
        let instance = wgpu::VertexBufferLayout {
            array_stride: 64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &instance_attributes,
        };
        let static_buffers = [
            Some(wgpu::VertexBufferLayout {
                array_stride: STATIC_STRIDE as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &static_attributes,
            }),
            Some(instance.clone()),
        ];
        let skinned_buffers = [
            Some(wgpu::VertexBufferLayout {
                array_stride: banished_formats::types::SKINNED_STRIDE as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &skin_attributes,
            }),
            Some(instance),
        ];
        use crate::gfx::shadow::DepthState;
        let depth = |vs: &str, fs, state| {
            crate::gfx::shadow::depth_pipeline(
                device,
                vs,
                &pipeline_layout,
                &shader,
                vs,
                fs,
                &static_buffers,
                state,
            )
        };
        // Same order as `KINDS`: Opaque and OpaqueSnow use Depth_ps (no alpha test).
        let depth_pipelines = [
            depth("vs_main", None, DepthState::BackFaces),
            depth("vs_main", None, DepthState::BackFaces),
            depth(
                "vs_main",
                Some("fs_depth_alpha"),
                DepthState::FrontFacesBiased,
            ),
            depth(
                "vs_foliage",
                Some("fs_depth_alpha"),
                DepthState::FrontFacesBiased,
            ),
            depth(
                "vs_billboard",
                Some("fs_depth_billboard"),
                DepthState::TwoSided,
            ),
            depth("vs_character", None, DepthState::BackFaces),
        ];
        let skinned_depth = |vs: &str| {
            crate::gfx::shadow::depth_pipeline(
                device,
                vs,
                &skinned_layout,
                &shader,
                vs,
                None,
                &skinned_buffers,
                DepthState::BackFaces,
            )
        };
        let skinned_depth_pipelines = [
            skinned_depth("vs_skinned"),
            skinned_depth("vs_skinned_character"),
        ];
        // Same order as `KINDS`.
        let pipelines = [
            make("vs_main", "fs_opaque"),
            make("vs_main", "fs_snow"),
            make("vs_main", "fs_alpha"),
            make("vs_foliage", "fs_foliage"),
            make("vs_billboard", "fs_billboard"),
            make("vs_character", "fs_opaque"),
        ];
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            ..crate::gfx::scene_gfx::material_sampler(wgpu::AddressMode::Repeat)
        });
        let white = banished_gfx::upload_rgba_as(
            device,
            queue,
            "white",
            1,
            1,
            &[vec![255; 4]],
            wgpu::TextureFormat::Rgba8Unorm,
        );
        ModelRenderer {
            pipelines,
            skinned_pipelines,
            depth_pipelines,
            skinned_depth_pipelines,
            skin_layout,
            layout,
            sampler,
            white,
            groups: Vec::new(),
            views: HashMap::new(),
        }
    }

    pub fn clear(&mut self) {
        self.groups.clear();
        self.views.clear();
    }

    /// Upload every group of a scene (a new world).
    pub fn upload(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, models: &SceneModels) {
        self.clear();
        for g in &models.groups {
            let gpu = self.upload_group(device, queue, g);
            self.groups.push(gpu);
        }
    }

    /// One group's ranges, textures (each image uploaded once) and instances.
    fn upload_group(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        g: &ModelGroup,
    ) -> GpuGroup {
        let white = self.white.clone();
        let views = &mut self.views;
        let mut view_of = |img: &ModelImage| -> wgpu::TextureView {
            let Some(img) = img else {
                return white.clone();
            };
            // Images are shared between ranges (`ModelImage`); key by the shared allocation.
            let key = std::sync::Arc::as_ptr(img) as usize;
            views
                .entry(key)
                .or_insert_with(|| {
                    let mips: Vec<Vec<u8>> = (0..img.mip_count().max(1))
                        .map_while(|i| {
                            decode_rgba(
                                img.format,
                                (img.width >> i).max(1),
                                (img.height >> i).max(1),
                                img.levels.get(i)?,
                            )
                        })
                        .collect();
                    if mips.is_empty() {
                        return white.clone();
                    }
                    banished_gfx::upload_rgba_as(
                        device,
                        queue,
                        "model texture",
                        img.width,
                        img.height,
                        &mips,
                        wgpu::TextureFormat::Rgba8Unorm,
                    )
                })
                .clone()
        };
        let ranges = g
            .ranges
            .iter()
            .map(|r| {
                let mut uniform = [0.0f32; 20];
                uniform[..16].copy_from_slice(&r.rigid);
                uniform[16] = if r.damage.is_some() { 1.0 } else { 0.0 };
                let rigid = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("rigid"),
                    contents: bytemuck::cast_slice(&uniform),
                    usage: wgpu::BufferUsages::UNIFORM,
                });
                let (diffuse, ao, snow, damage) = (
                    view_of(&r.diffuse),
                    view_of(&r.ao),
                    view_of(&r.snow),
                    view_of(&r.damage),
                );
                let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some(&g.name),
                    layout: &self.layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(&diffuse),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::TextureView(&ao),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::Sampler(&self.sampler),
                        },
                        wgpu::BindGroupEntry {
                            binding: 3,
                            resource: rigid.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 4,
                            resource: wgpu::BindingResource::TextureView(&snow),
                        },
                        wgpu::BindGroupEntry {
                            binding: 5,
                            resource: wgpu::BindingResource::TextureView(&damage),
                        },
                    ],
                });
                GpuRange {
                    kind: r.kind,
                    skinned: r.skinned,
                    vertices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some(&g.name),
                        contents: &r.vertices,
                        usage: wgpu::BufferUsages::VERTEX,
                    }),
                    indices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some(&g.name),
                        contents: &r.indices,
                        usage: wgpu::BufferUsages::INDEX,
                    }),
                    index_count: r.index_count,
                    bind,
                }
            })
            .collect();
        GpuGroup {
            ranges,
            instances: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("instances"),
                contents: bytemuck::cast_slice(&g.instances),
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            }),
            count: g.instances.len() as u32,
            skin: self.skin_buffer(device, &g.skins),
        }
    }

    /// A buffer of per-instance palettes and its bind group (`None` without palettes).
    fn skin_buffer(
        &self,
        device: &wgpu::Device,
        skins: &[[f32; 16]],
    ) -> Option<(wgpu::Buffer, wgpu::BindGroup)> {
        skin_buffer(device, &self.skin_layout, skins)
    }

    /// Refresh the instance buffers. Groups are only ever added to a scene, so new ones are
    /// uploaded on their own; re-uploading every group and texture took about 50 ms, a visible
    /// hitch each time a new kind of model appeared (issue #5).
    pub fn update_instances(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        models: &SceneModels,
    ) {
        if self.groups.len() > models.groups.len() {
            self.upload(device, queue, models);
            return;
        }
        for g in &models.groups[self.groups.len()..] {
            let gpu = self.upload_group(device, queue, g);
            self.groups.push(gpu);
        }
        for (gpu, group) in self.groups.iter_mut().zip(&models.groups) {
            let data = bytemuck::cast_slice(&group.instances);
            if data.len() as u64 > gpu.instances.size() {
                gpu.instances = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("grown instances"),
                    contents: data,
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                });
            } else if !data.is_empty() {
                queue.write_buffer(&gpu.instances, 0, data);
            }
            gpu.count = group.instances.len() as u32;
            let skins: &[u8] = bytemuck::cast_slice(&group.skins);
            match &gpu.skin {
                Some((buffer, _)) if skins.len() as u64 <= buffer.size() => {
                    if !skins.is_empty() {
                        queue.write_buffer(buffer, 0, skins);
                    }
                }
                _ if !skins.is_empty() => {
                    gpu.skin = skin_buffer(device, &self.skin_layout, &group.skins);
                }
                _ => {}
            }
        }
    }

    pub fn draw<'p>(&'p self, pass: &mut wgpu::RenderPass<'p>, globals: &'p wgpu::BindGroup) {
        self.draw_with(pass, globals, &self.pipelines, &self.skinned_pipelines);
    }

    /// The models' depth into the sun's shadow map.
    pub fn draw_depth<'p>(&'p self, pass: &mut wgpu::RenderPass<'p>, globals: &'p wgpu::BindGroup) {
        self.draw_with(
            pass,
            globals,
            &self.depth_pipelines,
            &self.skinned_depth_pipelines,
        );
    }

    fn draw_with<'p>(
        &'p self,
        pass: &mut wgpu::RenderPass<'p>,
        globals: &'p wgpu::BindGroup,
        pipelines: &'p [wgpu::RenderPipeline; 6],
        skinned_pipelines: &'p [wgpu::RenderPipeline; 2],
    ) {
        if self.groups.is_empty() {
            return;
        }
        pass.set_bind_group(0, globals, &[]);
        for (i, kind) in KINDS.into_iter().enumerate() {
            pass.set_pipeline(&pipelines[i]);
            for g in &self.groups {
                // Groups emptied by removal or a moved placement ghost keep their assets but
                // have nothing to draw (and an empty instance buffer cannot be bound).
                if g.count == 0 || g.instances.size() == 0 {
                    continue;
                }
                pass.set_vertex_buffer(1, g.instances.slice(..));
                for r in g.ranges.iter().filter(|r| r.kind == kind && !r.skinned) {
                    pass.set_bind_group(1, &r.bind, &[]);
                    pass.set_vertex_buffer(0, r.vertices.slice(..));
                    pass.set_index_buffer(r.indices.slice(..), wgpu::IndexFormat::Uint16);
                    pass.draw_indexed(0..r.index_count, 0, 0..g.count);
                }
            }
        }
        // Skinned ranges: one draw per instance with its palette (dynamic offset).
        for (pipeline, character) in [
            (&skinned_pipelines[0], false),
            (&skinned_pipelines[1], true),
        ] {
            pass.set_pipeline(pipeline);
            for g in &self.groups {
                let Some((_, skin)) = &g.skin else { continue };
                if g.count == 0 || g.instances.size() == 0 {
                    continue;
                }
                pass.set_vertex_buffer(1, g.instances.slice(..));
                for r in g
                    .ranges
                    .iter()
                    .filter(|r| r.skinned && (r.kind == MaterialKind::Character) == character)
                {
                    pass.set_bind_group(1, &r.bind, &[]);
                    pass.set_vertex_buffer(0, r.vertices.slice(..));
                    pass.set_index_buffer(r.indices.slice(..), wgpu::IndexFormat::Uint16);
                    for i in 0..g.count {
                        let offset = (i as u64 * banished_gfx::skin::PALETTE_BYTES) as u32;
                        pass.set_bind_group(2, skin, &[offset]);
                        pass.draw_indexed(0..r.index_count, 0, i..i + 1);
                    }
                }
            }
        }
    }
}

/// A buffer of per-instance palettes and its bind group (`None` without palettes).
fn skin_buffer(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    skins: &[[f32; 16]],
) -> Option<(wgpu::Buffer, wgpu::BindGroup)> {
    if skins.is_empty() {
        return None;
    }
    let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("skin palettes"),
        contents: bytemuck::cast_slice(skins),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });
    let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("skin palettes"),
        layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                buffer: &buffer,
                offset: 0,
                size: wgpu::BufferSize::new(banished_gfx::skin::PALETTE_BYTES),
            }),
        }],
    });
    Some((buffer, bind))
}
