//! Terrain decals: their shader, their meshes, and drawing them.

use super::*;

pub(super) const DECAL_WGSL: &str = r#"
@group(1) @binding(0) var diffuse: texture_2d<f32>;
@group(1) @binding(1) var mask: texture_2d<f32>;
@group(1) @binding(2) var snow_tex: texture_2d<f32>;
@group(1) @binding(3) var samp: sampler;

struct DecalIn {
    @location(0) pos: vec4<i32>,
    @location(1) uv: vec4<i32>,
    @location(2) normal: vec4<f32>,
};

struct DecalOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) t0: vec4<f32>,
    // x diffuse, y ambient factor, z fog, w vertex alpha.
    @location(1) t1: vec4<f32>,
    // xyz world (Blend triplanar), w underwater factor.
    @location(2) t2: vec4<f32>,
    // Blend weights |n|^4.
    @location(3) t3: vec3<f32>,
    // Sun shadow map coordinate.
    @location(4) sh: vec3<f32>,
};

@vertex
fn vs_decal(v: DecalIn) -> DecalOut {
    var o: DecalOut;
    let n = normalize(v.normal.xyz * 2.0 - 1.0);
    let d = dot(n, g.light_dir.xyz);
    let world = vec3<f32>(f32(v.pos.x), f32(v.pos.y), f32(v.pos.z) * 0.00390625);
    o.clip = g.view_proj * vec4<f32>(world, 1.0);
    o.t1 = vec4<f32>(-d, (d + 3.0) * 0.5, (o.clip.w + g.fog.x) * g.fog.y,
                     f32(v.pos.w) * 0.00392156886);
    o.t0 = vec4<f32>(vec2<f32>(v.uv.xy), vec2<f32>(v.uv.zw));
    o.t2 = vec4<f32>(world.x * 0.0625, world.y * 0.0625, f32(v.pos.z) * 0.000244140625,
                     (f32(v.pos.z) * 0.00390625 + 0.75) * -0.666666687);
    let n2 = n * n;
    o.t3 = n2 * n2;
    o.sh = shadow_coord(world);
    return o;
}

fn light(base: vec3<f32>, t1: vec4<f32>, sh: vec3<f32>) -> vec3<f32> {
    let l = g.ambient_color.rgb * t1.y
        + g.light_color.rgb * sun_shadow(sh) * clamp(t1.x, 0.0, 1.0);
    let lit = base * l;
    let fog = min(g.fog_color.w, clamp(t1.z, 0.0, 1.0));
    return lit + fog * (g.fog_color.rgb - lit);
}

// TerrainTiledSnowDecalMaterial_ps: diffuse at uv/16 (its alpha marks where snow settles), the
// tile mask's green at zw/4096 gives coverage; darkened under water.
@fragment
fn fs_tiled(i: DecalOut) -> @location(0) vec4<f32> {
    let uv = i.t0.xy * 0.0625;
    let base = textureSample(diffuse, samp, uv);
    let s = textureSample(snow_tex, samp, uv);
    let c = mix(base.rgb, s.rgb, base.w * g.user.x);
    let lit = light(c, i.t1, i.sh);
    let under = clamp(i.t2.w, 0.0, 1.0);
    let m = textureSample(mask, samp, i.t0.zw * 0.000244140625);
    return vec4<f32>(lit * (1.0 - under), m.y * i.t1.w);
}

// TerrainDecalBlendMaterial_ps: the footprint texture over a triplanar ground texture.
@fragment
fn fs_blend(i: DecalOut) -> @location(0) vec4<f32> {
    let uv = i.t0.xy * 0.000244140625;
    let base = textureSample(diffuse, samp, uv);
    let s = textureSample(snow_tex, samp, uv);
    let px = textureSample(mask, samp, i.t2.yz);
    let py = textureSample(mask, samp, i.t2.xz);
    let pz = textureSample(mask, samp, uv.yx);
    let ground = px.rgb * i.t3.x + py.rgb * i.t3.y + pz.rgb * i.t3.z;
    let c0 = mix(ground, base.rgb, base.w);
    let c = mix(c0, s.rgb, i.t3.z * g.user.x);
    return vec4<f32>(light(c, i.t1, i.sh), pz.w);
}

// TerrainDecalMaterial_ps.
@fragment
fn fs_plain(i: DecalOut) -> @location(0) vec4<f32> {
    let uv = i.t0.xy * 0.000244140625;
    let base = textureSample(diffuse, samp, uv);
    let lit = light(base.rgb, i.t1, i.sh);
    let under = clamp(i.t2.w, 0.0, 1.0);
    return vec4<f32>(mix(lit, vec3<f32>(0.0250000004, 0.123000003, 0.189999998), under),
                     base.w * i.t1.w);
}
"#;

pub(super) struct DecalGpu {
    kind: DecalKind,
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    index_count: u32,
    bind: wgpu::BindGroup,
}

impl SceneExtrasRenderer {
    /// The decals alone (the game's roads and footprints change as the town grows).
    pub fn upload_decals(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        decals: &[DecalData],
    ) {
        let mut views: std::collections::HashMap<usize, wgpu::TextureView> = Default::default();
        let white = self.white.clone();
        // One upload per shared texture (`DecalCache`), keyed by its allocation.
        let mut view = |img: &crate::view::scene_assets::DecalImage| -> wgpu::TextureView {
            let Some(img) = img else {
                return white.clone();
            };
            views
                .entry(std::sync::Arc::as_ptr(img) as usize)
                .or_insert_with(|| linear_view(device, queue, img, &white))
                .clone()
        };
        self.decals = decals
            .iter()
            .map(|d: &DecalData| {
                let (diffuse, mask, snow) = (view(&d.diffuse), view(&d.mask), view(&d.snow));
                let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("decal"),
                    layout: &self.decal_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(&diffuse),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::TextureView(&mask),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::TextureView(&snow),
                        },
                        wgpu::BindGroupEntry {
                            binding: 3,
                            resource: wgpu::BindingResource::Sampler(&self.wrap),
                        },
                    ],
                });
                DecalGpu {
                    kind: d.kind,
                    vertices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("decal vertices"),
                        contents: &d.vertices,
                        usage: wgpu::BufferUsages::VERTEX,
                    }),
                    indices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("decal indices"),
                        contents: &d.indices,
                        usage: wgpu::BufferUsages::INDEX,
                    }),
                    index_count: d.index_count,
                    bind,
                }
            })
            .collect();
    }

    pub fn draw_decals<'p>(
        &'p self,
        pass: &mut wgpu::RenderPass<'p>,
        globals: &'p wgpu::BindGroup,
    ) {
        if self.decals.is_empty() {
            return;
        }
        pass.set_bind_group(0, globals, &[]);
        let mut current = None;
        for d in &self.decals {
            if current != Some(d.kind) {
                let i = match d.kind {
                    DecalKind::Tiled => 0,
                    DecalKind::Blend => 1,
                    DecalKind::Plain => 2,
                };
                pass.set_pipeline(&self.decal_pipelines[i]);
                current = Some(d.kind);
            }
            pass.set_bind_group(1, &d.bind, &[]);
            pass.set_vertex_buffer(0, d.vertices.slice(..));
            pass.set_index_buffer(d.indices.slice(..), wgpu::IndexFormat::Uint16);
            pass.draw_indexed(0..d.index_count, 0, 0..1);
        }
    }
}
