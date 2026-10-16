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
    /// Base vertex added to each range-local u16 index.
    pub start_vertex: i32,
    pub vertex_count: i32,
    pub start_index: i32,
    pub index_count: i32,
    pub material_index: i32,
    /// Rigid node transform, or -1 for weighted skinning.
    pub bone: i32,
}

#[derive(Debug, Clone)]
pub struct MeshPart {
    pub lod_distance: f32,
    pub name: String,
    pub ao_texture: Option<String>,
    pub ranges: Vec<DrawRange>,
    /// Both streams can be present in one detail level (for example, the trader boat).
    pub static_indices: Vec<u8>,
    pub static_vertices: Vec<u8>,
    pub skinned_indices: Vec<u8>,
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

#[derive(Debug, Clone)]
pub struct AnimationClip {
    pub name: String,
    pub mesh_name: String,
    pub source_name: String,
    pub looping: bool,
    pub sample_rate: f32,
    /// Animation +0x30/+0x34: source frame range for loop-point conversion.
    pub start_frame: i32,
    pub end_frame: i32,
    /// +0x38 / +0x48: declared rate and stored sample interval.
    pub declared_rate: f32,
    pub sample_interval: f32,
    /// +0x40: source frame at which a looping animation resumes.
    pub loop_start_frame: i32,
    /// +0x28 / +0x2c: shared animation-instance configuration.
    pub instance_count: i32,
    pub instanced: bool,
    pub loop_start: usize,
    pub bone_count: usize,
    /// Frame-major quaternion (xyzw) + translation (xyz) samples.
    pub frames: Vec<Vec<crate::skeleton::Transform>>,
    /// +0x58: serialized System::Time, in microsecond ticks (Timer::Startup 0x1800cd030).
    pub duration_ticks: i64,
    pub events: Vec<AnimationEvent>,
}

#[derive(Debug, Clone)]
pub struct AnimationEvent {
    pub frame: i32,
    pub resource: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AnimationBlend {
    pub source: String,
    pub target: String,
    pub seconds: f32,
}

#[derive(Debug, Clone)]
pub struct AnimationGroup {
    pub clips: Vec<AnimationClip>,
    /// +0x30: fallback blend time; +0x38/+0x40: named blend overrides.
    pub default_blend: f32,
    pub blends: Vec<AnimationBlend>,
    /// +0x48/+0x50: precompiled [new clip][previous clip] transition matrix.
    pub blend_matrix: Vec<f32>,
}

impl AnimationGroup {
    pub const CLASS: &'static str = "AnimationGroup";

    /// `Graphics::AnimationGroup::Serialize` and `Graphics::Animation::Serialize` from the
    /// Ghidra Runtime export (0x1800fdf30 / 0x1800fd910). Animation samples are 4 quaternion
    /// floats and 3 translation floats; UpdateNodes interpolates adjacent samples per mesh node.
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
    pub const CLASS: &'static str = "GraphicsMesh";

    /// `Graphics::GraphicsMesh::Serialize` (0x180101c40) + `PlatformSerialize` (0x1800a5830).
    /// Block version 0 (meshes from older toolkits, in mods) keeps each part's geometry with
    /// the part (`FUN_1800a55c0`: the DX9 then the DX11 block); later versions keep all of it
    /// in the two platform blocks at the end, the DX11 one led by an integer.
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
decode_fn!(animation_group, AnimationGroup);
decode_fn!(material_instance, MaterialInstance);
decode_fn!(vertex_layout, VertexLayout);

// ---------------------------------------------------------------- SoundBuffer

/// `WAVEFORMATEX` header carried by every sound. All shipped audio is xWMA (`tag == 0x161`).
#[derive(Debug, Clone, Copy)]
pub struct WaveFormat {
    pub tag: u16,
    pub channels: u16,
    pub sample_rate: u32,
    pub avg_bytes_per_sec: u32,
    pub block_align: u16,
    pub bits_per_sample: u16,
}

#[derive(Debug, Clone)]
pub struct SoundBuffer {
    pub looping: bool,
    /// Streamed sounds keep their packets in a raw `.str` package entry named by `stream_entry`.
    pub streamed: bool,
    /// Original source file, e.g. `Audio\Animal\Chicken0.wav`.
    pub source: String,
    pub stream_entry: String,
    pub length_seconds: f32,
    pub format: WaveFormat,
    /// Raw `WAVEFORMATEX(TENSIBLE)` bytes (40).
    pub format_bytes: Vec<u8>,
    /// xWMA `dpds` table: cumulative decoded bytes per packet, u32 LE.
    pub packet_table: Vec<u8>,
    /// xWMA packets (empty when streamed).
    pub data: Vec<u8>,
}

impl SoundBuffer {
    pub const CLASS: &'static str = "SoundBuffer";

    /// `Audio::SoundBuffer::Serialize` (0x1800e2cd0) + `PlatformSerialize` (0x1800e2e10).
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
