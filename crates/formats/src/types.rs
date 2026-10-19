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

/// A texture: its size, format and levels (mip maps, and the six faces of a cube map).
#[derive(Debug, Clone)]
pub struct ImageBuffer {
    /// The width in pixels.
    pub width: u32,
    /// The height in pixels.
    pub height: u32,
    /// 0 = 2D; otherwise a cube map (levels are 6 faces x mips).
    pub image_type: u32,
    /// How the engine uses the texture.
    pub usage: u32,
    /// The pixel format.
    pub format: ImageFormat,
    /// Whether the engine makes the mip levels itself.
    pub generate_mips: bool,
    /// The address (wrap) mode on each axis.
    pub address: [u32; 3],
    /// The filter mode.
    pub filter: u32,
    /// The original image file the texture was made from.
    pub source: String,
    /// The data of each level: faces × mips, face by face.
    pub levels: Vec<Vec<u8>>,
}

impl ImageBuffer {
    /// The resource class name.
    pub const CLASS: &'static str = "ImageBuffer";

    /// `Video::ImageBuffer::Serialize` + `PlatformSerialize`.
    ///
    /// Original: Runtime `0x1800f4f40`, Runtime `0x1800f5980`.
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

    /// The number of faces: 6 for a cube map, else 1.
    pub fn faces(&self) -> usize {
        if self.image_type == 0 {
            1
        } else {
            6
        }
    }

    /// The number of mip levels of each face.
    pub fn mip_count(&self) -> usize {
        self.levels.len() / self.faces()
    }
}

// ---------------------------------------------------------------- GraphicsMesh

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

/// An animation clip: the bones' transforms for each frame, and its events.
#[derive(Debug, Clone)]
pub struct AnimationClip {
    /// The clip's name, for example `Walk`.
    pub name: String,
    /// The mesh it was made for.
    pub mesh_name: String,
    /// The original animation file.
    pub source_name: String,
    /// Whether the clip loops.
    pub looping: bool,
    /// The frame rate.
    pub sample_rate: f32,
    // +0x30/+0x34
    /// Animation: source frame range for loop-point conversion.
    pub start_frame: i32,
    /// The last frame of the source range.
    pub end_frame: i32,
    // +0x38 / +0x48
    /// Declared rate and stored sample interval.
    pub declared_rate: f32,
    /// The time between stored samples.
    pub sample_interval: f32,
    // +0x40
    /// Source frame at which a looping animation resumes.
    pub loop_start_frame: i32,
    // +0x28 / +0x2c
    /// Shared animation-instance configuration.
    pub instance_count: i32,
    /// Whether all entities share one instance of the clip.
    pub instanced: bool,
    /// The frame at which a looping clip starts again.
    pub loop_start: usize,
    /// The number of bones in each frame.
    pub bone_count: usize,
    /// Frame-major quaternion (xyzw) + translation (xyz) samples.
    pub frames: Vec<Vec<crate::skeleton::Transform>>,
    // +0x58: a serialised System::Time (microseconds, Runtime Timer::Startup 0x1800cd030).
    /// The clip's length, in microseconds.
    pub duration_ticks: i64,
    /// The events of the clip.
    pub events: Vec<AnimationEvent>,
}

/// An event in a clip (for example a footstep sound), at a time.
#[derive(Debug, Clone)]
pub struct AnimationEvent {
    /// The frame at which it happens.
    pub frame: i32,
    /// The resource it plays (for example a sound effect).
    pub resource: Option<String>,
}

/// How the game blends from one clip to another.
#[derive(Debug, Clone)]
pub struct AnimationBlend {
    /// The clip that ends.
    pub source: String,
    /// The clip that begins.
    pub target: String,
    /// The blend time.
    pub seconds: f32,
}

/// A group of animation clips for one skeleton.
#[derive(Debug, Clone)]
pub struct AnimationGroup {
    /// The clips.
    pub clips: Vec<AnimationClip>,
    // +0x30; +0x38/+0x40
    /// The blend time between clips that have no blend of their own, in seconds.
    pub default_blend: f32,
    /// The blends between named clips.
    pub blends: Vec<AnimationBlend>,
    // +0x48/+0x50
    /// Precompiled [new clip][previous clip] transition matrix.
    pub blend_matrix: Vec<f32>,
}

impl AnimationGroup {
    /// The resource class name.
    pub const CLASS: &'static str = "AnimationGroup";

    /// `Graphics::AnimationGroup::Serialize` and `Graphics::Animation::Serialize` from the Ghidra
    /// Runtime export. Animation samples are 4 quaternion floats and 3 translation floats;
    /// UpdateNodes interpolates adjacent samples per mesh node.
    ///
    /// Original: Runtime `0x1800fdf30`, Runtime `0x1800fd910`.
    pub fn read(r: &mut Reader) -> Result<Self> {
        r.block(|r, _| {
            let clip_count = r.count()?;
            let mut clips = Vec::with_capacity(clip_count);
            for _ in 0..clip_count {
                let name = r.string()?;
                let mesh_name = r.string()?;
                let source_name = r.string()?;
                let start_frame = r.i32()?;
                let end_frame = r.i32()?;
                let declared_rate = r.f32()?;
                let looping = r.bool()?;
                let loop_start_frame = r.i32()?;
                let instance_count = r.i32()?;
                let instanced = r.bool()?;
                let event_count = r.count()?;
                let events = (0..event_count).map(|_| Ok(AnimationEvent {
                    frame: r.i32()?, resource: r.external_ref()?,
                })).collect::<Result<Vec<_>>>()?;
                let frame_count = r.count()?;
                let sample_rate = r.f32()?;
                let sample_interval = r.f32()?;
                let bone_count = r.count()?;
                let sample_count = r.count()?;
                if sample_count != frame_count.saturating_mul(bone_count) {
                    return Err(crate::error::Error::Format(format!(
                        "animation {name}: {sample_count} samples != {frame_count} frames x {bone_count} bones"
                    )));
                }
                let loop_start = if loop_start_frame == 0 || end_frame == start_frame {
                    0
                } else {
                    ((((loop_start_frame - start_frame) as f32
                        / (end_frame - start_frame) as f32)
                        * frame_count as f32) as usize)
                        .min(frame_count.saturating_sub(1))
                };
                let mut frames = Vec::with_capacity(frame_count);
                for _ in 0..frame_count {
                    let mut frame = Vec::with_capacity(bone_count);
                    for _ in 0..bone_count {
                        frame.push(crate::skeleton::Transform {
                            // Engine quaternions use the inverse convention to our column-vector
                            // transform math, as do the serialized mesh bind rotations.
                            rotation: {
                                let mut q = r.f32s()?;
                                q[..3].iter_mut().for_each(|v| *v = -*v);
                                q
                            },
                            translation: r.f32s()?,
                        });
                    }
                    frames.push(frame);
                }
                let duration_ticks = r.i64()?;
                clips.push(AnimationClip {
                    name,
                    mesh_name,
                    source_name,
                    looping,
                    sample_rate,
                    start_frame,
                    end_frame,
                    declared_rate,
                    sample_interval,
                    loop_start_frame,
                    instance_count,
                    instanced,
                    loop_start,
                    bone_count,
                    frames,
                    duration_ticks,
                    events,
                });
            }
            let blend_count = r.count()?;
            let blends = (0..blend_count).map(|_| Ok(AnimationBlend {
                source: r.string()?, target: r.string()?, seconds: r.f32()?,
            })).collect::<Result<Vec<_>>>()?;
            let default_blend = r.f32()?;
            let blend_matrix_count = r.count()?;
            let blend_matrix = (0..blend_matrix_count).map(|_| r.f32()).collect::<Result<Vec<_>>>()?;
            Ok(AnimationGroup { clips, default_blend, blends, blend_matrix })
        })
    }
}

/// `ModelLayout`: sint16x4 position, sint16x4 texcoords, unorm8x4 normal, 4 bytes padding.
pub const STATIC_STRIDE: usize = 24;
/// `ModelSkinLayout`: as static, plus unorm8x4 weights at 24 and uint8x4 bone indices at 28.
pub const SKINNED_STRIDE: usize = 32;
/// Positions are stored as int16 and scaled by this in the vertex shader.
pub const POSITION_SCALE: f32 = 1.0 / 512.0;

impl GraphicsMesh {
    /// The resource class name.
    pub const CLASS: &'static str = "GraphicsMesh";

    /// Read a mesh. Block version 0 (meshes from older toolkits, in mods) keeps each part's
    /// geometry with the part (the DX9 block, then the DX11 block); later versions keep all of it
    /// in the two platform blocks at the end, the DX11 one led by an integer.
    ///
    /// Original: `Graphics::GraphicsMesh::Serialize` (Runtime `0x180101c40`) and
    /// `PlatformSerialize` (Runtime `0x1800a5830`); the version 0 part geometry, Runtime
    /// `0x1800a55c0`.
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

// ---------------------------------------------------------------- SoundBuffer

/// `WAVEFORMATEX` header carried by every sound. All shipped audio is xWMA (`tag == 0x161`).
#[derive(Debug, Clone, Copy)]
pub struct WaveFormat {
    /// The format tag: `0x161`, WMA v2, for every sound of the game.
    pub tag: u16,
    /// The number of channels.
    pub channels: u16,
    /// The sample rate.
    pub sample_rate: u32,
    /// The average data rate, in bytes per second.
    pub avg_bytes_per_sec: u32,
    /// The packet size in bytes.
    pub block_align: u16,
    /// The bits per sample.
    pub bits_per_sample: u16,
}

/// A sound: its format, its loop flag, its length, and its data in xWMA (or the entry of its
/// stream).
#[derive(Debug, Clone)]
pub struct SoundBuffer {
    /// Whether the sound loops.
    pub looping: bool,
    /// Streamed sounds keep their packets in a raw `.str` package entry named by `stream_entry`.
    pub streamed: bool,
    /// Original source file, e.g. `Audio\Animal\Chicken0.wav`.
    pub source: String,
    /// The package entry of a streamed sound's packets.
    pub stream_entry: String,
    /// The length in seconds.
    pub length_seconds: f32,
    /// The format header.
    pub format: WaveFormat,
    /// Raw `WAVEFORMATEX(TENSIBLE)` bytes (40).
    pub format_bytes: Vec<u8>,
    /// xWMA `dpds` table: cumulative decoded bytes per packet, u32 LE.
    pub packet_table: Vec<u8>,
    /// xWMA packets (empty when streamed).
    pub data: Vec<u8>,
}

impl SoundBuffer {
    /// The resource class name.
    pub const CLASS: &'static str = "SoundBuffer";

    /// `Audio::SoundBuffer::Serialize` + `PlatformSerialize`.
    ///
    /// Original: Runtime `0x1800e2cd0`, Runtime `0x1800e2e10`.
    pub fn read(r: &mut Reader) -> Result<Self> {
        r.block(|r, _| {
            let looping = r.bool()?;
            let streamed = r.bool()?;
            let source = r.string()?;
            let stream_entry = r.string()?;
            let length_seconds = r.f32()?;
            let data_len = r.count()?;
            let table_len = r.count()?;
            let packet_table = r.bytes(table_len)?.to_vec();
            let format_bytes = r.bytes(0x28)?.to_vec();
            let data = r.bytes(data_len)?.to_vec();
            let u16_at = |o: usize| u16::from_le_bytes([format_bytes[o], format_bytes[o + 1]]);
            let u32_at = |o: usize| u32::from_le_bytes(format_bytes[o..o + 4].try_into().unwrap());
            let format = WaveFormat {
                tag: u16_at(0),
                channels: u16_at(2),
                sample_rate: u32_at(4),
                avg_bytes_per_sec: u32_at(8),
                block_align: u16_at(12),
                bits_per_sample: u16_at(14),
            };
            Ok(SoundBuffer {
                looping,
                streamed,
                source,
                stream_entry,
                length_seconds,
                format,
                format_bytes,
                packet_table,
                data,
            })
        })
    }

    /// Wrap as a standard xWMA RIFF file (`RIFF/XWMA` with `fmt `, `dpds`, `data`), which FFmpeg
    /// and XAudio2 tooling read. `stream` supplies the packets for streamed sounds.
    pub fn to_xwma(&self, stream: Option<&[u8]>) -> Vec<u8> {
        fn chunk(out: &mut Vec<u8>, tag: &[u8; 4], body: &[u8]) {
            out.extend_from_slice(tag);
            out.extend_from_slice(&(body.len() as u32).to_le_bytes());
            out.extend_from_slice(body);
            if body.len() % 2 == 1 {
                out.push(0);
            }
        }
        let data = stream.unwrap_or(&self.data);
        let mut body = b"XWMA".to_vec();
        chunk(&mut body, b"fmt ", &self.format_bytes[..18]);
        chunk(&mut body, b"dpds", &self.packet_table);
        chunk(&mut body, b"data", data);
        let mut out = b"RIFF".to_vec();
        out.extend_from_slice(&(body.len() as u32).to_le_bytes());
        out.extend_from_slice(&body);
        out
    }
}

decode_fn!(sound_buffer, SoundBuffer);
