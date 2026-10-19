//! Reading frames back from the GPU: save previews, screenshots and headless captures.

use super::*;

impl Renderer {
    /// Copy `texture` into a mappable buffer and ask for the mapping.
    pub(super) fn begin_readback(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        texture: &wgpu::Texture,
    ) -> Readback {
        let (width, height) = (texture.width(), texture.height());
        let row = (width * 4).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: (row * height) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(row),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        Readback {
            buffer,
            mapped: Default::default(),
            width,
            height,
            row,
            bgra: matches!(
                texture.format(),
                wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
            ),
        }
    }

    pub(super) fn map(readback: &Readback) {
        let flag = readback.mapped.clone();
        readback
            .buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |r| {
                let v = if r.is_ok() { 1 } else { 2 };
                flag.store(v, std::sync::atomic::Ordering::Release);
            });
    }

    /// Draw `frame` (no UI) offscreen and start reading it and the minimap back.
    pub fn begin_capture(&mut self, frame: &Frame) -> PendingCapture {
        let (w, h) = self.size;
        let target = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("save capture"),
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
        let mut encoder = self.device.create_command_encoder(&Default::default());
        let frame_readback = self.begin_readback(&mut encoder, &target);
        let minimap = self
            .minimap
            .as_ref()
            .map(|m| self.begin_readback(&mut encoder, m));
        self.queue.submit([encoder.finish()]);
        Self::map(&frame_readback);
        if let Some(m) = &minimap {
            Self::map(m);
        }
        PendingCapture {
            frame: frame_readback,
            minimap,
        }
    }

    /// The capture once both copies are mapped: the frame's `width × height` RGBA texels and
    /// the minimap (size, `0x00RRGGBB` texels top row first). An empty frame means it failed.
    pub fn poll_capture(&self, p: &PendingCapture) -> Option<(Vec<u8>, Option<(u32, Vec<u32>)>)> {
        #[cfg(not(target_arch = "wasm32"))]
        let _ = self.device.poll(if self.blocking_readback {
            wgpu::PollType::wait_indefinitely()
        } else {
            wgpu::PollType::Poll
        });
        #[cfg(target_arch = "wasm32")]
        let _ = self.device.poll(wgpu::PollType::Poll);
        let frame = p.frame.take()?;
        let minimap = match &p.minimap {
            Some(m) => {
                let texels = m.take()?;
                (!texels.is_empty()).then(|| {
                    let px = texels
                        .chunks(4)
                        .map(|c| (c[0] as u32) << 16 | (c[1] as u32) << 8 | c[2] as u32)
                        .collect();
                    (m.width, px)
                })
            }
            None => None,
        };
        Some((frame, minimap))
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn capture(&mut self, frame: &Frame) -> Vec<u8> {
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
}

impl Readback {
    /// Rows top first, each texel `[r, g, b, a]`.
    pub(super) fn take(&self) -> Option<Vec<u8>> {
        match self.mapped.load(std::sync::atomic::Ordering::Acquire) {
            0 => None,
            2 => Some(Vec::new()),
            _ => {
                let mapped = self.buffer.slice(..).get_mapped_range().ok()?;
                let mut out = Vec::with_capacity((self.width * self.height * 4) as usize);
                for y in 0..self.height {
                    let start = (y * self.row) as usize;
                    for px in mapped[start..start + (self.width * 4) as usize].chunks(4) {
                        if self.bgra {
                            out.extend([px[2], px[1], px[0], px[3]]);
                        } else {
                            out.extend_from_slice(px);
                        }
                    }
                }
                Some(out)
            }
        }
    }
}
