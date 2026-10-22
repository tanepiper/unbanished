//! Rivers and lakes: their shader, their meshes, and drawing them.

use super::*;

pub(super) const WATER_WGSL: &str = r#"
@group(1) @binding(0) var water_tex: texture_2d<f32>;
@group(1) @binding(1) var frame_tex: texture_2d<f32>;
@group(1) @binding(2) var reflection_tex: texture_2d<f32>;
@group(1) @binding(3) var highlight: texture_cube<f32>;
@group(1) @binding(4) var wrap: sampler;
@group(1) @binding(5) var clamp_s: sampler;

struct WaterIn {
    @location(0) pos: vec4<i32>,
    @location(1) uv: vec2<i32>,
    @location(2) normal: vec4<f32>,
};

struct WaterOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) t0: vec4<f32>,
    @location(1) view: vec3<f32>,
    @location(2) screen: vec4<f32>,
    // x: -L.z, y: L.z/2 + 1.5, z: fog, w: alpha.
    @location(3) t3: vec4<f32>,
    // Sun shadow map coordinate (t5).
    @location(4) sh: vec3<f32>,
};

fn water_common(v: WaterIn) -> WaterOut {
    var o: WaterOut;
    let world = vec3<f32>(f32(v.pos.x) * 0.020833334, f32(v.pos.y) * 0.020833334,
                          f32(v.pos.z) * 0.00390625);
    o.view = world - g.camera_pos.xyz;
    let clip = g.view_proj * vec4<f32>(world, 1.0);
    o.clip = clip;
    o.screen = vec4<f32>(0.5 * clip.x + 0.5 * clip.w, -0.5 * clip.y + 0.5 * clip.w, clip.z, clip.w);
    o.t3 = vec4<f32>(-g.light_dir.z, 0.5 * g.light_dir.z + 1.5,
                     (clip.w + g.fog.x) * g.fog.y, v.normal.z);
    o.sh = shadow_coord(world);
    return o;
}

// RiverMaterial_vs: both normal-map layers flow along v.
@vertex
fn vs_river(v: WaterIn) -> WaterOut {
    var o = water_common(v);
    let uv = vec2<f32>(v.uv) * 0.0009765625;
    let t = g.camera_forward.w;
    o.t0 = vec4<f32>(uv.x, uv.y + t * 0.02, uv.x, uv.y + t * 0.01);
    return o;
}

// RiverMaterial_Normal_lakevs: lakes drift slowly in two directions.
@vertex
fn vs_lake(v: WaterIn) -> WaterOut {
    var o = water_common(v);
    let uv = vec2<f32>(v.uv) * 0.0009765625;
    let t = g.camera_forward.w;
    o.t0 = vec4<f32>(uv.x, uv.y - 0.0057 * t, uv.x + 0.005 * t, uv.y + 0.01 * t);
    return o;
}

// RiverMaterial_ps.
@fragment
fn fs_water(i: WaterOut) -> @location(0) vec4<f32> {
    let a = textureSample(water_tex, wrap, i.t0.xy);
    let b = textureSample(water_tex, wrap, i.t0.zw);
    let d = vec2<f32>(a.w * 2.0 - 1.0 + b.w * 2.0 - 1.0, a.y * 2.0 - 1.0 + b.y * 2.0 - 1.0);
    let h = d * 0.5;
    let n = vec3<f32>(h, 1.0 - length(h));
    let v = normalize(i.view);
    let r = v - 2.0 * dot(v, n) * n;
    let screen = i.screen.xy / i.screen.w;
    let refl_uv = d * 0.3 + screen;
    let frame_uv = d * 0.025 + screen;
    let spec = textureSample(highlight, wrap, r).rgb * 1.5;
    let frame = textureSample(frame_tex, clamp_s, frame_uv);
    let refl = textureSample(reflection_tex, clamp_s, refl_uv);
    let nn = vec3<f32>(d * 2.0, n.z);
    let f = dot(-v, nn);
    let reflected = spec * refl.w + refl.rgb;
    let f2 = f * f;
    // Program 2 sums its four shadow taps against a quarter of program 0's constant.
    let sun = vec3<f32>(0.165, 0.1815, 0.165) * sun_shadow(i.sh);
    let colour = reflected * ((1.0 - f2) * 0.67) + (frame.rgb * (f2 * 0.67) + sun);
    let fog = min(g.fog_color.w, clamp(i.t3.z, 0.0, 1.0));
    let fogged = mix(colour, g.fog_color.rgb, fog);
    // The terrain wrote its underwater factor to alpha: shallow shores show the ground.
    return vec4<f32>(mix(frame.rgb, fogged, frame.w), i.t3.w);
}
"#;

pub(super) struct WaterGpu {
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    rivers: Vec<[i32; 4]>,
    lakes: Vec<[i32; 4]>,
    pub(super) river_tex: wgpu::TextureView,
    pub(super) lake_tex: wgpu::TextureView,
    pub(super) highlight: wgpu::TextureView,
    pub(super) river_bind: Option<wgpu::BindGroup>,
    pub(super) lake_bind: Option<wgpu::BindGroup>,
    pub level: f32,
}

impl SceneExtrasRenderer {
    pub fn water_level(&self) -> Option<f32> {
        self.water.as_ref().map(|w| w.level)
    }

    pub(super) fn upload_water(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        w: &WaterData,
    ) -> WaterGpu {
        let view2d = |img: &ImageBuffer| {
            unbanished_gfx::upload_image_linear(device, queue, "water", img)
                .unwrap_or_else(|| self.white.clone())
        };
        let highlight = unbanished_gfx::upload_cube(device, queue, "highlight", &w.highlight)
            .unwrap_or_else(|| {
                let t = device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("highlight"),
                    size: wgpu::Extent3d {
                        width: 1,
                        height: 1,
                        depth_or_array_layers: 6,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                });
                t.create_view(&wgpu::TextureViewDescriptor {
                    dimension: Some(wgpu::TextureViewDimension::Cube),
                    ..Default::default()
                })
            });
        WaterGpu {
            vertices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("water vertices"),
                contents: &w.vertices,
                usage: wgpu::BufferUsages::VERTEX,
            }),
            indices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("water indices"),
                contents: &w.indices,
                usage: wgpu::BufferUsages::INDEX,
            }),
            rivers: w.rivers.clone(),
            lakes: w.lakes.clone(),
            river_tex: view2d(&w.river_water),
            lake_tex: view2d(&w.lake_water),
            highlight,
            river_bind: None,
            lake_bind: None,
            level: w.level,
        }
    }

    pub fn draw_water<'p>(&'p self, pass: &mut wgpu::RenderPass<'p>, globals: &'p wgpu::BindGroup) {
        let Some(w) = &self.water else { return };
        let (Some(river), Some(lake)) = (&w.river_bind, &w.lake_bind) else {
            return;
        };
        pass.set_bind_group(0, globals, &[]);
        pass.set_index_buffer(w.indices.slice(..), wgpu::IndexFormat::Uint16);
        for (pipeline, bind, ranges) in [
            (&self.river_pipeline, river, &w.rivers),
            (&self.lake_pipeline, lake, &w.lakes),
        ] {
            pass.set_pipeline(pipeline);
            pass.set_bind_group(1, bind, &[]);
            for &[first_index, first_vertex, vertex_count, index_count] in ranges {
                if index_count <= 0 || vertex_count <= 0 {
                    continue;
                }
                // Indices are local to the range; bind its vertices at an offset (no base vertex
                // on WebGL2).
                let start = first_vertex as u64 * 16;
                let end = start + vertex_count as u64 * 16;
                pass.set_vertex_buffer(0, w.vertices.slice(start..end));
                let first = first_index as u32;
                pass.draw_indexed(first..first + index_count as u32, 0, 0..1);
            }
        }
    }
}
