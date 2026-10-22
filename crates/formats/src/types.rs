//! Decoders for individual resource classes.

mod animation;
mod image;
mod mesh;
mod sound;
pub use animation::{AnimationBlend, AnimationClip, AnimationEvent, AnimationGroup};
pub use image::{ImageBuffer, ImageFormat};
pub use mesh::{
    Bone, DrawRange, GraphicsMesh, MeshPart, POSITION_SCALE, SKINNED_STRIDE, STATIC_STRIDE,
};
pub use sound::{SoundBuffer, WaveFormat};

use crate::crc::type_id;
use crate::crs::{self, Reader};
use crate::error::{format_err, Error, Result};

// ---------------------------------------------------------------- MaterialInstance

/// A material instance: a base material, and its named textures and values.
#[derive(Debug, Clone)]
pub struct MaterialInstance {
    /// The base material, which selects the shader.
    pub material: Option<String>,
    /// The textures by slot name, for example `diffuse`.
    pub textures: Vec<(String, Option<String>)>,
    /// The named values (four floats each).
    pub vectors: Vec<(String, [f32; 4])>,
}

impl MaterialInstance {
    /// The resource class name.
    pub const CLASS: &'static str = "MaterialInstance";

    /// Original: `Graphics::MaterialInstance::Serialize`, Runtime `0x180106300`.
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

    /// The texture in a named slot, for example `diffuse`.
    pub fn texture(&self, slot: &str) -> Option<&str> {
        self.textures
            .iter()
            .find(|(s, _)| s == slot)
            .and_then(|(_, t)| t.as_deref())
    }
}

// ---------------------------------------------------------------- VertexLayout

/// One attribute of a vertex layout.
#[derive(Debug, Clone)]
pub struct VertexElement {
    /// The semantic, for example `POSITION`.
    pub semantic: String,
    /// The vertex stream.
    pub stream: i32,
    /// The offset in the vertex, in bytes.
    pub offset: i32,
    /// The usage.
    pub usage: i32,
    /// The usage index.
    pub usage_index: i32,
    /// 0 float4, 1 float3, 2 float2, 3 float1, 4 sint16x4, 5 sint16x2,
    /// 6 bgra8unorm, 7 uint8x4, 8 unorm8x4 (VideoDX11 DXGI table).
    pub format: i32,
}

/// A vertex layout: its attributes, and the Direct3D 11 input signature.
#[derive(Debug, Clone)]
pub struct VertexLayout {
    /// The attributes.
    pub elements: Vec<VertexElement>,
    /// The Direct3D 11 input signature.
    pub dxbc_signature: Vec<u8>,
}

impl VertexLayout {
    /// The resource class name.
    pub const CLASS: &'static str = "VertexLayout";

    /// Original: `Video::VertexLayout::Serialize`, Runtime `0x1800f86c0`.
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
        #[doc = concat!("Read a [`", stringify!($ty), "`] from the bytes of its resource.")]
        pub fn $name(data: &[u8]) -> Result<$ty> {
            decode(data, <$ty>::CLASS, <$ty>::read)
        }
    };
}

decode_fn!(image_buffer, ImageBuffer);
decode_fn!(graphics_mesh, GraphicsMesh);
decode_fn!(animation_group, AnimationGroup);
decode_fn!(material_instance, MaterialInstance);
decode_fn!(vertex_layout, VertexLayout);

decode_fn!(sound_buffer, SoundBuffer);
