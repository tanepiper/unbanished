//! Picking geometry: the meshes that select and highlight entities.
//!
//! Original: Runtime `0x1801251f0`, `0x18007b6f0`, `0x18007ae20`.
use crate::{crs, Result};

/// A triangle of a picking mesh.
#[derive(Debug, Clone)]
pub struct Triangle {
    /// The indices of its three vertices.
    pub vertices: [u16; 3],
    /// The indices of its three normals.
    pub normals: [u16; 3],
}

/// A part of a picking mesh.
#[derive(Debug, Clone)]
pub struct SubMesh {
    /// The name.
    pub name: String,
    /// Parent submesh index, used by Runtime AddMesh's bind hierarchy (root is -1).
    pub node: i32,
    /// The rotation, a quaternion (x, y, z, w).
    pub rotation: [f32; 4],
    /// The translation.
    pub translation: [f32; 3],
    /// The lowest corner of its bounds.
    pub bounds_min: [f32; 3],
    /// The highest corner of its bounds.
    pub bounds_max: [f32; 3],
    /// The vertices.
    pub vertices: Vec<[f32; 3]>,
    /// The normals.
    pub normals: Vec<[f32; 3]>,
    /// The triangles.
    pub triangles: Vec<Triangle>,
    /// The edges: pairs of vertex indices.
    pub edges: Vec<[u16; 2]>,
}

/// The simplified mesh the game tests the pointer against, to select an entity.
#[derive(Debug, Clone)]
pub struct PickingMesh {
    /// The source file.
    pub source: String,
    /// The name.
    pub name: String,
    /// Whether it follows a skeleton (*inferred*).
    pub skinned: bool,
    /// The parts.
    pub meshes: Vec<SubMesh>,
}

fn shorts<const N: usize>(r: &mut crs::Reader<'_>) -> Result<[u16; N]> {
    let mut out = [0; N];
    for n in &mut out {
        *n = u16::from_le_bytes(r.bytes(2)?.try_into().unwrap());
    }
    Ok(out)
}

/// Read a [`PickingMesh`] from the bytes of its resource.
pub fn picking_mesh(data: &[u8]) -> Result<PickingMesh> {
    crs::parse(data, |r, _| {
        r.block(|r, _| {
            let source = r.string()?;
            let name = r.string()?;
            let skinned = r.bool()?;
            let mut meshes = Vec::new();
            for _ in 0..r.count()? {
                let name = r.string()?;
                let node = r.i32()?;
                let rotation = r.f32s()?;
                let translation = r.f32s()?;
                let bounds_min = r.f32s()?;
                let bounds_max = r.f32s()?;
                let mut vertices = Vec::new();
                for _ in 0..r.count()? {
                    vertices.push(r.f32s()?);
                }
                let mut normals = Vec::new();
                for _ in 0..r.count()? {
                    normals.push(r.f32s()?);
                }
                let mut triangles = Vec::new();
                for _ in 0..r.count()? {
                    triangles.push(Triangle {
                        vertices: shorts(r)?,
                        normals: shorts(r)?,
                    });
                }
                let mut edges = Vec::new();
                for _ in 0..r.count()? {
                    edges.push(shorts(r)?);
                }
                meshes.push(SubMesh {
                    name,
                    node,
                    rotation,
                    translation,
                    bounds_min,
                    bounds_max,
                    vertices,
                    normals,
                    triangles,
                    edges,
                });
            }
            Ok(PickingMesh {
                source,
                name,
                skinned,
                meshes,
            })
        })
    })
    .map(|(_, v)| v)
}

/// A `PickingDescription`: the meshes that select an entity, for each model variant and each
/// construction stage.
///
/// Original: `FUN_14012e1d0`.
#[derive(Debug, Clone)]
pub struct PickingDescription {
    /// The meshes, by model variant, then by construction stage.
    pub meshes: Vec<Vec<Option<String>>>,
    /// The material.
    pub material: Option<String>,
    /// Not identified yet.
    pub flag_38: bool,
}

/// Read a [`PickingDescription`] from the bytes of its resource.
pub fn picking_description(data: &[u8]) -> Result<PickingDescription> {
    crs::parse(data, |r, _| {
        r.block(|r, _| {
            let mut meshes = Vec::new();
            for _ in 0..r.count()? {
                let mut stages = Vec::new();
                for _ in 0..r.count()? {
                    stages.push(r.external_ref()?);
                }
                meshes.push(stages);
            }
            Ok(PickingDescription {
                meshes,
                material: r.external_ref()?,
                flag_38: r.bool()?,
            })
        })
    })
    .map(|(_, v)| v)
}

/// A `HighlightDescription`: the meshes drawn to highlight an entity, in the order of the model's
/// variants and stages.
///
/// Original: `FUN_14004d330`.
#[derive(Debug, Clone)]
pub struct HighlightDescription {
    /// The meshes, by model variant, then by stage.
    pub meshes: Vec<Vec<Option<String>>>,
    /// The material of the mask.
    pub mask_material: Option<String>,
    /// The material of the edges.
    pub edge_material: Option<String>,
}

/// Read a [`HighlightDescription`] from the bytes of its resource.
pub fn highlight_description(data: &[u8]) -> Result<HighlightDescription> {
    crs::parse(data, |r, _| {
        r.block(|r, _| {
            let mut meshes = Vec::new();
            for _ in 0..r.count()? {
                let mut stages = Vec::new();
                for _ in 0..r.count()? {
                    stages.push(r.external_ref()?);
                }
                meshes.push(stages);
            }
            Ok(HighlightDescription {
                meshes,
                mask_material: r.external_ref()?,
                edge_material: r.external_ref()?,
            })
        })
    })
    .map(|(_, v)| v)
}

/// `Graphics::EdgeMesh`: the mask and edge meshes of a highlight. Its ranges have the same six
/// integers as a GraphicsMesh's.
///
/// Original: `Graphics::EdgeMesh::Serialize`, Runtime `0x1800a3120`.
#[derive(Debug, Clone)]
pub struct EdgeMesh {
    /// Five external references; not identified yet.
    pub references: [Option<String>; 5],
    /// The draw ranges of the mask.
    pub ranges: Vec<[i32; 6]>,
    /// The draw ranges of the edges.
    pub edge_ranges: Vec<[i32; 6]>,
    /// Mask indices/vertices (rigid, skinned), then edge indices/vertices (rigid, skinned).
    pub buffers: [Vec<u8>; 8],
    /// The lowest corner of its bounds.
    pub bounds_min: [f32; 3],
    /// The highest corner of its bounds.
    pub bounds_max: [f32; 3],
}

/// Read a [`EdgeMesh`] from the bytes of its resource.
pub fn edge_mesh(data: &[u8]) -> Result<EdgeMesh> {
    crs::parse(data, |r, _| {
        r.block(|r, _| {
            let mut references = [const { None }; 5];
            for v in &mut references {
                *v = r.external_ref()?;
            }
            let mut ranges = Vec::new();
            for _ in 0..r.count()? {
                ranges.push([r.i32()?, r.i32()?, r.i32()?, r.i32()?, r.i32()?, r.i32()?]);
            }
            let mut edge_ranges = Vec::new();
            for _ in 0..r.count()? {
                edge_ranges.push([r.i32()?, r.i32()?, r.i32()?, r.i32()?, r.i32()?, r.i32()?]);
            }
            let mut buffers = std::array::from_fn(|_| Vec::new());
            for b in &mut buffers {
                r.u8()?;
                let count = r.count()?;
                *b = r.bytes(count)?.to_vec();
            }
            Ok(EdgeMesh {
                references,
                ranges,
                edge_ranges,
                buffers,
                bounds_min: r.f32s()?,
                bounds_max: r.f32s()?,
            })
        })
    })
    .map(|(_, v)| v)
}
