//! CPU-side description of a `GraphicsMesh`'s draw ranges, shared by the viewer and the game:
//! which material each range uses, which shader kind that is, its textures and its rigid node
//! transform. GPU upload and skinning strategy stay with each app.

use banished_formats::skeleton::{local_bind, world_transforms, Transform};
use banished_formats::types::{DrawRange, GraphicsMesh, MaterialInstance, MeshPart};

use crate::material::MaterialKind;

/// Everything about one draw range except its geometry bytes.
#[derive(Debug, Clone)]
pub struct RangeMaterial {
    pub kind: MaterialKind,
    pub diffuse: Option<String>,
    pub snow: Option<String>,
    /// The damaged materials' `damage` texture (`OpaqueSnowDamageMaterial` and the like).
    pub damage: Option<String>,
    /// Skinned ranges (`bone < 0`) go through the skin palette; rigid ones use `rigid`.
    pub skinned: bool,
    /// Rigid node index (`bone >= 0`).
    pub rigid_bone: Option<usize>,
    /// The rigid node's bind world transform (identity for skinned ranges).
    pub rigid: Transform,
}

/// The material set a mesh is drawn with (falls back to the first).
pub fn material_set(mesh: &GraphicsMesh, set: usize) -> Option<&[Option<String>]> {
    mesh.material_instances
        .get(set)
        .or_else(|| mesh.material_instances.first())
        .map(Vec::as_slice)
}

/// Describe a range: resolve its material instance (`resolve` reads a reference) and pick the
/// shader kind and texture references the ported shaders sample.
pub fn range_material(
    world: &[Transform],
    set: Option<&[Option<String>]>,
    range: &DrawRange,
    resolve: &mut impl FnMut(&str) -> Option<MaterialInstance>,
) -> RangeMaterial {
    let instance = set
        .and_then(|s| s.get(range.material_index as usize).cloned().flatten())
        .and_then(|r| resolve(&r));
    let texture = |slot: &str| {
        instance
            .as_ref()
            .and_then(|mi| mi.texture(slot).map(str::to_owned))
    };
    let skinned = range.bone < 0;
    let rigid_bone = usize::try_from(range.bone).ok();
    RangeMaterial {
        kind: MaterialKind::of(instance.as_ref().and_then(|mi| mi.material.as_deref())),
        diffuse: texture("diffuse"),
        snow: texture("snow"),
        damage: texture("damage"),
        skinned,
        rigid_bone,
        rigid: rigid_bone
            .and_then(|b| world.get(b).copied())
            .unwrap_or(Transform::IDENTITY),
    }
}

/// Bind-pose world transforms of a mesh's nodes.
pub fn bind_world(mesh: &GraphicsMesh) -> Vec<Transform> {
    let locals: Vec<Transform> = mesh.bones.iter().map(local_bind).collect();
    world_transforms(&mesh.bones, &locals)
}

/// The single rigid node all of a part's ranges hang from, if any: such meshes (trees) animate
/// by moving the whole instance with that node.
pub fn animation_bone(part: &MeshPart) -> Option<usize> {
    let mut nodes = part
        .ranges
        .iter()
        .filter(|r| r.index_count > 0)
        .map(|r| r.bone);
    nodes
        .next()
        .filter(|&first| first >= 0 && nodes.all(|bone| bone == first))
        .map(|bone| bone as usize)
}
