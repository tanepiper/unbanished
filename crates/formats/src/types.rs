//! Decoders for individual resource classes.

use crate::crc::type_id;
use crate::crs::{self, Reader};
use crate::error::{format_err, Error, Result};

// ---------------------------------------------------------------- ImageBuffer

/// `Video::ImageBuffer::Format`, mapped to DXGI by the VideoDX11 texture-creation table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFormat {
    Bc1,
    Bc3,
    A8,
    R8,
    Bgra8,
    R32F,
    R24G8,
}

impl ImageFormat {
    fn from_raw(v: u32) -> Result<Self> {
        Ok(match v {
            0 => Self::Bc1,
            1 | 2 => Self::Bc3,
            3 => Self::A8,
            4 => Self::R8,
            5 => Self::Bgra8,
            6 => Self::R32F,
            7 => Self::R24G8,
            _ => return Err(Error::Unsupported(format!("image format {v}"))),
        })
    }

    /// Byte size of one level.
    pub fn level_size(self, w: u32, h: u32) -> usize {
        let (w, h) = (w.max(1) as usize, h.max(1) as usize);
        let blocks = w.div_ceil(4) * h.div_ceil(4);
        match self {
            Self::Bc1 => blocks * 8,
            Self::Bc3 => blocks * 16,
            Self::A8 | Self::R8 => w * h,
            Self::Bgra8 | Self::R32F | Self::R24G8 => w * h * 4,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ImageBuffer {
    pub width: u32,
    pub height: u32,
    /// 0 = 2D; otherwise a cube map (levels are 6 faces x mips).
    pub image_type: u32,
    pub usage: u32,
    pub format: ImageFormat,
    pub generate_mips: bool,
    pub address: [u32; 3],
    pub filter: u32,
    pub source: String,
    pub levels: Vec<Vec<u8>>,
}

impl ImageBuffer {
    pub const CLASS: &'static str = "ImageBuffer";

    /// `Video::ImageBuffer::Serialize` (0x1800f4f40) + `PlatformSerialize` (0x1800f5980).
    pub fn read(r: &mut Reader) -> Result<Self> {
        r.block(|r, _| {
            let width = r.u32()?;
            let height = r.u32()?;
            let image_type = r.u32()?;
            let usage = r.u32()?;
            let format = ImageFormat::from_raw(r.u32()?)?;
            let generate_mips = r.u32()? != 0;
            let address = [r.u32()?, r.u32()?, r.u32()?];
            let filter = r.u32()?;
            let source = r.string()?;
            let mut levels = Vec::new();
            if !matches!(usage, 4 | 5) {
                for _ in 0..r.count()? {
                    let n = r.count()?;
                    levels.push(r.bytes(n)?.to_vec());
                }
            }
            Ok(ImageBuffer {
                width,
                height,
                image_type,
                usage,
                format,
                generate_mips,
                address,
                filter,
                source,
                levels,
            })
        })
    }

    pub fn faces(&self) -> usize {
        if self.image_type == 0 {
            1
        } else {
            6
        }
    }

    pub fn mip_count(&self) -> usize {
        self.levels.len() / self.faces()
    }
}

// ---------------------------------------------------------------- GraphicsMesh

#[derive(Debug, Clone)]
pub struct DrawRange {
    pub start_vertex: i32,
    pub vertex_count: i32,
    pub start_index: i32,
    pub index_count: i32,
    pub base_vertex: i32,
    pub bone: i32,
}

#[derive(Debug, Clone)]
pub struct MeshPart {
    pub lod_distance: f32,
    pub name: String,
    pub ao_texture: Option<String>,
    pub ranges: Vec<DrawRange>,
    /// u16 indices and vertices; static parts use [`STATIC_STRIDE`], skinned parts [`SKINNED_STRIDE`].
    pub indices: Vec<u8>,
    pub vertices: Vec<u8>,
    pub skinned: bool,
}

#[derive(Debug, Clone)]
pub struct Bone {
    pub name: String,
    pub parent: i32,
    pub rotation: [f32; 4],
    pub translation: [f32; 3],
    pub inv_rotation: [f32; 4],
    pub inv_translation: [f32; 3],
}

#[derive(Debug, Clone)]
pub struct GraphicsMesh {
    pub source: String,
    pub parts: Vec<MeshPart>,
    /// Material instance references, one list per material set.
    pub material_instances: Vec<Vec<Option<String>>>,
    pub bones: Vec<Bone>,
    pub bounds_min: [f32; 3],
    pub bounds_max: [f32; 3],
}

/// `ModelLayout`: sint16x4 position, sint16x4 texcoords, unorm8x4 normal, 4 bytes padding.
pub const STATIC_STRIDE: usize = 24;
/// `ModelSkinLayout`: as static, plus unorm8x4 weights at 24 and uint8x4 bone indices at 28.
pub const SKINNED_STRIDE: usize = 32;
/// Positions are stored as int16 and scaled by this in the vertex shader.
pub const POSITION_SCALE: f32 = 1.0 / 512.0;

impl GraphicsMesh {
    pub const CLASS: &'static str = "GraphicsMesh";

    /// `Graphics::GraphicsMesh::Serialize` (0x180101c40) + `PlatformSerialize` (0x1800a5830).
    pub fn read(r: &mut Reader) -> Result<Self> {
        r.block(|r, version| {
            if version == 0 {
                return Err(Error::Unsupported("GraphicsMesh block version 0".into()));
            }
            let source = r.string()?;
            r.f32s::<6>()?; // unused bounds, always (1,1,1)/(-1,-1,-1)
            r.i32()?;
            r.f32s::<2>()?;
            let mut parts = Vec::new();
            for _ in 0..r.count()? {
                parts.push(MeshPart {
                    lod_distance: r.f32()?,
                    name: r.string()?,
                    ao_texture: r.external_ref()?,
                    ranges: Vec::new(),
                    indices: Vec::new(),
                    vertices: Vec::new(),
                    skinned: false,
                });
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
            r.skip_block()?; // DX9 geometry
            r.block(|r, _| {
                r.i32()?;
                for part in &mut parts {
                    for _ in 0..r.count()? {
                        let v = [r.i32()?, r.i32()?, r.i32()?, r.i32()?, r.i32()?, r.i32()?];
                        part.ranges.push(DrawRange {
                            start_vertex: v[0],
                            vertex_count: v[1],
                            start_index: v[2],
                            index_count: v[3],
                            base_vertex: v[4],
                            bone: v[5],
                        });
                    }
                    let mut buffer = || -> Result<Vec<u8>> {
                        r.u8()?;
                        let n = r.u32()? as usize;
                        Ok(r.bytes(n)?.to_vec())
                    };
                    let (si, sv, ki, kv) = (buffer()?, buffer()?, buffer()?, buffer()?);
                    part.skinned = !kv.is_empty();
                    (part.indices, part.vertices) = if part.skinned { (ki, kv) } else { (si, sv) };
                    r.i32()?;
                }
                Ok(())
            })?;
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

// ---------------------------------------------------------------- MaterialInstance

#[derive(Debug, Clone)]
pub struct MaterialInstance {
    pub material: Option<String>,
    pub textures: Vec<(String, Option<String>)>,
    pub vectors: Vec<(String, [f32; 4])>,
}

impl MaterialInstance {
    pub const CLASS: &'static str = "MaterialInstance";

    /// `Graphics::MaterialInstance::Serialize` (0x180106300).
    pub fn read(r: &mut Reader) -> Result<Self> {
        r.block(|r, _| {
            let material = r.external_ref()?;
            let textures = (0..r.count()?)
                .map(|_| Ok((r.string()?, r.external_ref()?)))
                .collect::<Result<_>>()?;
            let vectors = (0..r.count()?)
                .map(|_| Ok((r.string()?, r.f32s()?)))
                .collect::<Result<_>>()?;
            Ok(MaterialInstance {
                material,
                textures,
                vectors,
            })
        })
    }

    pub fn texture(&self, slot: &str) -> Option<&str> {
        self.textures
            .iter()
            .find(|(s, _)| s == slot)
            .and_then(|(_, t)| t.as_deref())
    }
}

// ---------------------------------------------------------------- VertexLayout

#[derive(Debug, Clone)]
pub struct VertexElement {
    pub semantic: String,
    pub stream: i32,
    pub offset: i32,
    pub usage: i32,
    pub usage_index: i32,
    /// 0 float4, 1 float3, 2 float2, 3 float1, 4 sint16x4, 5 sint16x2,
    /// 6 bgra8unorm, 7 uint8x4, 8 unorm8x4 (VideoDX11 DXGI table).
    pub format: i32,
}

#[derive(Debug, Clone)]
pub struct VertexLayout {
    pub elements: Vec<VertexElement>,
    pub dxbc_signature: Vec<u8>,
}

impl VertexLayout {
    pub const CLASS: &'static str = "VertexLayout";

    /// `Video::VertexLayout::Serialize` (0x1800f86c0).
    pub fn read(r: &mut Reader) -> Result<Self> {
        r.block(|r, _| {
            let elements = (0..r.count()?)
                .map(|_| {
                    Ok(VertexElement {
                        semantic: r.string()?,
                        stream: r.i32()?,
                        offset: r.i32()?,
                        usage: r.i32()?,
                        usage_index: r.i32()?,
                        format: r.i32()?,
                    })
                })
                .collect::<Result<_>>()?;
            let n = r.count()?;
            Ok(VertexLayout {
                elements,
                dxbc_signature: r.bytes(n)?.to_vec(),
            })
        })
    }
}

// ---------------------------------------------------------------- dispatch

/// Decode a resource whose class is known statically; checks the type id.
pub fn decode<T>(data: &[u8], class: &str, read: fn(&mut Reader) -> Result<T>) -> Result<T> {
    let (_, value) = crs::parse(data, |r, h| {
        if h.type_id != type_id(class) {
            return format_err(format!(
                "expected {class}, found type id {:#010x}",
                h.type_id
            ));
        }
        read(r)
    })?;
    Ok(value)
}

macro_rules! decode_fn {
    ($name:ident, $ty:ty) => {
        pub fn $name(data: &[u8]) -> Result<$ty> {
            decode(data, <$ty>::CLASS, <$ty>::read)
        }
    };
}

decode_fn!(image_buffer, ImageBuffer);
decode_fn!(graphics_mesh, GraphicsMesh);
decode_fn!(material_instance, MaterialInstance);
decode_fn!(vertex_layout, VertexLayout);
