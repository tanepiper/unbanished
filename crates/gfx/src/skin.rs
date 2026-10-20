//! GPU skinning shared by the viewer and the game: the `ModelSkinLayout` vertex attributes, the
//! bone palette uniform and its WGSL. The original's `NormalSkin_*vs` programs blend four bones
//! from a palette of transforms (`tc_transform`, `c41`, four rows per bone).

use unbanished_formats::skeleton::Transform;

/// Bones per palette: the original's `tc_transform` is 208 constants from `c41`, four per bone
/// (`NormalSkin_vs`), so 52; the largest shipped skeleton has 22, a mod's may have more.
pub const MAX_BONES: usize = 52;
/// Bytes of one palette (`MAX_BONES` column-major 4x4 matrices); a multiple of 256 so palettes
/// can be addressed with dynamic uniform offsets.
pub const PALETTE_BYTES: u64 = (MAX_BONES * 64) as u64;

/// WGSL for the palette at `@group(2) @binding(0)` and the four-bone blend.
pub const SKIN_WGSL: &str = r#"
const MAX_BONES: u32 = 52u;
struct Skin {
    bones: array<mat4x4<f32>, MAX_BONES>,
};
@group(2) @binding(0) var<uniform> skin: Skin;

fn skin_matrix(weights: vec4<f32>, bones: vec4<u32>) -> mat4x4<f32> {
    return skin.bones[bones.x] * weights.x + skin.bones[bones.y] * weights.y
         + skin.bones[bones.z] * weights.z + skin.bones[bones.w] * weights.w;
}

// A skinned point with w = 1. The shipped weights sum to 253..255/255, so the blended matrix's w
// is not 1; the original blends `R·p + t` per bone and projects the result as a point.
fn skin_point(skin_m: mat4x4<f32>, p: vec3<f32>) -> vec4<f32> {
    return vec4<f32>((skin_m * vec4<f32>(p, 1.0)).xyz, 1.0);
}
"#;

/// `ModelSkinLayout` (32-byte vertices): i16x4 position (/512), i16x4 texcoords, unorm8x4 normal,
/// 4 unused bytes, unorm8x4 weights, u8x4 bone indices, at shader locations 0..4.
pub fn skinned_attributes() -> [wgpu::VertexAttribute; 5] {
    let at = |format, offset, shader_location| wgpu::VertexAttribute {
        format,
        offset,
        shader_location,
    };
    use wgpu::VertexFormat::{Sint16x4, Uint8x4, Unorm8x4};
    [
        at(Sint16x4, 0, 0),
        at(Sint16x4, 8, 1),
        at(Unorm8x4, 16, 2),
        at(Unorm8x4, 24, 3),
        at(Uint8x4, 28, 4),
    ]
}

/// The palette as `MAX_BONES` matrices (identity padded; extra bones are dropped with a warning).
pub fn palette_matrices(palette: &[Transform]) -> Vec<[f32; 16]> {
    if palette.len() > MAX_BONES {
        log::warn!(
            "{} bones exceed the palette size {MAX_BONES}",
            palette.len()
        );
    }
    let mut m: Vec<[f32; 16]> = palette
        .iter()
        .take(MAX_BONES)
        .map(Transform::to_matrix)
        .collect();
    m.resize(MAX_BONES, Transform::IDENTITY.to_matrix());
    m
}

/// The palette's bind group layout; `dynamic` palettes are addressed per instance by offset.
pub fn skin_layout(device: &wgpu::Device, dynamic: bool) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("skin"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: dynamic,
                min_binding_size: wgpu::BufferSize::new(PALETTE_BYTES),
            },
            count: None,
        }],
    })
}
