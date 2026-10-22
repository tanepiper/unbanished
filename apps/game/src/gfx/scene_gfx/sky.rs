//! The sky: its shader, its cube mesh, and drawing it.

use super::*;

pub(super) const SKY_WGSL: &str = r#"
@group(1) @binding(0) var clear_cube: texture_cube<f32>;
@group(1) @binding(1) var samp: sampler;
@group(1) @binding(2) var<uniform> model: mat4x4<f32>;

struct SkyOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) dir: vec3<f32>,
};

@vertex
fn vs_sky(@location(0) position: vec4<i32>) -> SkyOut {
    var out: SkyOut;
    let local = vec3<f32>(position.xyz) * 0.001953125;
    out.dir = local;
    let world = model * vec4<f32>(local, 1.0);
    out.clip = g.view_proj * world;
    // Always on the far plane.
    out.clip.z = out.clip.w * 0.99999;
    return out;
}

// SkyMaterial_ps: clear sky from the upper half of the cube, overcast (z mirrored) mixed in by
// snow and rain.
@fragment
fn fs_sky(in: SkyOut) -> @location(0) vec4<f32> {
    let clear = textureSample(clear_cube, samp, in.dir);
    let overcast = textureSample(clear_cube, samp, in.dir * vec3<f32>(1.0, 1.0, -1.0));
    let s = g.user.x * 0.75;
    let t = g.user.w * (1.0 - s) + s;
    return vec4<f32>(mix(clear, overcast, t).rgb, 0.0);
}
"#;

pub(super) struct SkyGpu {
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    index_count: u32,
    model: wgpu::Buffer,
    bind: wgpu::BindGroup,
}

impl SceneExtrasRenderer {
    pub(super) fn upload_sky(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        s: &SkyData,
    ) -> Option<SkyGpu> {
        let cube = unbanished_gfx::upload_cube(device, queue, "sky", &s.cube)?;
        let model = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sky model"),
            size: 64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("sky"),
            layout: &self.sky_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&cube),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.clamp),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: model.as_entire_binding(),
                },
            ],
        });
        Some(SkyGpu {
            vertices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("sky vertices"),
                contents: &s.vertices,
                usage: wgpu::BufferUsages::VERTEX,
            }),
            indices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("sky indices"),
                contents: &s.indices,
                usage: wgpu::BufferUsages::INDEX,
            }),
            index_count: s.index_count,
            model,
            bind,
        })
    }

    /// Centre the sky dome on the camera, scaled to stay inside the far plane.
    pub fn place_sky(&self, queue: &wgpu::Queue, eye: glam::Vec3, far: f32) {
        if let Some(s) = &self.sky {
            let radius = far * 0.9 / 0.5;
            let m = glam::Mat4::from_scale_rotation_translation(
                glam::Vec3::splat(radius),
                glam::Quat::IDENTITY,
                glam::Vec3::new(eye.x, eye.y, eye.z - radius * 0.1),
            );
            queue.write_buffer(&s.model, 0, bytemuck::cast_slice(&m.to_cols_array()));
        }
    }

    pub fn draw_sky<'p>(&'p self, pass: &mut wgpu::RenderPass<'p>, globals: &'p wgpu::BindGroup) {
        let Some(s) = &self.sky else { return };
        pass.set_pipeline(&self.sky_pipeline);
        pass.set_bind_group(0, globals, &[]);
        pass.set_bind_group(1, &s.bind, &[]);
        pass.set_vertex_buffer(0, s.vertices.slice(..));
        pass.set_index_buffer(s.indices.slice(..), wgpu::IndexFormat::Uint16);
        pass.draw_indexed(0..s.index_count, 0, 0..1);
    }
}
