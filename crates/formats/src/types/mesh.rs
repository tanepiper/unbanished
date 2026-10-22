//! Graphics meshes: parts, draw ranges, bones and the vertex strides.

use super::*;

/// A range of a mesh part that is drawn with one material.
#[derive(Debug, Clone)]
pub struct DrawRange {
    /// Base vertex added to each range-local u16 index.
    pub start_vertex: i32,
    /// The number of vertices.
    pub vertex_count: i32,
    /// The first index.
    pub start_index: i32,
    /// The number of indices.
    pub index_count: i32,
    /// The material of the range, an index into the mesh's material set.
    pub material_index: i32,
    /// Rigid node transform, or -1 for weighted skinning.
    pub bone: i32,
}

/// A part of a mesh, with its level of detail, its draw ranges and its buffers.
#[derive(Debug, Clone)]
pub struct MeshPart {
    /// The distance up to which this level of detail is used.
    pub lod_distance: f32,
    /// The part's name.
    pub name: String,
    /// The ambient-occlusion texture.
    pub ao_texture: Option<String>,
    /// The draw ranges.
    pub ranges: Vec<DrawRange>,
    /// Both streams can be present in one detail level (for example, the trader boat).
    pub static_indices: Vec<u8>,
    /// The vertex buffer of the rigid geometry.
    pub static_vertices: Vec<u8>,
    /// The index buffer of the skinned geometry.
    pub skinned_indices: Vec<u8>,
    /// The vertex buffer of the skinned geometry.
    pub skinned_vertices: Vec<u8>,
}

/// A bone of a skeleton.
#[derive(Debug, Clone)]
pub struct Bone {
    /// The bone's name.
    pub name: String,
    /// The parent bone's index (-1 for the root).
    pub parent: i32,
    /// The rotation relative to the parent (a quaternion, stored conjugated).
    pub rotation: [f32; 4],
    /// The translation relative to the parent.
    pub translation: [f32; 3],
    /// The rotation of the inverse bind transform.
    pub inv_rotation: [f32; 4],
    /// The translation of the inverse bind transform.
    pub inv_translation: [f32; 3],
}

/// A mesh: its parts, its material sets, its skeleton and its bounds.
#[derive(Debug, Clone)]
pub struct GraphicsMesh {
    /// The original model file.
    pub source: String,
    /// The parts.
    pub parts: Vec<MeshPart>,
    /// Material instance references, one list per material set.
    pub material_instances: Vec<Vec<Option<String>>>,
    /// The skeleton's bones (empty for a rigid mesh).
    pub bones: Vec<Bone>,
    /// The smallest corner of the bounding box.
    pub bounds_min: [f32; 3],
    /// The largest corner of the bounding box.
    pub bounds_max: [f32; 3],
}

/// `ModelLayout`: sint16x4 position, sint16x4 texcoords, unorm8x4 normal, 4 bytes padding.
pub const STATIC_STRIDE: usize = 24;

/// `ModelSkinLayout`: as static, plus unorm8x4 weights at 24 and uint8x4 bone indices at 28.
pub const SKINNED_STRIDE: usize = 32;

/// Positions are stored as int16 and scaled by this in the vertex shader.
pub const POSITION_SCALE: f32 = 1.0 / 512.0;

impl MeshPart {
    /// Select the index and vertex stream used by a triangle list.
    pub fn geometry(&self, range: &DrawRange) -> (&[u8], &[u8], usize) {
        if range.bone < 0 {
            (
                &self.skinned_indices,
                &self.skinned_vertices,
                SKINNED_STRIDE,
            )
        } else {
            (&self.static_indices, &self.static_vertices, STATIC_STRIDE)
        }
    }
}

impl GraphicsMesh {
    /// The resource class name.
    pub const CLASS: &'static str = "GraphicsMesh";

    /// Read a mesh. Block version 0 (meshes from older toolkits, in mods) keeps each part's
    /// geometry with the part (the DX9 block, then the DX11 block); later versions keep all of it
    /// in the two platform blocks at the end, the DX11 one led by an integer.
    ///
    /// Original: `Graphics::GraphicsMesh::Serialize` and `PlatformSerialize`; the version 0 part
    /// geometry, Runtime `0x1800a55c0`.
    ///
    /// Original: Runtime `0x180101c40`, Runtime `0x1800a5830`.
    pub fn read(r: &mut Reader) -> Result<Self> {
        /// One part's draw ranges and buffers (DX11).
        fn geometry(r: &mut Reader, part: &mut MeshPart) -> Result<()> {
            for _ in 0..r.count()? {
                let v = [r.i32()?, r.i32()?, r.i32()?, r.i32()?, r.i32()?, r.i32()?];
                part.ranges.push(DrawRange {
                    start_vertex: v[0],
                    vertex_count: v[1],
                    start_index: v[2],
                    index_count: v[3],
                    material_index: v[4],
                    bone: v[5],
                });
            }
            let mut buffer = || -> Result<Vec<u8>> {
                r.u8()?;
                let n = r.u32()? as usize;
                Ok(r.bytes(n)?.to_vec())
            };
            part.static_indices = buffer()?;
            part.static_vertices = buffer()?;
            part.skinned_indices = buffer()?;
            part.skinned_vertices = buffer()?;
            r.i32()?;
            Ok(())
        }
        r.block(|r, version| {
            let source = r.string()?;
            r.f32s::<6>()?; // unused bounds, always (1,1,1)/(-1,-1,-1)
            r.i32()?;
            r.f32s::<2>()?;
            let mut parts = Vec::new();
            for _ in 0..r.count()? {
                let mut part = MeshPart {
                    lod_distance: r.f32()?,
                    name: r.string()?,
                    ao_texture: r.external_ref()?,
                    ranges: Vec::new(),
                    static_indices: Vec::new(),
                    static_vertices: Vec::new(),
                    skinned_indices: Vec::new(),
                    skinned_vertices: Vec::new(),
                };
                if version == 0 {
                    r.skip_block()?; // DX9 geometry
                    r.block(|r, _| geometry(r, &mut part))?;
                }
                parts.push(part);
            }
            let mut material_instances = Vec::new();
            for _ in 0..r.count()? {
                let n = r.count()?;
                material_instances.push((0..n).map(|_| r.external_ref()).collect::<Result<_>>()?);
            }
            let mut bones = Vec::new();
            for _ in 0..r.count()? {
                bones.push(Bone {
                    name: r.string()?,
                    parent: r.i32()?,
                    rotation: r.f32s()?,
                    translation: r.f32s()?,
                    inv_rotation: r.f32s()?,
                    inv_translation: r.f32s()?,
                });
            }
            let bounds_min = r.f32s()?;
            let bounds_max = r.f32s()?;
            if version > 0 {
                r.skip_block()?; // DX9 geometry
                r.block(|r, _| {
                    r.i32()?;
                    for part in &mut parts {
                        geometry(r, part)?;
                    }
                    Ok(())
                })?;
            }
            Ok(GraphicsMesh {
                source,
                parts,
                material_instances,
                bones,
                bounds_min,
                bounds_max,
            })
        })
    }
}
