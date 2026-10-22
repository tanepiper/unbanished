//! Drawing a frame to the window or to a captured image.

use super::*;

/// Everything drawn in one frame.
pub struct Frame<'a> {
    pub clear: wgpu::Color,
    pub mesh: Option<&'a GpuMesh>,
    pub view_proj: Mat4,
    pub eye: Vec3,
    /// Pixel rectangle (x, y, w, h) for the 3D view; the whole target when `None`.
    pub viewport: Option<[f32; 4]>,
    pub ui: Option<(&'a unbanished_ui::Ui, &'a [unbanished_ui::Quad])>,
    pub render_mode: RenderMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderMode {
    Textured,
    Vertices,
    Wireframe,
}

pub const CLEAR_SKY: wgpu::Color = wgpu::Color {
    r: 0.42,
    g: 0.52,
    b: 0.62,
    a: 1.0,
};

pub const CLEAR_DARK: wgpu::Color = wgpu::Color {
    r: 0.075,
    g: 0.08,
    b: 0.09,
    a: 1.0,
};

impl Renderer {
    /// Render one frame offscreen and return it as tightly packed RGBA8.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn capture(&mut self, frame: Frame) -> Vec<u8> {
        let (w, h) = self.size;
        let target = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("capture"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        self.draw(&target.create_view(&Default::default()), frame);
        let row = (w * 4).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        let readback = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: (row * h) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = self.device.create_command_encoder(&Default::default());
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &target,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(row),
                    rows_per_image: Some(h),
                },
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit([encoder.finish()]);
        readback
            .slice(..)
            .map_async(wgpu::MapMode::Read, |r| r.expect("map readback"));
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .expect("poll");
        let mapped = readback.slice(..).get_mapped_range().expect("mapped range");
        let mut out = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            let start = (y * row) as usize;
            out.extend_from_slice(&mapped[start..start + (w * 4) as usize]);
        }
        out
    }

    pub(super) fn draw(&mut self, view: &wgpu::TextureView, frame: Frame) {
        let light = Vec3::new(-0.4, -0.3, -1.0).normalize();
        let globals = Globals {
            view_proj: frame.view_proj.to_cols_array_2d(),
            light_dir: light.extend(frame.render_mode.shader_flag()).to_array(),
            eye: frame
                .eye
                .extend(if frame.render_mode == RenderMode::Textured {
                    1.0
                } else {
                    0.0
                })
                .to_array(),
        };
        self.queue
            .write_buffer(&self.globals, 0, bytemuck::bytes_of(&globals));

        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("main"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(frame.clear),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            if let Some(mesh) = frame.mesh {
                let [x, y, w, h] =
                    frame
                        .viewport
                        .unwrap_or([0.0, 0.0, self.size.0 as f32, self.size.1 as f32]);
                let (x, y) = (x.max(0.0), y.max(0.0));
                let (w, h) = (w.min(self.size.0 as f32 - x), h.min(self.size.1 as f32 - y));
                if w >= 1.0 && h >= 1.0 {
                    pass.set_viewport(x, y, w, h, 0.0, 1.0);
                    pass.set_bind_group(0, &self.globals_group, &[]);
                    pass.set_bind_group(2, &mesh.skin, &[]);
                    for part in &mesh.parts {
                        pass.set_pipeline(if frame.render_mode == RenderMode::Vertices {
                            if part.skinned {
                                &self.skinned_points
                            } else {
                                &self.static_points
                            }
                        } else if frame.render_mode == RenderMode::Wireframe {
                            if part.skinned {
                                &self.skinned_wireframe
                            } else {
                                &self.static_wireframe
                            }
                        } else if part.skinned {
                            &self.skinned_pipeline
                        } else {
                            &self.static_pipeline
                        });
                        pass.set_bind_group(1, &part.textures, &[]);
                        pass.set_vertex_buffer(0, part.vertices.slice(..));
                        if frame.render_mode == RenderMode::Wireframe {
                            pass.set_index_buffer(
                                part.wire_indices.slice(..),
                                wgpu::IndexFormat::Uint16,
                            );
                            pass.draw_indexed(0..part.wire_index_count, part.base_vertex, 0..1);
                        } else {
                            pass.set_index_buffer(
                                part.indices.slice(..),
                                wgpu::IndexFormat::Uint16,
                            );
                            pass.draw_indexed(part.index_range.clone(), part.base_vertex, 0..1);
                        }
                    }
                }
            }
            if let Some((ui, quads)) = frame.ui {
                self.ui
                    .draw(&self.device, &self.queue, &mut pass, ui, quads, self.size);
            }
        }
        self.queue.submit([encoder.finish()]);
    }
}

impl RenderMode {
    pub(super) fn shader_flag(self) -> f32 {
        match self {
            Self::Textured => 0.0,
            Self::Vertices => 1.0,
            Self::Wireframe => 2.0,
        }
    }
}
