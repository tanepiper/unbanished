//! Picking geometry: Runtime serializers 1801251f0 / 18007b6f0 / 18007ae20.
use crate::{crs, Result};

/// A triangle of a picking mesh.
#[derive(Debug, Clone)]
pub struct Triangle {
    pub vertices: [u16; 3],
    pub normals: [u16; 3],
}

/// A part of a picking mesh.
#[derive(Debug, Clone)]
pub struct SubMesh {
    pub name: String,
    /// Parent submesh index, used by Runtime AddMesh's bind hierarchy (root is -1).
    pub node: i32,
    pub rotation: [f32; 4],
    pub translation: [f32; 3],
    pub bounds_min: [f32; 3],
    pub bounds_max: [f32; 3],
    pub vertices: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub triangles: Vec<Triangle>,
    pub edges: Vec<[u16; 2]>,
}

/// The simplified mesh the game tests the pointer against, to select an entity.
#[derive(Debug, Clone)]
pub struct PickingMesh {
    pub source: String,
    pub name: String,
    pub skinned: bool,
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
    pub meshes: Vec<Vec<Option<String>>>,
    pub material: Option<String>,
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
    pub meshes: Vec<Vec<Option<String>>>,
    pub mask_material: Option<String>,
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

/// Graphics::EdgeMesh::Serialize 1800a3120. Ranges share GraphicsMesh's six i32 fields.
#[derive(Debug, Clone)]
pub struct EdgeMesh {
    pub references: [Option<String>; 5],
    pub ranges: Vec<[i32; 6]>,
    pub edge_ranges: Vec<[i32; 6]>,
    /// Mask indices/vertices (rigid, skinned), then edge indices/vertices (rigid, skinned).
    pub buffers: [Vec<u8>; 8],
    pub bounds_min: [f32; 3],
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
