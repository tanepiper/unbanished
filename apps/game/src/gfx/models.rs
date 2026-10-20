//! Placed static meshes (menu scenes now; buildings and resources in game later): CPU loading of
//! `GraphicsMesh` resources and an instanced wgpu renderer.
//!
//! Each placement is `FromYawPitchOffset(yaw, offset)` (`FUN_1400a9030`): a rotation about z by
//! `yaw` degrees and a translation. Every scene mesh uses rigid triangle lists (one node
//! transform per range), so no skinning is needed here. Shading follows the disassembled model
//! programs: Opaque, OpaqueSnow (roof snow), AlphaTest, Foliage (pines: wind and snow) and
//! Billboard (leaf cards). See docs/MENU-SCENE.md.

mod assets;
mod draw;
mod pipelines;
mod upload;
use upload::GpuGroup;

use std::collections::HashMap;

use glam::{Mat4, Quat, Vec3};
use unbanished_formats::menu_scene::MenuScene;
use unbanished_formats::pkg::Archive;
use unbanished_formats::skeleton::{bind_palette, inverse_bind, world_transforms, Transform};
use unbanished_formats::texture::decode_rgba;
use unbanished_formats::types::{
    graphics_mesh, image_buffer, material_instance, Bone, ImageBuffer, STATIC_STRIDE,
};
use wgpu::util::DeviceExt;

pub use unbanished_gfx::material::MaterialKind;

mod culling;
pub(crate) use culling::Frustum;
use culling::{detail_level, Bounds};

/// The passes models are drawn in, each with its own view and its own list of the instances it
/// can see ([`ModelRenderer::cull`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModelPass {
    /// The sun's shadow map.
    Shadow = 0,
    /// The planar water reflection.
    Reflection = 1,
    /// The scene.
    Scene = 2,
}

/// The instances of a group one pass draws.
#[derive(Clone, Debug, Default)]
enum Visible {
    /// All of them, from the group's own buffer.
    #[default]
    All,
    /// A rigid group's visible instances, copied in order into the pass's buffer at `offset`.
    Packed { offset: u64, count: u32 },
    /// A skinned group's visible instances (each is drawn on its own with its palette).
    Indices(Vec<u32>),
    /// A mesh with several detail levels: what each level draws (`Packed` or `Indices`).
    Levels(Vec<Visible>),
}

impl Visible {
    /// What one detail level draws. A group not split by level draws only level 0 (also a
    /// mesh with several levels before its first cull).
    fn level(&self, level: usize) -> Option<&Visible> {
        match self {
            Visible::Levels(lists) => lists.get(level),
            other => (level == 0).then_some(other),
        }
    }
}

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
    /// The detail level (mesh part) the range belongs to.
    pub level: usize,
}

/// A mesh (all its detail levels) and the transforms of its instances.
pub struct ModelGroup {
    pub name: String,
    /// The mesh's material set and material 0 override.
    pub material: GroupMaterial,
    pub ranges: Vec<RangeData>,
    /// The distance up to which each detail level is drawn (one per mesh part).
    pub levels: Vec<f32>,
    /// The mesh's bounding box (`GraphicsMesh +0x80` and `+0x8c`), smallest corner first.
    pub bounds: ([f32; 3], [f32; 3]),
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
#[derive(Clone)]
pub struct ModelInstance {
    pub matrix: [f32; 16],
    pub locals: Option<Vec<Transform>>,
}

struct ModelAsset {
    ranges: Vec<RangeData>,
    levels: Vec<f32>,
    bounds: ([f32; 3], [f32; 3]),
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
            levels: self.levels,
            bounds: self.bounds,
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
                        unbanished_formats::skeleton::palette(&self.bones, &placed_pose(locals))
                    }
                    _ => self.default_palette.clone(),
                };
                self.skins
                    .extend(unbanished_gfx::skin::palette_matrices(&palette));
            }
            self.instances = instances.iter().map(|i| i.matrix).collect();
        } else {
            self.instances = self.matrices(&instances);
        }
    }

    fn matrices(&self, instances: &[ModelInstance]) -> Vec<[f32; 16]> {
        instances
            .iter()
            .map(|instance| {
                let Some(locals) = &instance.locals else {
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
    /// Each pass's visible rigid instances, packed group after group, and the scratch list
    /// they are packed in.
    pass_buffers: [Option<wgpu::Buffer>; 3],
    packed: Vec<[f32; 16]>,
    /// Uploaded textures by their image data's address. The scene's groups keep their images
    /// until the next full upload, which clears this.
    views: HashMap<usize, wgpu::TextureView>,
}

impl ModelRenderer {
    /// A buffer of per-instance palettes and its bind group (`None` without palettes).
    fn skin_buffer(
        &self,
        device: &wgpu::Device,
        skins: &[[f32; 16]],
    ) -> Option<(wgpu::Buffer, wgpu::BindGroup)> {
        skin_buffer(device, &self.skin_layout, skins)
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
                size: wgpu::BufferSize::new(unbanished_gfx::skin::PALETTE_BYTES),
            }),
        }],
    });
    Some((buffer, bind))
}
